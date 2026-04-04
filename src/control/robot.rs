use std::backtrace::Backtrace;
use std::cell::{Ref, RefCell};
use std::fmt::Debug;
use std::mem::discriminant;
use std::panic::{catch_unwind, panic_any, RefUnwindSafe, UnwindSafe};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use arc_swap::ArcSwap;
use crossbeam_channel::{Sender, Receiver, select, unbounded, never, RecvTimeoutError};
use crate::{catch, PROXY, RUNNING};
use crate::control::gamepad::Gamepad;
use crate::control::hardware::{LynxHub, UnderlyingHw};
use crate::control::hardware::UnderlyingHw::DirectProxy;
use crate::sdk_proxy::proxy::Proxy;
use crate::serialization::command::Command::Ack;
use crate::serialization::command_utils::Module;
use crate::serialization::i2c_comms::i2c_device::{I2CConsumer, I2CDevice, I2CDeviceHandler, I2CDevicePair};
use crate::serialization::lynx_commands::lynx_commands::LynxGetBulkDataResponseData;
use crate::serialization::packet::Packet;
use crate::telemetry::telemetry::Telemetry;

pub struct Robot<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    pub hub_0: &'static LynxHub,
    pub hub_1: Option<&'static LynxHub>,//optional expansion hub. not tested
    hub_0_handlers: Vec<Box<Mutex<dyn BulkReadHandler<Target, StateUpdate>>>>,
    hub_1_handlers: Vec<Box<Mutex<dyn BulkReadHandler<Target, StateUpdate>>>>,
    gp_handlers: Vec<Box<Mutex<dyn GamepadHandler<Target, StateUpdate>>>>,
    gamepad_receiver: &'static Receiver<(Vec<u8>, Vec<u8>)>,
    pub telemetry: &'static Telemetry,
    initializer: Option<fn(&mut Robot<Target, StateUpdate>) -> Target>,
    i2c_devices: Vec<Mutex<Box<dyn I2CConsumer<Target, StateUpdate>>>>,
    handler_target: RwLock<Option<Target>>,
    target_receiver: Option<Receiver<Target>>,
    state_updater: Option<Sender<StateUpdate>>,
    init_update_processors: Vec<fn(&mut MainThread<Target, StateUpdate>, &StateUpdate) -> ()>,
    main_thread_func: Option<fn(&mut MainThread<Target, StateUpdate>) -> ()>,
    proxy_interceptors_init_hub_0: Option<Vec<Box<dyn SdkPacketHandler<Target, StateUpdate>>>>,
    proxy_interceptors_init_hub_1: Option<Vec<Box<dyn SdkPacketHandler<Target, StateUpdate>>>>,
}
pub static KILL_CHANNEL: OnceLock<(Sender<()>, Receiver<()>)> = OnceLock::new();
pub(crate) static IS_RUNNING: OnceLock<AtomicBool> = OnceLock::new();
impl<Target, StateUpdate> Robot<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static,
                                                           StateUpdate: Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    pub(crate) fn kill_channel() -> &'static (Sender<()>, Receiver<()>) {
        KILL_CHANNEL.get_or_init(|| unbounded())
    }
    pub(crate) fn is_running() -> &'static AtomicBool {
        IS_RUNNING.get_or_init(|| AtomicBool::new(true))
    }
    pub fn new(hub_0: &'static LynxHub, hub_1: Option<&'static LynxHub>,
               gamepad_receiver: &'static Receiver<(Vec<u8>, Vec<u8>)>, telemetry: &'static Telemetry,
               initializer: fn(&mut Robot<Target, StateUpdate>) -> Target) -> Robot<Target, StateUpdate> {
        Self::kill_channel();
        Self::is_running().store(true, Ordering::SeqCst);
        Robot {
            hub_0,
            hub_1,
            hub_0_handlers: vec![],
            hub_1_handlers: vec![],
            gp_handlers: vec![],
            gamepad_receiver,
            telemetry,
            initializer: Some(initializer),
            i2c_devices: vec![],
            handler_target: RwLock::new(None),
            target_receiver: None,
            state_updater: None,
            init_update_processors: vec![],
            main_thread_func: None,
            proxy_interceptors_init_hub_0: Some(vec![]),
            proxy_interceptors_init_hub_1: Some(vec![]),
        }
    }
    pub fn init(mut self) {
        //create the channels that will be used to send data between main thread and other threads
        let (s_tx, s_rx) = unbounded();
        let (t_tx, t_rx) = unbounded();
        self.state_updater = Some(s_tx);
        self.target_receiver = Some(t_rx);

        //grab the initializer function that the user provided. they will give us the handlers when we call this
        let init = self.initializer.expect("no initializer found");
        self.initializer = None;
        log::info!("running init function");
        let default = init(&mut self);
        log::info!("ran init function");
        self.set_target(default.clone());//make sure we have a target set always, so we don't have to use Option<>

        //grab the things the main thread will need later, before we create the Arc which consumes this type
        let processors = self.init_update_processors;
        self.init_update_processors = vec![];
        let func = self.main_thread_func.take();
        let later_telemetry = self.telemetry.clone();

        //grab the interceptors, create the Arc<Robot> so multiple threads can use self, send interceptors to proxy
        let taken_proxy_interceptors_h0 = self.proxy_interceptors_init_hub_0.take().expect("init func could not get proxy_interceptors 0!!!");
        let taken_proxy_interceptors_h1 = self.proxy_interceptors_init_hub_1.take().expect("init func could not get proxy_interceptors 1!!!");
        let arc_self = Arc::new(self);
        let proxies = arc_self.try_get_proxies();
        taken_proxy_interceptors_h0
            .into_iter().map(|x| Box::new(InterceptorData::new(x, arc_self.clone())))
            .for_each(|x| {
                proxies[0].add_interceptor(x);
            });
        taken_proxy_interceptors_h1
            .into_iter().map(|x| Box::new(InterceptorData::new(x, arc_self.clone())))
            .for_each(|x| {
                if proxies.len() > 1 {
                    proxies[1].add_interceptor(x);
                } else {log::info!("do not give us interceptors for hub1 if hub1 does not have a usb connection!")}
            });


        //start the reactor thread. later this may be a threadpool. idk.
        thread::spawn(move || {
            catch(move || {
                let robot = arc_self;
                while Self::is_running().load(Ordering::SeqCst) {
                    robot.run()
                }
                log::info!("main robot thread exited!")
            }, "main robot thread")
        });
        //spawn MainThread. note that it *does not* have Robot access, which is intentional.
        //hardware things should be handled in hardware threads.
        thread::spawn(move || {
            //this thread waits for MainThread to panic or exit
            let thread = thread::spawn(move || {
                MainThread::new(default, t_tx, s_rx, func, later_telemetry, processors)
                    .run();
            });
            match thread.join() {
                Ok(it) => {/*kinda shouldn't happen I think*/}
                Err(it) => {
                    if let Some(v) = it.downcast_ref::<OpModeStop>() {
                        log::info!("MainThread Stopped");
                    } else {
                        //user code panicked.
                        log::error!("User panic: {:?}", it);
                        panic_any(it);//just rethrow it. remember people: don't panic!
                    }
                }
            }
        });
    }
    fn run(&self) {
        let never = never();
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
            recv(self.hub_0.receiver) -> msg => {
                log::trace!("got new packet");
                let data = msg.unwrap();
                self.handle_packet(data);
            }
            recv(self.get_hub_1_receiver(&never)) -> msg => {
                log::trace!("got new packet - hub 2");
                let data = msg.unwrap();
                self.handle_packet(data);
            }
            recv(Self::kill_channel().1) -> _ => {
                log::info!("STOPPING OPMODE!");
                //should stop us from running
                Self::is_running().store(false, Ordering::SeqCst);
                log::info!("Opmode status --- running: {}", Self::is_running().load(Ordering::SeqCst));
                if let DirectProxy(proxy) = &self.hub_0.sdk_proxy {//clean up reactors in proxies
                    proxy.remove_interceptors();
                }
                if let Some(hub) = &self.hub_1 {
                    if let DirectProxy(proxy) = &hub.sdk_proxy {
                        proxy.remove_interceptors();
                    }
                }
            }
        }
    }
    fn get_hub_1_receiver<'a>(&self, never: &'a Receiver<Packet>) -> &'a Receiver<Packet> {
        match self.hub_1 {
            None => never,
            Some(it) => {
                if it.is_over_rs.is_some() { never } else { &it.receiver }
            }
        }
    }
    fn handle_packet(&self, data: Packet) {
        match &data.payload_data {
            //Ack(_) => {/*ignore this. that's literally the _entire_ point of this project*/},
            _ => {
                log::trace!("handling packet 0 rn{}", data.reference_number);
                let mut data = match self.hub_0.should_consume(&data) {
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
                for i2c in &self.i2c_devices {
                    match data {
                        None => {}
                        Some(it) => {
                            let mut device = i2c.lock().unwrap();
                            data = device.maybe_consume_packet(&self, it);
                        }
                    };
                }
                if data.is_none() || self.hub_1.is_none() {
                    return;
                }
                log::trace!("handling packet 1 rn{}", data.as_ref().unwrap().reference_number);
                data = match self.hub_1.as_ref().unwrap().should_consume(data.as_ref().unwrap()) {
                    None => {
                        Some(data.unwrap())
                    }
                    Some(it) => {
                        for x in 0..self.hub_1_handlers.len() {
                            Self::catch_user_function(|| self.hub_1_handlers[x].lock().expect("failed to lock hub1 handler").update(&self, &it),
                                                      || format!("hub 1 handler {}", x), &self.telemetry);
                        }
                        None
                    }
                };
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
    pub fn add_i2c_device<Device: 'static, T: 'static>(&mut self, device: Box<Device>, handlers: Vec<Box<dyn I2CDeviceHandler<Device, T, Target, StateUpdate>>>) where Device: I2CDevice<T> {
        let both = I2CDevicePair { device, handlers };
        self.i2c_devices.push(Mutex::new(Box::new(both)));
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
    pub fn add_proxy_interceptor_hub_0<D>(&mut self, func: D) where D: SdkPacketHandler<Target, StateUpdate> + 'static {
        let list = self.proxy_interceptors_init_hub_0.as_mut().unwrap();
        list.push(Box::new(func));
    }
    pub fn add_proxy_interceptor_hub_1<D>(&mut self, func: D) where D: SdkPacketHandler<Target, StateUpdate> + 'static {
        let list = self.proxy_interceptors_init_hub_1.as_mut().unwrap();
        list.push(Box::new(func));
    }
    pub fn try_get_proxies(&self) -> Vec<&Proxy> {
        if let DirectProxy(p_0) = &self.hub_0.sdk_proxy {
            //make sure we have a real proxy.
            if let Some(it) = self.hub_1 {
                //we have an ex hub
                if let DirectProxy(it) = &it.sdk_proxy {
                    //we have a usb connection
                    vec![p_0, it]
                } else {
                    //we do not have an usb connection
                    vec![p_0]
                }
            } else {
                //we do not have an ex hub
                vec![p_0]
            }
        } else { panic!("could not find any real proxies!") }
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
}
impl<Target, StateUpdate> MainThread<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static,
                                                                StateUpdate: Send + UnwindSafe + Sync + RefUnwindSafe + 'static + PartialEq + Clone + std::fmt::Debug {
    fn new(default_target: Target, sender: Sender<Target>, receiver: Receiver<StateUpdate>, func: Option<fn(&mut Self) -> ()>, telemetry: Telemetry,  processors: Vec<fn(&mut MainThread<Target, StateUpdate>, &StateUpdate) -> ()>) -> MainThread<Target, StateUpdate> {
        MainThread {target: default_target, sender, receiver, state: vec![], function: func, processors: Some(processors), telemetry}
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
        self.maybe_panic();
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
        self.maybe_panic();
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
    fn process(&mut self, data: StateUpdate) -> StateUpdate {
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
    fn block_on_receive(&mut self) -> StateUpdate {
        while self.is_running() {
            match self.receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(it) => {return it}
                Err(_) => {}
            }
        }
        self.maybe_panic();
        panic!("ERROR SOMETHING IS WRONG");
    }
    fn get_statuses_blocking(&mut self) {
        if !self.get_statuses() {//try to get non-blocking. if nothing, block.
            let pack = self.block_on_receive();
            self.process(pack);
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
       Robot::<Target, StateUpdate>::is_running().load(Ordering::SeqCst)
    }
    fn maybe_panic(&self) {
        if !self.is_running() {
            panic_any(OpModeStop::default())
        }
    }
    //TODO turn this partially into a macro
    pub fn get_updated_status(&self, status: &StateUpdate) -> Option<&StateUpdate> {
        self.maybe_panic();
        for i in 0..self.state.len() {
            if discriminant(&self.state[i]) == discriminant(&status){
                return Some(&self.state[i]);
            }
        }
        None
    }
}
pub(crate) trait Interceptor: Send + Sync + RefUnwindSafe {
    fn intercept(&mut self, pack: Packet, send: &Sender<Packet>) -> Option<Packet>;
}
struct InterceptorData<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    func: Box<dyn SdkPacketHandler<Target, StateUpdate>>,
    robot: Arc<Robot<Target, StateUpdate>>
}
impl<Target, StateUpdate> Interceptor for InterceptorData<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    fn intercept(&mut self, pack: Packet, send: &Sender<Packet>) -> Option<Packet> {
        self.func.handle_packet(self.robot.as_ref(), pack, send)
    }
}
impl<'a, Target, StateUpdate> InterceptorData<Target, StateUpdate> where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    fn new(func: Box<dyn SdkPacketHandler<Target, StateUpdate>>, robot: Arc<Robot<Target, StateUpdate>>) -> InterceptorData<Target, StateUpdate> {
        InterceptorData {func, robot}
    }
}

