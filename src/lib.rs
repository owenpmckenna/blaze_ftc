pub mod serialization;
pub mod threads;
pub mod sdk_proxy;
pub mod control;
pub mod telemetry;
pub extern crate crossbeam_channel;
pub extern crate jni;
//pub extern crate self as blaze_ftc;

use jni::sys::{jboolean, jbyte, jbyteArray};
use std::thread::sleep;

/*fn unwrap<T>(x: Option<T>) -> T where T: Sized
{
    match x {
        None => {log::error!("ERROR")}
        Some(it) => {it}
    }
}*/
use jni::{JNIEnv, JavaVM};
use jni::objects::{GlobalRef, JClass, JObject, JString};
use jni::sys::jint;

use crate::serialization::packet::Packet;
use crate::threads::{read::generate_read_threads, send::generate_write_threads};
use crossbeam_channel::{unbounded, Receiver, Sender};
//use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use std::backtrace::Backtrace;
use std::io::{Read, Write};
use std::ops::{Add, Div, Mul, Sub};
use std::os::fd::{FromRawFd, RawFd};
use std::panic;
use std::panic::{AssertUnwindSafe, catch_unwind, UnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use crate::telemetry::telemetry::Telemetry;

static RUNNING: AtomicBool = AtomicBool::new(false);

struct Channels {
    send_channel: Mutex<Option<Sender<Packet>>>,
    recv_channel: Mutex<Option<Receiver<Packet>>>,
}
static PORT: Mutex<Option<SerialPort>> = Mutex::new(None);
static NORMAL_CHANNELS_HOLDING: Channels = Channels {
    send_channel: Mutex::new(None),
    recv_channel: Mutex::new(None),
};

static PROXY: OnceLock<Proxy> = OnceLock::new();
fn log_fd_info(fd: &RawFd) -> std::io::Result<Option<String>> {
    // Use nix to get detailed stat info
    use nix::fcntl::fcntl;
    use nix::sys::stat::fstat;
    let stat = unsafe { fstat(*fd) }.expect("Failed to fstat");

    log::info!("File Descriptor: {}", fd);
    log::info!("Device ID: {}", stat.st_dev);
    log::info!("Inode: {}", stat.st_ino);
    log::info!("Mode (permissions + type): {:o}", stat.st_mode);
    log::info!("Number of hard links: {}", stat.st_nlink);
    log::info!("Owner UID: {}", stat.st_uid);
    log::info!("Owner GID: {}", stat.st_gid);
    log::info!("Rdev (for devices): {}", stat.st_rdev);
    log::info!("File size: {}", stat.st_size);
    log::info!("Block size: {}", stat.st_blksize);
    log::info!("Number of blocks: {}", stat.st_blocks);
    // Get file status flags using fcntl
    let flags = unsafe { fcntl(*fd, FcntlArg::F_GETFL) }.expect("Failed to get file flags");
    println!("File status flags (O_RDONLY/O_WRONLY/etc): {:o}", flags);

    // ---- SELinux context ----
    /*unsafe {
        let mut context_ptr: *mut i8 = std::ptr::null_mut();

        let ret = selinux_sys::fgetfilecon(*fd, &mut context_ptr);
        if ret == 0 && !context_ptr.is_null() {
            let cstr = std::ffi::CStr::from_ptr(context_ptr);
            if let Ok(context) = cstr.to_str() {
                log::info!("SELinux context: {}", context);
            }
            selinux_sys::freecon(context_ptr);
        } else {
            log::info!("SELinux context: (none or error)");
        }
    }*/

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

    // ---- Current process SELinux context ----
    /*unsafe {
        let mut context_ptr: *mut i8 = std::ptr::null_mut();
        if selinux_sys::getcon(&mut context_ptr) == 0 && !context_ptr.is_null() {
            let cstr = std::ffi::CStr::from_ptr(context_ptr);
            if let Ok(context) = cstr.to_str() {
                log::info!("Process SELinux context: {}", context);
            }
            selinux_sys::freecon(context_ptr);
        } else {
            log::info!("Process SELinux context: (unknown)");
        }
    }*/

    //let termios = unsafe { termios::tcgetattr(fd).unwrap() };

    Ok(filepath)
}

fn call_close_object(env: &JNIEnv) {
    // Find the class
    let class = env
        .find_class("dev/anygeneric/blazeftc/BlazeFTC")
        .expect("Failed to load the target class");
    let result = env.call_static_method(class, "closeStreams", "()V", &[]);
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
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_openFile(
    env: JNIEnv,
    _class: JClass,
    fd: JObject,
    telemetry: JObject
) {
    catch(|| {
        log::info!("Hello from RUST!");

        let vm = env.get_java_vm().unwrap();
        match JAVA_VM.set(vm) {
            Ok(_) => {log::info!("stored javavm!")}
            Err(it) => {log::error!("could not store javavm!")}
        }

        let fd_int = env
            .call_method(fd, "getInt$", "()I", &[])
            .unwrap()
            .i()
            .unwrap();
        let read_fd = fd_int as RawFd;

        let path = log_fd_info(&read_fd).unwrap().unwrap();
        // Now wrap it in Rust I/O
        //let file = unsafe { std::fs::File::from_raw_fd(raw_fd) };

        //let path: String = env.get_string(name).expect("Invalid string").into();

        //let file = File::open(path).expect("Failed to open file");
        call_close_object(&env);
        log::info!("acquiring port...");
        //let write_fd: RawFd = unsafe { libc::dup(read_fd) };
        //let read_fd: RawFd = unsafe { libc::dup(write_fd) };
        //let write_fd = read_fd;
        //let mut port_read = unsafe { SerialStream::from_raw_fd(read_fd) };
        //let mut port_write = unsafe { SerialStream::from_raw_fd(write_fd) };
        let mut port = SerialPort::open(path, |mut settings: Settings| {
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
        log::info!("acquired port... not destructing old one");

        //TODO: try this later
        //unsafe {
        //    let x = SerialStream::from_raw_fd(read_fd);
        //    drop(x)
        //}
        log::info!("didn't destroy old fd");

        RUNNING.store(true, Ordering::SeqCst);

        let read_rx = generate_read_threads(port.try_clone().unwrap(), &RUNNING);
        //reg_read_rx will not be used for this level of testing.

        let write_tx = generate_write_threads(port.try_clone().unwrap(), &RUNNING);
        //reg_write_tx not used here

        let (write_tx, read_rx, proxy) = Proxy::new(write_tx, read_rx, &RUNNING);
        PROXY.get_or_init(|| proxy);
        GAMEPAD_CHANNELS.get_or_init(|| unbounded());

        let lock_port = PORT.lock();
        lock_port.unwrap().get_or_insert(port);
        let lock_tx_2 = NORMAL_CHANNELS_HOLDING.send_channel.lock();
        lock_tx_2.unwrap().get_or_insert(write_tx);
        let lock_rx_2 = NORMAL_CHANNELS_HOLDING.recv_channel.lock();
        lock_rx_2.unwrap().get_or_insert(read_rx);

        BLAZEFTC_CLASS.get_or_init(|| env.new_global_ref(telemetry).unwrap());
    }, "open file");
}
pub static BLAZEFTC_CLASS: OnceLock<GlobalRef> = OnceLock::new();

type GamepadNotification = (Vec<u8>, Vec<u8>);
static GAMEPAD_CHANNELS: OnceLock<(Sender<GamepadNotification>, Receiver<GamepadNotification>)> = OnceLock::new();

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_run(
    env: JNIEnv,
    _class: JClass,
    to_run: jint
) {
    catch(|| {
        let modules_lock = MODULES.lock().unwrap();
        if modules_lock.len() == 0 {
            panic!("no modules known! maybe we received a bad packet?");
        }
        let receiver = NORMAL_CHANNELS_HOLDING.recv_channel.lock().unwrap().clone().expect("no packet receiver");
        let sender = NORMAL_CHANNELS_HOLDING.send_channel.lock().unwrap().clone().expect("no packet sender");
        let gp_channels = GAMEPAD_CHANNELS.get().expect("failed to unwrap gamepad channels");
        let gp_channel = gp_channels.1.clone();
        let telemetry = Telemetry::new(&RUNNING);
        let modules: &Vec<Module> = modules_lock.as_ref();
        //let robot = Robot::new(modules, receiver, sender, gp_channel, telemetry, robot_init, &RUNNING);
        //robot.init();
        INITFUNC.get().unwrap()(modules, receiver, sender, gp_channel, telemetry, &RUNNING, to_run);
    }, "setup robot");
}
type InitFunction = fn(&Vec<Module>, Receiver<Packet>, Sender<Packet>,
                       Receiver<(Vec<u8>, Vec<u8>)>, Telemetry, &'static AtomicBool, i32) -> ();
static INITFUNC: OnceLock<InitFunction> = OnceLock::new();
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_informOfModule(
    env: JNIEnv,
    _class: JClass,
    module: jint,
    parent: jboolean,
) {
    catch(|| {
        log::info!(
            "informed of module {}! (parent:{})",
            module as u8,
            parent != 0
        );
        {
            let send_lock = NORMAL_CHANNELS_HOLDING.send_channel.lock().unwrap();
            send_lock
                .as_ref()
                .unwrap()
                .send(Packet::new(
                    Command::QueryInterface(QueryInterfaceData::new_deka()),
                    module as u8,
                    0,
                ))
                .unwrap();
        }
        let recv_lock = NORMAL_CHANNELS_HOLDING.recv_channel.lock().unwrap();
        let data = recv_lock.as_ref().unwrap().recv().unwrap();
        match data.payload_data {
            Command::QueryInterfaceResponse(it) => {
                log::info!("got query interface response data! [{}]", it);
                let mut mod_lock = MODULES.lock().unwrap();
                /*if mod_lock.len() == 0 {
                    mod_lock.push(Module::null());
                }*/
                mod_lock.push(Module::from_deka_discovery(module as u8, &it, parent != 0));
                log::info!("added module. number of modules: {}", mod_lock.len())
            }
            it => {
                log::info!("Wrong command received: {}", it);
            }
        }
    }, "inform of module");
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
    env: JNIEnv,
    _class: JClass,
    buffer: JObject,
) {
    catch(|| {
        log::trace!("writing...");
        let jba: jbyteArray = buffer.into_inner();
        let bytes = env.convert_byte_array(jba).unwrap();
        log::trace!("Received data from java! (len={})", bytes.len());
        log::trace!("-EVADEBUG- abt to write bytes from java: {:?} and get lock", bytes);

        PROXY.get().unwrap().write(bytes);
        log::trace!("sent bytes to proxy!");
    }, "ftc write");
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_gamepad(
    env: JNIEnv,
    _class: JClass,
    gp1: JObject,
    gp2: JObject,
) {
    catch(|| {
        let jba0: jbyteArray = gp1.into_inner();
        let bytes0 = env.convert_byte_array(jba0).unwrap();
        let jba1: jbyteArray = gp2.into_inner();//thanks @AS1624
        let bytes1 = env.convert_byte_array(jba1).unwrap();
        let channel = GAMEPAD_CHANNELS.get().unwrap();
        channel.0.send((bytes0, bytes1)).unwrap();
    }, "ftc gamepad");
}


#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_available(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    //TODO implement this haha
    0
}


#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_read(
    env: JNIEnv,
    _class: JClass,
    buffer: JObject,
    off: jint,
    len: jint,
) -> jint {
    catch(|| {
        log::trace!("ftc: waiting for data...");
        let proxy = PROXY.get().unwrap();

        let temp: Vec<u8> = proxy.read(len as usize);
        let tmp = temp.len();
        log::trace!("Hello from rust! giving ftc {} bytes", tmp);

        if tmp > 0 {
            let data = &temp[..tmp];
            log::trace!("-EVADEBUG- read bytes to java: {:?}", data);
            env.set_byte_array_region(
                buffer.into_inner(),
                off,
                &data.into_iter().map(|x| *x as i8).collect::<Vec<jbyte>>(),
            )
            .unwrap();
        }

        tmp as jint
    }, "ftc read")
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_anygeneric_blazeftc_BlazeFTC_close(_env: JNIEnv, _class: JClass) {
    RUNNING.store(false, Ordering::SeqCst);

    match PORT.lock().unwrap().take() {
        None => {
            log::info!("close didn't have anything to close")
        }
        //TODO: hey fyi this is not going to work there are other references to this thing
        //we need to interrupt the other threads, probably. or handle errors idk
        Some(it) => {
            log::info!("closing port...");
            drop(it)
        }
    }
    sleep(Duration::from_secs(2));
    panic!("shutdown")
}

use crate::serialization::command::Command;
use crate::serialization::command_utils::Module;
use crate::serialization::commands::QueryInterfaceData;
use crate::serialization::lynx_commands::base_lynx_command::MODULES;
use android_logger::Config;
use log::{LevelFilter, log};
use nix::fcntl::FcntlArg;
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use lazy_static::lazy_static;
use crate::control::gamepad::Gamepad;
use crate::sdk_proxy::proxy::Proxy;

pub fn JNI_OnLoad_handler(initfunc: InitFunction) -> jint {
    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Info)
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