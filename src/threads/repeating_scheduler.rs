
use std::ops::Add;
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::{unbounded, Receiver, Sender};
use spin_sleep::native_sleep;
use thread_priority::{get_current_thread_priority, set_current_thread_priority, ThreadPriority};
use crate::control::hardware::LynxHub;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand;

///This is a relatively simple scheduler for sending hardware reads at specific times to reduce jitter.
///It should be better than the current solution, sending new reads as soon as we get a response to the previous.
///It will need to handle 1-4 Orders with rates anywhere from .5 ms to 20 ms.
///It may drop commands if need be to stay on time.
pub struct RepeatingScheduler {
	current_kill_channel: Sender<()>,
	current_kill_confirm_channel: Receiver<()>,
}
impl RepeatingScheduler {
	pub fn new() -> Self {
		let channels = unbounded();
		Self {
			current_kill_channel: channels.0,
			current_kill_confirm_channel: channels.1,
		}
	}
	fn kill_and_wait(&mut self) {
		let _ = self.current_kill_channel.send(());
		let _ = self.current_kill_confirm_channel.recv();
		let (tx, rx) = unbounded();
		self.current_kill_channel = tx;
		self.current_kill_confirm_channel = rx;
	}
	pub fn set_orders(&mut self, orders: Vec<Order>) -> Option<()> {
		if orders.is_empty() {
			self.kill_and_wait();
			return Some(());
		}
		if orders.len() > 10 {
			return None
		}
		let mut orders_list: Vec<Orders> = Vec::new();
		for i in orders {
			if i.rate < CAREFUL_CHECK_DUR * 2 {
				return None
			}

			let mut add = true;
			for t in &mut orders_list {
				if t.offset == i.offset && t.rate == i.rate {
					t.orders.push(i);
					add = false;
					break;
				}
			}
			if add {
				orders_list.push(Orders::new(i))
			}
		}

		let (kill_tx, kill_rx) = unbounded();
		let (ckill_tx, ckill_rx) = unbounded();
		self.kill_and_wait();
		thread::spawn(move || {
			set_affinity();
			run_timer_thread(orders_list, kill_rx, ckill_tx)
		});
		self.current_kill_channel = kill_tx;
		self.current_kill_confirm_channel = ckill_rx;
		Some(())
	}
}
pub struct Order {
	offset: Duration,
	rate: Duration,
	hub: &'static LynxHub,
	cmd: LynxCommand,
	///sometimes a sensor "hiccups" or similar. writes are fire-and-forget so we can be informed to pause
	///for a few ms for the system to recover. Only some systems will need this so it's optional
	pause_receiver: Option<Receiver<Instant>>,
	not_sending_until: Option<Instant>
}
impl Order {
	pub fn new(offset: Duration, rate: Duration, hub: &'static LynxHub, cmd: LynxCommand) -> Self {
		Self {
			offset,
			rate,
			hub,
			cmd,
			pause_receiver: None,
			not_sending_until: None
		}
	}
	pub fn set_pause_receiver(mut self, pause_receiver: Receiver<Instant>) -> Self {
		self.pause_receiver = Some(pause_receiver);
		self
	}
	fn send(&mut self) {
		if let Some(rx) = &self.pause_receiver {
			//user in charge of sending sane waits. We just replace because they may want to resume early.
			while let Ok(instant) = rx.try_recv() {
				self.not_sending_until = Some(instant);
			}
		}
		if let Some(time) = self.not_sending_until {
			if time < Instant::now() {
				//clear the status if time before now. No sense checking it over and over again
				self.not_sending_until = None;
			} else {
				return;
			}
		}
		//send_lynx_packet this is backed by a crossbeam channel - it won't block or panic
		self.hub.send_lynx_packet(self.cmd.clone());
	}
}
struct Orders {
	last_scheduled: Option<Instant>,
	offset: Duration,
	rate: Duration,
	orders: Vec<Order>
}
impl Orders {
	fn new(order: Order) -> Self {
		Self {
			last_scheduled: None,
			offset: order.offset,
			rate: order.rate,
			orders: vec![order]
		}
	}
	fn when_would_run(&self, now: Instant, start: Instant) -> Instant {
		let mut would_run = match self.last_scheduled {
			None => {
				start + self.offset
			}
			Some(it) => {
				it + self.rate
			}
		};
		//Sending each command isn't crucial. We'd rather miss a few and stay on time.
		while would_run < now {
			would_run += self.rate
		}
		would_run
	}
}
fn run_timer_thread(mut orders: Vec<Orders>, kill: Receiver<()>, confirm: Sender<()>) {
	let start = Instant::now();
	for i in &mut orders {
		if i.offset == Duration::from_millis(0) {
			//just do it now
			i.last_scheduled = Some(start);
			for i in &mut i.orders {
				i.send();
			}
		}
	}
	let mut todo = get_next_timer(start, &mut orders);
	let mut timer_overrun = false;
	loop {
		if !timer_overrun && todo[0].0 < Instant::now() {
			//ok. something is wrong. This isn't catastrophic, though.
			//log it for the operator and move on. Avoid logging multiple times.
			timer_overrun = true;
			log::error!("Repeating Scheduler Timer Overrun with {} orders!", orders.len())
		}

		while todo[0].0.duration_since(Instant::now()) > Duration::from_millis(10) {
			native_sleep(Duration::from_millis(5));
			//limit the worst case time it takes to reset
			if kill.try_recv() {
				let _ = confirm.send(());
				return;
			}
		}

		for todo in todo {
			if todo.0 > Instant::now() {
				spin_sleep::sleep_until(todo.0);
			}

			for i in &mut orders[todo.1].orders {
				i.send();
			}
		}
		todo = get_next_timer(start, &mut orders);
		if kill.try_recv() {
			let _ = confirm.send(());
			return;
		}
	}
}
const CAREFUL_CHECK_DUR: Duration = Duration::from_micros(250);
const CAREFUL_DROP_DUR: Duration = Duration::from_micros(50);
const DO_CAREFUL_SCHED: bool = true;
fn get_next_timer(start: Instant, orders: &mut Vec<Orders>) -> Vec<(Instant, usize)> {
	let now = Instant::now();
	let mut best_id = 0;
	let mut soonest_to_run = Instant::now().add(Duration::from_secs(60 * 60));
	for (id, order) in orders.iter().enumerate() {
		let would_run = order.when_would_run(now, start);
		if would_run < soonest_to_run {
			soonest_to_run = would_run;
			best_id = id
		}
	}
	orders[best_id].last_scheduled = Some(soonest_to_run);
	if !DO_CAREFUL_SCHED {
		return vec![(soonest_to_run, best_id)];
	}

	let mut to_run = vec![(soonest_to_run, best_id)];
	let mut to_run_tmp: Vec<(Instant, usize)> = Vec::with_capacity(orders.len());
	for (id, order) in orders.iter().enumerate() {
		if id == best_id {
			continue;
		}
		let when = order.when_would_run(now, start);
		if when < soonest_to_run + CAREFUL_CHECK_DUR {
			to_run.push((when, id));
		}
	}
	if to_run.len() > 2 {
		to_run[1..].sort_by_key(|it| it.0);
	}
	for i in 0..to_run.len() {
		//if the next one is far enough away, copy everything earlier to the real buffer
		if to_run[i + 1].0 - to_run[i].0 > CAREFUL_DROP_DUR {
			for i in to_run[0..i+1].into_iter() {
				to_run_tmp.push(*i);
			}
			break;
		}
	}
	to_run_tmp
}
fn set_affinity() {
	let core_ids = core_affinity::get_core_ids();
	if let Some(core_ids) = &core_ids && core_ids.len() >= 4 {
		//pin to 4th core. target machine (fixed hw) has 4 cores. linux/android
		let worked = core_affinity::set_for_current(core_ids[3]);
		log::info!("scheduler thread just attempted to pin to core {}. return: {}", core_ids[3].id, worked);
	} else {
		log::info!("scheduler thread just failed to pin to core. {}", core_ids);
	}
	log::info!("just set scheduler thread priority: old: {:?} err: {:?}, new: {:?}",
                    get_current_thread_priority().expect("could not get scheduler priority read"),
                    set_current_thread_priority(ThreadPriority::Max),
                    get_current_thread_priority().expect("could not get scheduler priority read - 2"),
                );
}