pub trait SdkPacketHandler<Target, StateUpdate>: Send + Sync + UnwindSafe + RefUnwindSafe where Target: Send + UnwindSafe + Sync + RefUnwindSafe + Clone + 'static, StateUpdate:  Send + UnwindSafe + Sync + RefUnwindSafe + PartialEq + 'static + Clone + Debug {
    fn handle_packet(&mut self, robot: &Robot<Target, StateUpdate>, packet: Packet, to_reader: &Sender<Packet>) -> Option<Packet>;
    //msgnum = refnum
    fn try_get_sender<'a>(&self, robot: &'a Robot<Target, StateUpdate>, addr: u8) -> Option<&'a Sender<Packet>> {
        Some(&self.try_get_hub(robot, addr)?.sender)
    }
    fn try_get_hub(&self, robot: &Robot<Target, StateUpdate>, addr: u8) -> Option<&'static LynxHub> {
        if robot.hub_0.module.module_addr == addr {
            Some(&robot.hub_0)
        } else if let Some(hub) = robot.hub_1.as_ref() {
            if hub.module.module_addr == addr {
                Some(robot.hub_1.as_ref()?)
            } else {None}
        } else {None}
    }
}
///This tells us whether a main thread panic was intentional or not.
#[derive(Debug, Copy, Clone, Default)]
pub struct OpModeStop {}