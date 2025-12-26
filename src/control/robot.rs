use std::backtrace::Backtrace;
use std::cell::{Ref, RefCell};
use std::fmt::Debug;
use std::mem::discriminant;
use std::panic::{catch_unwind, RefUnwindSafe, UnwindSafe};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use arc_swap::ArcSwap;
use crossbeam_channel::{Sender, Receiver, select, unbounded};
use crate::{catch, RUNNING};
use crate::control::gamepad::Gamepad;
use crate::control::hardware::LynxHub;
use crate::serialization::command::Command::Ack;
use crate::serialization::command_utils::Module;
use crate::serialization::lynx_commands::lynx_commands::LynxGetBulkDataResponseData;
use crate::serialization::packet::Packet;
use crate::telemetry::telemetry::Telemetry;

pub struct Robot<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    pub hub_0: LynxHub,
    pub hub_1: Option<LynxHub>,//optional expansion hub. not tested
    hub_0_handlers: Vec<Box<Mutex<dyn BulkReadHandler<Target, StateUpdate>>>>,
    hub_1_handlers: Vec<Box<Mutex<dyn BulkReadHandler<Target, StateUpdate>>>>,
    gp_handlers: Vec<Box<Mutex<dyn GamepadHandler<Target, StateUpdate>>>>,
    gamepad_receiver: Receiver<(Vec<u8>, Vec<u8>)>,
    packets_in: Receiver<Packet>,
    pub telemetry: Telemetry,
    initializer: Option<fn(&mut Robot<Target, StateUpdate>) -> Target>,
    handler_target: RwLock<Option<Target>>,
    target_receiver: Option<Receiver<Target>>,
    state_updater: Option<Sender<StateUpdate>>,
    init_update_processors: Vec<fn(&mut MainThread<Target, StateUpdate>, &StateUpdate) -> ()>,
    main_thread_func: Option<fn(&mut MainThread<Target, StateUpdate>) -> ()>,
    running: &'static AtomicBool
}
impl<Target, StateUpdate> Robot<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static,
                                                           StateUpdate: Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    pub fn new(modules: &Vec<Module>, packets_in: Receiver<Packet>, packets_out: Sender<Packet>,
               gamepad_receiver: Receiver<(Vec<u8>, Vec<u8>)>, telemetry: Telemetry,
               initializer: fn(&mut Robot<Target, StateUpdate>) -> Target, running: &'static AtomicBool) -> Robot<Target, StateUpdate> {
        let hub_0 = LynxHub::new(&modules[0], &packets_out);
        let hub_1 = if modules.len() > 1 {
            Some(LynxHub::new(&modules[0], &packets_out))
        } else {None};
        Robot {
            hub_0,
            hub_1,
            hub_0_handlers: vec![],
            hub_1_handlers: vec![],
            gp_handlers: vec![],
            gamepad_receiver,
            packets_in,
            telemetry,
            initializer: Some(initializer),
            handler_target: RwLock::new(None),
            target_receiver: None,
            state_updater: None,
            init_update_processors: vec![],
            main_thread_func: None,
            running
        }
    }
    pub fn init(mut self) {
        let (s_tx, s_rx) = unbounded();
        let (t_tx, t_rx) = unbounded();
        self.state_updater = Some(s_tx);
        self.target_receiver = Some(t_rx);
        let init = self.initializer.expect("no initializer found");
        self.initializer = None;
        log::info!("running init function");
        let default = init(&mut self);
        log::info!("ran init function");
        self.set_target(default.clone());
        let processors = self.init_update_processors;
        self.init_update_processors = vec![];
        let running = self.running;
        let func = self.main_thread_func.take();
        let later_telemetry = self.telemetry.clone();
        thread::spawn(move || {
            catch(move || {
                while self.running.load(Ordering::SeqCst) {
                    self.run()
                }
            }, "main robot thread")
        });
        thread::spawn(move || {
            catch(move || {
                MainThread::new(default, t_tx, s_rx, func, later_telemetry, processors, running)
                    .run();
            }, "robot control thread");
        });
    }
    fn run(&self) {
        select! {
            recv(self.target_receiver.as_ref().unwrap()) -> msg => {
                log::trace!("got new target");
                let data = msg.unwrap();
                self.set_target(data);
            }
            recv(self.gamepad_receiver) -> msg => {
                log::trace!("got new gp");
                let data = msg.unwrap();
                let mut gp0 = Gamepad::new();
                gp0.read_into(data.0.as_slice());
                let mut gp1 = Gamepad::new();
                gp1.read_into(data.1.as_slice());
                for x in 0..self.gp_handlers.len() {
                    Self::catch_user_function(|| self.gp_handlers[x].lock().expect("failed to lock gp0 handler").update(&self, &gp0, &gp1),
                        || format!("gp handler {}", x), &self.telemetry);
                }
            }
            recv(self.packets_in) -> msg => {
                log::trace!("got new packet");
                let data = msg.unwrap();
                match &data.payload_data {
                    Ack(_) => {/*ignore this. that's literally the _entire_ point of this*/},
                    _ => {
                        let data = match self.hub_0.should_consume(&data) {
                            None => {
                                //handle expansion hub ig
                                Some(data)
                            }
                            Some(it) => {
                                for x in 0..self.hub_0_handlers.len() {
                                    Self::catch_user_function(|| self.hub_0_handlers[x].lock().expect("failed to lock hub0 handler").update(&self, &it),
                                        || format!("hub 0 handler {}", x), &self.telemetry);
                                }
                                None
                            }
                        };
                        if data.is_none() || self.hub_1.is_none() {
                            return;
                        }
                        let data = match self.hub_1.as_ref().unwrap().should_consume(&data.unwrap()) {
                            None => {}
                            Some(it) => {
                                for x in 0..self.hub_1_handlers.len() {
                                    Self::catch_user_function(|| self.hub_1_handlers[x].lock().expect("failed to lock hub1 handler").update(&self, &it),
                                        || format!("hub 1 handler {}", x), &self.telemetry);
                                }
                            }
                        };
                    }
                }
            }
        }
    }
    pub fn add_hub_0_handler<D>(&mut self, func: D) where D: BulkReadHandler<Target, StateUpdate> + 'static {
        self.hub_0_handlers.push(Box::new(Mutex::new(func)));
    }
    pub fn add_hub_1_handler<D>(&mut self, func: D) where D: BulkReadHandler<Target, StateUpdate> + 'static {
        self.hub_1_handlers.push(Box::new(Mutex::new(func)));
    }
    pub fn add_gp_handler<D>(&mut self, func: D) where D: GamepadHandler<Target,StateUpdate> + 'static {
        self.gp_handlers.push(Box::new(Mutex::new(func)));
    }
    pub fn add_update_processor(&mut self, func: fn(&mut MainThread<Target, StateUpdate>, &StateUpdate) -> ()) {
        self.init_update_processors.push(func);
    }
    pub fn set_main_thread(&mut self, func: fn(&mut MainThread<Target, StateUpdate>) -> ()) {
        self.main_thread_func = Some(func);
    }
    pub fn target(&self) -> Target {
        self.handler_target.read().expect("could not lock handler target for read").as_ref()
            .expect("no handler target").clone()
    }
    fn set_target(&self, state: Target) {
        let x = self.handler_target.write();
        let mut write = x.expect("could not get handler target lock");
        *write = Some(state);
    }
    pub fn send_state_update(&self, s: StateUpdate) {
        self.state_updater.as_ref().unwrap().send(s).unwrap();
    }
    pub(crate) fn catch_user_function<F, R>(func: F, error_func: R, telemetry: &Telemetry) where F: FnOnce() -> () + UnwindSafe, R: FnOnce() -> String {
        let result = catch_unwind(func);
        match result {
            Ok(_) => {}
            Err(it) => {
                let err = error_func();
                log::error!("error while {}!", err);
                if let Some(s) = it.downcast_ref::<&str>() {
                    log::error!("Caught panic: {}", s);
                } else if let Some(s) = it.downcast_ref::<String>() {
                    log::error!("Caught panic: {}", s);
                } else {
                    log::error!("Caught unknown panic type");
                }
                telemetry.add_string("ERROR DURING FUNCTION (check logcat)", &err);
                sleep(Duration::new(2, 0));//allow telemetry to be sent
                panic!("ERROR DURING FUNCTION: {}", &err);
            }
        }
    }
}
pub trait GamepadHandler<Target, StateUpdate>: Send + UnwindSafe where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    fn update(&mut self, robot: &Robot<Target, StateUpdate>, gp0: &Gamepad, gp1: &Gamepad);
}
pub trait BulkReadHandler<Target, StateUpdate>: Send + UnwindSafe where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    fn update(&mut self, robot: &Robot<Target, StateUpdate>, data: &LynxGetBulkDataResponseData);
}
pub struct MainThread<Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate: Send + UnwindSafe + Sync + RefUnwindSafe + 'static> {
    pub target: Target,
    sender: Sender<Target>,
    receiver: Receiver<StateUpdate>,
    pub state: Vec<StateUpdate>,
    function: Option<fn(&mut Self) -> ()>,
    processors: Option<Vec<fn(&mut MainThread<Target, StateUpdate>, &StateUpdate) -> ()>>,
    pub telemetry: Telemetry,
    running: &'static AtomicBool
}
impl<Target, StateUpdate> MainThread<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static,
                                                                StateUpdate: Send + UnwindSafe + Sync + RefUnwindSafe + 'static + PartialEq + Clone + std::fmt::Debug {
    fn new(default_target: Target, sender: Sender<Target>, receiver: Receiver<StateUpdate>, func: Option<fn(&mut Self) -> ()>, telemetry: Telemetry,  processors: Vec<fn(&mut MainThread<Target, StateUpdate>, &StateUpdate) -> ()>, running: &'static AtomicBool) -> MainThread<Target, StateUpdate> {
        MainThread {target: default_target, sender, receiver, state: vec![], function: func, processors: Some(processors), telemetry, running}
    }
    fn run(&mut self) {
        let func = self.function;
        if let Some(func) = func {
            func(self);
        }
        while self.is_running() {
            self.get_statuses_blocking()
        }
    }
    pub fn set_target(&self) {
        self.sender.send(self.target.clone()).unwrap();
    }
    fn is_valid_fn(&self, data: &[(&StateUpdate, fn(&StateUpdate) -> bool)]) -> bool {
        for status in data {
            let mut found = false;
            for comp in &self.state {
                if discriminant(status.0) == discriminant(comp) {
                    if !status.1(comp) {
                        return false
                    }
                    found = true;
                }
            }
            if !found {
                return false
            }
        }
        true
    }
    fn is_valid(&self, data: &[&StateUpdate]) -> bool {
        for status in data {
            let mut found = false;
            //self.state is a Vec<StatusUpdate>
            for comp in &self.state {
                let stat_disc = discriminant(*status);
                let comp_disc = discriminant(comp);
                if stat_disc == comp_disc {
                    if !(*status).eq(comp) {
                        //log::info!("not valid on status:{:?} because not equal:{:?}. disc0:{:?}, disc1:{:?}",
                        //    status, comp, stat_disc, comp_disc);
                        return false
                    }
                    found = true;
                }
            }
            if !found {
                log::info!("not valid on status:{:?} because not found in state len:{}", status, self.state.len());
                return false
            }
        }
        true
    }
    pub fn process(&mut self, data: StateUpdate) -> StateUpdate {
        //take them out, use them, reinsert them. whyyyyyyy
        let processors = self.processors.take().unwrap();
        for x in &processors {
            x(self, &data);
        }
        let _ = self.processors.insert(processors);

        //check it against stuff in state, shove it in there.
        for i in 0..self.state.len() {
            if discriminant(&self.state[i]) == discriminant(&data){
                self.state[i] = data.clone();
                return data;
            }
        }
        //didn't find anything...
        self.state.push(data.clone());
        data
    }
    pub fn get_statuses(&mut self) -> bool {
        match self.receiver.try_recv() {
            Ok(it) => {
                self.process(it);
                self.get_statuses();//just try again
                true
            }
            Err(_) => {false}
        }
    }
    fn get_statuses_blocking(&mut self) {
        if !self.get_statuses() {//try to get non blocking. if nothing, block.
            self.process(self.receiver.recv().unwrap());
        }
    }
    pub fn wait_for_status(&mut self, status: &[&StateUpdate], status_fn: &[(&StateUpdate, fn(&StateUpdate) -> bool)]) {
        self.get_statuses();
        loop {
            let valid = self.is_valid(status);
            if valid && status.len() > 0 {
                return;
            }
            if self.is_valid_fn(status_fn) && status_fn.len() > 0 {
                return;
            }
            /*if self.state.len() > 1 {
                log::info!("wait for status! d0eq: {}, d1eq: {}, valid: {}, data0: {:?}, data1: {:?}",
                    self.state[0].eq(status[0]), self.state[1].eq(status[1]), valid, self.state[0], self.state[1]);
            } else {
                log::info!("wait for status! status0len: {}, known len: {}", status.len(), self.state.len());
            }*///debugging logic issue
            self.get_statuses_blocking();
        }
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    //TODO turn this partially into a macro
    pub fn get_updated_status(&self, status: &StateUpdate) -> Option<&StateUpdate> {
        for i in 0..self.state.len() {
            if discriminant(&self.state[i]) == discriminant(&status){
                return Some(&self.state[i]);
            }
        }
        None
    }
}