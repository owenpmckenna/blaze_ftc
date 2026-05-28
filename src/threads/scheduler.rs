use std::ops::Add;
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::{unbounded, Receiver, Sender};
use crate::catch;
use crate::control::hardware::LynxHub;
use crate::serialization::packet::Packet;

static SCHEDULER: OnceLock<Mutex<Scheduler>> = OnceLock::new();
///number of schedulers busy
static SCHEDULER_PACKET_COUNT: AtomicUsize = AtomicUsize::new(0);
///number of schedulers in existence
static SCHEDULER_COUNT: AtomicUsize = AtomicUsize::new(0);
pub fn schedule_packet(when_to_send: Instant, packet: Packet, lynx_hub: &'static LynxHub) {
    let mut scheduler = SCHEDULER.get_or_init(|| Mutex::new(Scheduler::new()))
        .lock().expect("could not lock scheduler lock");
    let packet_count = SCHEDULER_PACKET_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    let num_schedulers = SCHEDULER_COUNT.load(Ordering::SeqCst);
    if num_schedulers < packet_count {
        SCHEDULER_COUNT.fetch_add(1, Ordering::SeqCst);
        scheduler.spawn_new_scheduler(num_schedulers);
    }
    scheduler.packet_tx.send((when_to_send, packet, lynx_hub)).expect("scheduler thread dead!");
}
///this is a kind of "best effort" scheduler.
///Its goal is to allow us to send packets after a wait, without blocking.
///It uses a bad thread pool implementation, more or less. This would be inefficient but threads are cheap,
///and if we make too many they just block.
struct Scheduler {
    packet_tx: Sender<(Instant, Packet, &'static LynxHub)>,
    packet_rx: Receiver<(Instant, Packet, &'static LynxHub)>
}
impl Scheduler {
    fn new() -> Scheduler {
        let (packet_tx, packet_rx) = unbounded();
        Scheduler {packet_tx, packet_rx}
    }
    fn spawn_new_scheduler(&mut self, id: usize) {
        let packet_rx = self.packet_rx.clone();
        thread::spawn(move || {
            catch(move || {
                Self::run_scheduler(packet_rx);
            }, &format!("scheduler thread {}", id));
        });
    }
    fn run_scheduler(packet_rx: Receiver<(Instant, Packet, &'static LynxHub)>) {
        let spin_sleeper = spin_sleep::SpinSleeper::new(100_000)
            .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);
        loop {
            let (ttr, packet, hub) = packet_rx.recv()
                .expect("scheduler rx disconnected?");
            spin_sleeper.sleep_until(ttr);

            hub.send_prepared_packet(packet);

            SCHEDULER_PACKET_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
    }
}