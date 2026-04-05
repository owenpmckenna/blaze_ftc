pub mod serialization;
pub mod threads;
pub mod sdk_proxy;
pub mod control;
pub mod telemetry;
pub extern crate crossbeam_channel;
pub extern crate jni;
//pub extern crate self as blaze_ftc;

use jni::sys::{jboolean, jbyte, jbyteArray, jdouble, jstring};
use std::thread::sleep;

/*fn unwrap<T>(x: Option<T>) -> T where T: Sized
{
    match x {
        None => {log::error!("ERROR")}
        Some(it) => {it}
    }
}*/
use jni::{jni_sig, jni_str, Env, EnvUnowned, JNIEnv, JavaVM};
use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString};
use jni::sys::jint;

use crate::serialization::packet::Packet;
use crate::threads::{read::generate_read_threads, send::generate_write_threads};
use crossbeam_channel::{unbounded, Receiver, Sender};
//use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use std::backtrace::Backtrace;
use std::io::{Read, Write};
use std::ops::{Add, Div, Mul, Sub};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::panic;
use std::panic::{AssertUnwindSafe, catch_unwind, UnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use crate::telemetry::telemetry::{set_allowed_to_send_dangerous_packets, Telemetry};

static RUNNING: AtomicBool = AtomicBool::new(false);

static PROXY: OnceLock<Proxy> = OnceLock::new();
static HUB_0: OnceLock<LynxHub> = OnceLock::new();
static HUB_1: OnceLock<LynxHub> = OnceLock::new();
fn log_fd_info(fd: &RawFd) -> std::io::Result<Option<String>> {
    // ---- Origin / source file ----
    // /proc/self/fd/<fd> symlink points to the actual file/device
    let fd_path = format!("/proc/self/fd/{}", fd);
    let mut filepath = None;
    if let Ok(path) = std::fs::read_link(&fd_path) {
        log::info!("FD points to: {}", path.display());
        filepath = Some(format!("{}", path.display()))
    } else {
        log::info!("FD origin: unknown");
    }

    Ok(filepath)
}

fn call_close_object(env: &mut Env) {
    // Find the class
    let class = env
        .find_class(jni_str!("dev/anygeneric/blazeftc/BlazeFTC"))
        .expect("Failed to load the target class");
    let result = env.call_static_method(class, jni_str!("closeStreams"), jni_sig!("()V"), &[]);
    match result {
        Ok(it) => {
            println!("called callback")
        }
        Err(it) => {
            println!("failed to call callback: {}", it)
        }
    }
}

//dev.anygeneric.blazeftc
pub static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();
fn setup_port(port: impl AsRef<Path>) -> (Sender<Packet>, Receiver<Packet>, Proxy) {
    let mut port = SerialPort::open(port, |mut settings: Settings| {
        settings.set_raw();
        settings.set_stop_bits(StopBits::One);
        settings.set_char_size(CharSize::Bits8);
        settings.set_baud_rate(460800).expect("Bad baud rate");
        settings.set_parity(Parity::None);
        settings.set_flow_control(FlowControl::None);
        Ok(settings)
    })
        .unwrap();
    port.set_read_timeout(Duration::from_hours(24)).unwrap(); //may not always have data.
    attempt_speedup_fd(port.as_raw_fd());
    log::info!("acquired port...");

    RUNNING.store(true, Ordering::SeqCst);

    let read_rx = generate_read_threads(port.try_clone().unwrap(), &RUNNING);
    //reg_read_rx will not be used for this level of testing.

    let write_tx = generate_write_threads(port.try_clone().unwrap(), &RUNNING);
    //reg_write_tx not used here

    Proxy::new(write_tx, read_rx, &RUNNING)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_initialize(
    mut env: EnvUnowned,
    _class: JClass,
    telemetry: JObject
) {
    log::info!("Hello from RUST!");
    env.with_env(|env| -> Result<_, jni::errors::Error> {
        match JAVA_VM.get() {
            None => { log::info!("uninitialized! grabbing hardware..."); }
            Some(_) => {
                log::info!("Already initialized! --- skipping...");
                return Ok(());
            }
        }

        let vm = env.get_java_vm().unwrap();
        match JAVA_VM.set(vm) {
            Ok(_) => { log::info!("stored javavm!") }
            Err(it) => { log::error!("could not store javavm!") }
        }
        // Now wrap it in Rust I/O
        //let file = unsafe { std::fs::File::from_raw_fd(raw_fd) };

        //let path: String = env.get_string(name).expect("Invalid string").into();

        //let file = File::open(path).expect("Failed to open file");
        call_close_object(env);
        log::info!("acquiring port...");
        let ctrl_hub_init = CTRL_HUB_MODULE_INIT_DATA.get().expect("could not get main module data");
        //let write_fd: RawFd = unsafe { libc::dup(read_fd) };
        //let read_fd: RawFd = unsafe { libc::dup(write_fd) };
        //let write_fd = read_fd;
        //let mut port_read = unsafe { SerialStream::from_raw_fd(read_fd) };
        //let mut port_write = unsafe { SerialStream::from_raw_fd(write_fd) };
        //PROXY.get_or_init(|| proxy);
        //here
        let (write_tx, read_rx, proxy) = setup_port(ctrl_hub_init.0.clone());
        GAMEPAD_CHANNELS.get_or_init(|| unbounded());

        let ctrl_hub_module = Module::generate_module(ctrl_hub_init.1 as u8, true, &write_tx, &read_rx);
        let ctrl_hub = LynxHub::new(ctrl_hub_module, write_tx.clone(), UnderlyingHw::DirectProxy(proxy), read_rx.clone(), false);
        HUB_0.set(ctrl_hub).expect("couldn't set ctrl_hub");

        if let Some(it) = EX_HUB_MODULE_INIT_DATA.get() {
            log::info!("Found expansion hub.");
            if it.0.eq_ignore_ascii_case(&ctrl_hub_init.0) { //there is no /dev/ttys1 vs ttyS1 we're fine and Java has given me a fundamental distrust of string comparisons
                let ctrl_hub = HUB_0.get().unwrap();
                let ex_hub_module = Module::generate_module(it.1 as u8, false, &ctrl_hub.sender, &ctrl_hub.receiver);
                let ex_hub = LynxHub::new(ex_hub_module, write_tx, UnderlyingHw::OtherHub(ctrl_hub), read_rx, true);
                HUB_1.set(ex_hub).expect("could not set ex hub!");
            } else {
                let (write_tx, read_rx, proxy) = setup_port(it.0.clone());
                let ex_hub_module = Module::generate_module(it.1 as u8, false, &write_tx, &read_rx);
                let ex_hub = LynxHub::new(ex_hub_module, write_tx, UnderlyingHw::DirectProxy(proxy), read_rx, false);
                HUB_1.set(ex_hub).expect("could not set ex hub!");
            }
        } else { log::info!("No expansion hub."); }
        BLAZEFTC_CLASS.get_or_init(|| env.new_global_ref(telemetry).unwrap());
        Ok(())
    }).resolve::<ThrowRuntimeExAndDefault>();
}

fn attempt_speedup_fd(p0: RawFd) {
    let mut term = Termios::from_fd(p0).expect("could not get termios");
    cfmakeraw(&mut term);
    term.c_iflag &= !(IXON | IXOFF | IXANY);
    term.c_oflag &= !OPOST;
    term.c_cc[VMIN]  = 1;   // return as soon as 1 byte arrives
    term.c_cc[VTIME] = 0;   // no timeout

    tcsetattr(p0, TCSANOW, &term).expect("could not set the terminal values!");
}


pub static BLAZEFTC_CLASS: OnceLock<Global<JObject>> = OnceLock::new();

type GamepadNotification = (Vec<u8>, Vec<u8>);
static GAMEPAD_CHANNELS: OnceLock<(Sender<GamepadNotification>, Receiver<GamepadNotification>)> = OnceLock::new();
static TELEMETRY: OnceLock<Telemetry> = OnceLock::new();

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_run(
    env: EnvUnowned,
    _class: JClass,
    to_run: jint
) {
    catch(|| {
        set_allowed_to_send_dangerous_packets(true);
        let hub_0 = HUB_0.get().expect("no packet receiver");
        let hub_1 = HUB_1.get();//may not exist, don't expect it
        let gp_channels = GAMEPAD_CHANNELS.get().expect("failed to unwrap gamepad channels");
        let telemetry = TELEMETRY.get_or_init(|| Telemetry::new(IS_RUNNING.get_or_init(|| AtomicBool::new(true))));
        let function = INITFUNC.get().expect("No initfunc!");
        log::info!("handing over control to user code!!!");
        log::info!("hub_0 id: {:?}", hub_0.module);
        if let Some(h1) = &hub_1 {
            log::info!("hub_1 id: {:?}", h1.module);
        }
        function(hub_0, hub_1, &gp_channels.1, telemetry, to_run);
    }, "setup robot");
}
type InitFunction = fn(&'static LynxHub, Option<&'static LynxHub>,
                       &'static Receiver<(Vec<u8>, Vec<u8>)>, &'static Telemetry, i32) -> ();
static INITFUNC: OnceLock<InitFunction> = OnceLock::new();
static CTRL_HUB_MODULE_INIT_DATA: OnceLock<(String, i32)> = OnceLock::new();
static EX_HUB_MODULE_INIT_DATA: OnceLock<(String, i32)> = OnceLock::new();
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_informOfModule(
    mut env: EnvUnowned,
    _class: JClass,
    module: jint,
    parent: jboolean,
    fd: JObject,
) {
    env.with_env(|env| -> Result<_, jni::errors::Error> {
        log::info!(
            "informed of module {}! (parent:{})",
            module as u8,
            parent
        );

        let fd_int = env
            .call_method(fd, jni_str!("getInt$"), jni_sig!("()I"), &[])
            .expect("could not call getInt")
            .i()
            .expect("could not convert to jint!");
        let read_fd = fd_int as RawFd;

        let path = log_fd_info(&read_fd).expect("could not get fd info!").expect("could not get inner fd info!");

        println!("module path: {}", path);

        if CTRL_HUB_MODULE_INIT_DATA.get().is_none() {
            log::info!("putting data in ctrl hub init");
            CTRL_HUB_MODULE_INIT_DATA.set((path, module as i32))
                .expect("could not set ctrl hub init data");
        } else if EX_HUB_MODULE_INIT_DATA.get().is_none() {
            log::info!("putting data in ex hub init");
            EX_HUB_MODULE_INIT_DATA.set((path, module as i32))
                .expect("could not set expansion hub init data");
        } else {
            log::info!("we have already been informed of module!")
        }
        Ok(())
    }).resolve::<ThrowRuntimeExAndDefault>();
}
pub(crate) fn catch<F, R>(func: F, name: &str) -> R where F: FnOnce() -> R + UnwindSafe {
    let result = catch_unwind(func);
    match result {
        Ok(it) => {it}
        Err(it) => {
            log::error!("error while {}!", name);
            log::info!("error while {}!", name);
            if let Some(s) = it.downcast_ref::<&str>() {
                log::error!("Caught panic: {}", s);
                log::info!("Caught panic: {}", s)
            } else if let Some(s) = it.downcast_ref::<String>() {
                log::error!("Caught panic: {}", s);
                log::info!("Caught panic: {}", s);
            } else {
                log::error!("Caught unknown panic type");
                log::info!("Caught unknown panic type");
            }

            // Capture a backtrace
            let bt = Backtrace::capture();
            log::error!("Backtrace:\n{:?}", bt);
            log::info!("Backtrace:\n{:?}", bt);

            RUNNING.store(false, Ordering::SeqCst);//uhh just kill it idk

            sleep(Duration::from_secs(2));

            panic!("Failure while: {}", name);
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_write(
    mut env: EnvUnowned,
    _class: JClass,
    buffer: JByteArray,
    connectionNumber: jint//we *should* be able to just use the packet's id, but I'm keeping this just in case
) {
    env.with_env(|env| -> Result<_, jni::errors::Error> {
        catch(|| {
            log::trace!("writing...");
            let bytes: Vec<u8> = env.convert_byte_array(buffer).unwrap();
            log::trace!("Received data from java! (len={})", bytes.len());
            log::trace!("-EVADEBUG- abt to write bytes from java: {:?} and get lock", bytes);
            if let Some(it) = Packet::from_data(bytes.as_slice()) {
                log::trace!("fw packet from java! {:?}", it);
                let hub_0 = HUB_0.get().expect("HUB 0 unset!");
                if let Some(hub) = LynxHub::get_for_id_careful(it.dest_module_addr) {
                    //hub.notify_send_packet(&it);//TODO here
                }
                if it.dest_module_addr == hub_0.module.module_addr {
                    hub_0.send_from_sdk(it, None);
                    log::trace!("sent bytes to proxy!");
                    return Ok(());
                }
                if let Some(hub_1) = HUB_1.get() {
                    if hub_1.module.module_addr == it.dest_module_addr {
                        hub_1.send_from_sdk(it, None);
                        log::trace!("sent bytes to proxy!");
                        return Ok(());
                    }
                }
                log::info!("got packet {:?} for unknown destination! h0:{:?}, h1:{:?}", it, hub_0, HUB_1.get());
            } else {log::info!("could not deserialize packet from java!");}
            Ok(())
        }, "write function")
    }).resolve::<ThrowRuntimeExAndDefault>();
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_gamepad(
    mut env: EnvUnowned,
    _class: JClass,
    gp1: JByteArray,
    gp2: JByteArray,
) {
    env.with_env(|env| -> Result<_, jni::errors::Error> {
        let bytes0 = env.convert_byte_array(gp1).unwrap();
        let bytes1 = env.convert_byte_array(gp2).unwrap();//thanks @AS1624
        let channel = GAMEPAD_CHANNELS.get().unwrap();
        channel.0.send((bytes0, bytes1)).unwrap();
        Ok(())
    }).resolve::<ThrowRuntimeExAndDefault>();
}


#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_available(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    //TODO implement this haha
    0
}

static SEND_PROPERTY_CHANNELS: OnceLock<(Sender<(String, String)>, Receiver<(String, String)>)> = OnceLock::new();
pub(crate) fn get_property_channels() -> &'static (Sender<(String, String)>, Receiver<(String, String)>) {
    SEND_PROPERTY_CHANNELS.get_or_init(|| unbounded())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_send_property(
    mut env: EnvUnowned,
    _class: JClass,
    key: JString,
    value: JString
) {
    env.with_env(|env| -> Result<_, jni::errors::Error> {
        let key = key.try_to_string(&env)?;
        let value = value.try_to_string(&env)?;
        get_property_channels().0.send((key, value)).expect("could not send property");
        Ok(())
    }).resolve::<ThrowRuntimeExAndDefault>();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_setMotorPower(
    env: JNIEnv, _class: JClass, module: jint, port: jint, power: jdouble
) {
    catch(|| {
        [&HUB_0, &HUB_1].into_iter().for_each(move |it| {
            if let Some(it) = it.get() {
                if it.module.module_addr == module as u8 {
                    it.send_motor_command(port as u8, power as f32);
                }
            }
        });
        //log::info!("No module found! m{} p{} pow{}", module, port, power)
    }, "set motor power from java");
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_read(
    mut env: EnvUnowned,
    _class: JClass,
    buffer: JByteArray,
    off: jint,
    len: jint,
    connectionNumber: jint
) -> jint {
    env.with_env(|env| -> Result<jint, jni::errors::Error> {
        log::trace!("ftc: waiting for data...");
        let hub = match HUB_1.get() {
            None => {HUB_0.get().unwrap()}
            Some(it) => {
                if it.module.module_addr == connectionNumber as u8 {//will "as u8" work? stay tuned for more stupid java nonsense!
                    it
                } else { HUB_0.get().unwrap() }
            }
        };
        let temp: Vec<u8> = hub.read_for_sdk(len);
        let tmp = temp.len();
        log::trace!("Hello from rust! giving ftc {} bytes", tmp);

        if tmp > 0 {
            let data = &temp[..tmp];
            log::trace!("-EVADEBUG- read bytes to java: {:?}", data);
            buffer.set_region(env, 0, &data.into_iter().map(|x| *x as i8).collect::<Vec<jbyte>>())?;
        }

        Ok(tmp as jint)
    }).resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_close(_env: JNIEnv, _class: JClass) {
    log::info!("trying to stop...");
    set_allowed_to_send_dangerous_packets(false);
    KILL_CHANNEL.get().expect("could not access kill channel???")
        .0.send(()).expect("could not send kill message???");
    log::info!("sent stop command");
}

use crate::serialization::command_utils::Module;
use android_logger::Config;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::refs::Global;
use jni::strings::JNIString;
use log::{LevelFilter, log};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use crate::sdk_proxy::proxy::Proxy;

pub fn JNI_OnLoad_handler(initfunc: InitFunction) -> jint {
    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Debug)
            .with_tag("MyRust"),
    );

    log::info!("Rust logger initialized");
    INITFUNC.set(initfunc).expect("could not set initfunc");
    jni::sys::JNI_VERSION_1_6
}

pub struct ElapsedTimer {
    last_time: Instant
}
impl ElapsedTimer {
    pub fn new() -> ElapsedTimer {
        ElapsedTimer {last_time: Instant::now()}
    }
    pub fn duration(&self) -> Duration {
        self.last_time.elapsed()
    }
    pub fn duration_reset(&mut self) -> Duration {
        let dur = self.last_time.elapsed();
        self.last_time = Instant::now();
        dur
    }
}

use num_traits::{Num, FromPrimitive};
use termios::{cfmakeraw, tcgetattr, tcsetattr, Termios, IXANY, IXOFF, IXON, OPOST, TCSANOW, VMIN, VTIME};
use crate::control::hardware::{LynxHub, UnderlyingHw};
use crate::control::robot::{Robot, IS_RUNNING, KILL_CHANNEL};

pub struct MovingAverage<T> where T: Num + FromPrimitive + Copy {
    data: Vec<T>,
    total: T,
    index: usize
}
impl<T> MovingAverage<T> where T: Num + FromPrimitive + Copy {
    pub fn new(size: usize) -> MovingAverage<T> {
        if size == 0 {
            panic!("moving average size cannot be zero!");
        }
        MovingAverage {data: vec![T::zero(); size], total: T::zero(), index: 0}
    }
    pub fn avg(&self) -> T {
        self.total / T::from_usize(self.data.len()).expect("cannot convert number from usize")
    }
    pub fn update(&mut self, new: T) {
        self.total = self.total - self.data[self.index];
        self.total = self.total + new;
        self.data[self.index] = new;

        self.index = (self.index + 1) % self.data.len()
    }
}