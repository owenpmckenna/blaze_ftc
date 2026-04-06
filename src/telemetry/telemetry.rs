use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use crossbeam_channel::{unbounded, Receiver, Sender};
use jni::{jni_sig, jni_str, Env};
use jni::errors::{Error};
use jni::objects::{JClass, JClassLoader, JObject, JString, JValue};
use jni::refs::Global;
use jni::strings::JNIStr;
use crate::{catch, ElapsedTimer, MovingAverage, BLAZEFTC_CLASS, JAVA_VM};

//note: "timer" is mostly pointless debug code sorry
#[derive(Clone)]
pub struct Telemetry {
    send: Sender<TelemetryData>,
}
static ALLOWED_TO_SEND_DANGEROUS_COMMANDS: AtomicBool = AtomicBool::new(false);
///starts as true because we are only called to action by an opmode starting
///we're using the telemetry thread for this because it's always running, is not performance sensitive,
///and it's already attached to the jvm which makes this easier
pub fn get_allowed_to_send_dangerous_packets() -> bool {
    ALLOWED_TO_SEND_DANGEROUS_COMMANDS.load(Ordering::SeqCst)
}
///only our crate can trigger this!
pub(crate) fn set_allowed_to_send_dangerous_packets(allowed: bool) -> bool {
    ALLOWED_TO_SEND_DANGEROUS_COMMANDS.swap(allowed, Ordering::SeqCst)
}

impl Telemetry {
    pub fn new(running: &'static AtomicBool) -> Telemetry {
        let (tx, rx) = unbounded();
        thread::spawn(move || {
            let vm = JAVA_VM.get().unwrap();
            catch(move || {
                vm.attach_current_thread(move |env: &mut Env| -> Result<(), Error> {
                    log::info!("started telemetry!");
                    //let class = env.find_class("dev/anygeneric/blazeftc/BlazeFTC")?;
                    let object: &JObject = BLAZEFTC_CLASS.get().unwrap().as_obj();
                    let mut data = vec![TelemetryData::new_f64("".to_string(), 0f64); 0];
                    let mut timer = TelemetryTimers::new(&mut data);

                    let class_loader = get_class_loader(env, object);
                    let opmode_manager_class = load_class(env, &class_loader, "com.qualcomm.robotcore.eventloop.opmode.OpModeManagerImpl");
                    log::info!("telemetry got manager class");
                    loop {
                        if running.load(Ordering::SeqCst) && get_allowed_to_send_dangerous_packets() {
                            Telemetry::execute(&rx, &mut data, &mut timer, env, &object, true);
                        } else {
                            sleep(Duration::from_millis(100));
                            data.clear();
                            timer.reset_last_sent();
                            while rx.try_recv().is_ok() {
                                //this will clear out all the like, write saturation messages or whatever ends up in here
                            }
                        }
                        check_dangerous_commands(env, &opmode_manager_class);
                    }
                }).expect("telemetry thread exited badly!");
                log::info!("telemetry thread exited!")
            }, "telemetry thread");
        });
        Telemetry {send: tx}
    }
    fn send(&self, td: TelemetryData) {
        self.send.send(td).expect("could not get telemetry channel!")
    }
    pub fn add_string(&self, name: &str, data: &str) {
        self.send(
            TelemetryData::new_string(name.to_string(), data.to_string())
        )
    }
    pub fn add_String(&self, name: &str, data: String) {
        self.send(
            TelemetryData::new_string(name.to_string(), data)
        )
    }
    pub fn add_bool(&self, name: &str, data: bool) {
        self.send(
            TelemetryData::new_string(name.to_string(), if data {"true"} else {"false"}.to_string())
        )
    }
    pub fn add_f64(&self, name: &str, data: f64) {
        self.send(
            TelemetryData::new_f64(name.to_string(), data)
        )
    }
    pub fn add_f32(&self, name: &str, data: f32) {
        self.send(
            TelemetryData::new_f64(name.to_string(), data as f64)
        )
    }
    pub fn add_i64(&self, name: &str, data: i64) {
        self.send(
            TelemetryData::new_i64(name.to_string(), data)
        )
    }
    fn execute(rx: &Receiver<TelemetryData>, cache: &mut Vec<TelemetryData>, timer: &mut TelemetryTimers, env: &mut Env, obj: &JObject, running: bool) {
        timer.start();
        let mut data = match rx.recv_timeout(Duration::from_millis(62)) { //16 hz. ds processes at 4 hz so this means you do get updated data
            Ok(it) => {Some(it)}
            Err(_) => {None}
        };
        if rx.len() > 0 {
            //log::info!("telemetry rx size: {}", rx.len());
        }
        timer.waited();
        if timer.need_to_send() && data.is_some() {
            let it = data.take().unwrap();
            Self::process_data(cache, it);
            timer.new_data();
        }
        match data {
            None => {
                timer.new_data();
                timer.push(cache);
                timer.reset_last_sent();
                if running {
                    Self::send_all_telemetry(cache, env, obj).unwrap();
                } else {
                    sleep(Duration::from_millis(75));
                    cache.clear();//do not keep old data, obv
                }
                timer.publish();
            }
            Some(it) => {
                timer.publish();
                Self::process_data(cache, it);
                timer.new_data();
            }
        }
    }
    fn process_data(cache: &mut Vec<TelemetryData>, it: TelemetryData) {
        //ok so this is kind of... wierd. idk if this is how you're supposed to do it but it seems to work idk
        //maybe i should update telemetry in java when i get a piece of data twice. idk.
        let mut option = Some(it);
        let mut x = 0;
        while x < cache.len() && option.is_some() {
            option = cache[x].combine(option.unwrap());
            x += 1;
        }
        if option.is_some() {
            cache.push(option.unwrap())
        }
    }
    fn send_all_telemetry(cache: &mut Vec<TelemetryData>, env: &mut Env, obj: &JObject) -> Result<(), Box<dyn std::error::Error>> {
        /*send all telemetry*/
        for i in cache {
            log::trace!("logging data: {}", i.name);
            let j_name: JValue = if let Some(it) = &i.string_jni_loc {
                it.as_obj().into()
            } else {
                let name = env.new_string(i.name.clone())?;
                let data = Some(Arc::new(env.new_global_ref(name)?));
                i.string_jni_loc = data;
                i.string_jni_loc.as_ref().unwrap().as_obj().into()
            };
            match &i.data {
                TelemetryType::String(it) => {
                    let sig = jni_sig!("(Ljava/lang/String;Ljava/lang/Object;)V");
                    let j_data = env.new_string(it.clone())?;
                    let lst = [j_name, JValue::Object(&j_data)];
                    env.call_method(obj, jni_str!("addData"), sig, &lst)?;
                    env.delete_local_ref(j_data);
                }
                TelemetryType::F64(it) => {
                    let sig = jni_sig!("(Ljava/lang/String;D)V");
                    env.call_method(obj, jni_str!("addData"), sig, &[j_name, (*it).into()])?;
                }
                TelemetryType::I64(it) => {
                    let sig = jni_sig!("(Ljava/lang/String;J)V");
                    env.call_method(obj, jni_str!("addData"), sig, &[j_name.into(), (*it).into()])?;
                }
            }
        }
        env.call_method(obj, jni_str!("update"), jni_sig!("()V"), &[])?;
        Ok(())
    }
}
fn get_class_loader<'a>(env: &mut Env<'a>, obj: &JObject<'a>) -> JClassLoader<'a> {
    let class_obj: JObject = env.call_method(obj, jni_str!("getClass"), jni_sig!("()Ljava/lang/Class;"), &[])
        .expect("cmuc to get class fail")
        .l().expect("could not get obj from class get call");
    let class_loader: JObject = env.call_method(class_obj, jni_str!("getClassLoader"), jni_sig!("()Ljava/lang/ClassLoader;"), &[])
        .expect("cmuc to get cl fail")
        .l().expect("could not get obj from cl get call");
    JClassLoader::cast_local(env, class_loader).expect("could not cast classloader")
    //class_loader
}
fn load_class<'a>(env: &mut Env<'a>, class_loader: &JClassLoader<'a>, class_name: &str) -> JClass<'a> {
    let class_name = env.new_string(class_name).expect("could not get jstring");
    class_loader.load_class(env, class_name).expect("could not load class")
}
fn get_static_bool_field<'a>(env: &mut Env<'a>, cls: &JClass<'a>, field_name: &JNIStr) -> bool {
    env.get_static_field(cls, field_name, jni_sig!("Z")).unwrap().z().unwrap()
}
fn check_dangerous_commands<'a>(env: &mut Env<'a>, class: &JClass<'a>) {
    //let classLoader = get_class_loader(env, obj);
    //let class = load_class(env, classLoader, "com.qualcomm.robotcore.eventloop.opmode.OpModeManagerImpl");
    let prevent = get_static_bool_field(env, class, jni_str!("preventDangerousHardwareAccess"));
    let allowed = !prevent;
    if set_allowed_to_send_dangerous_packets(allowed) != allowed {
        log::info!("telemetry thread caught new hardware access rule: allowed={}", allowed)
    }
}
#[derive(Clone)]
struct TelemetryData {
    name: String,
    string_jni_loc: Option<Arc<Global<JString<'static>>>>,
    data: TelemetryType
}
impl TelemetryData {
    fn new_string(name: String, data: String) -> TelemetryData {
        TelemetryData { name, string_jni_loc: None, data: TelemetryType::String(data) }
    }
    fn new_f64(name: String, data: f64) -> TelemetryData {
        TelemetryData { name, string_jni_loc: None, data: TelemetryType::F64(data) }
    }
    fn new_i64(name: String, data: i64) -> TelemetryData {
        TelemetryData { name, string_jni_loc: None, data: TelemetryType::I64(data) }
    }
    fn combine(&mut self, new: TelemetryData) -> Option<TelemetryData> {
        if self.name == new.name {
            self.data = new.data;
            None
        } else { Some(new) }
    }
}
#[derive(Clone, Debug)]
enum TelemetryType {
    String(String),
    F64(f64),
    I64(i64)
}
//this timer is very dumb. i put it in for testing, and kind of didn't remove it after i added last_submitted because that's actually important
//ignore this except for last_submitted
pub struct TelemetryTimers {
    wait_time_avg: MovingAverage<f32>,
    new_data_time_avg: MovingAverage<f32>,
    publish_time_avg: MovingAverage<f32>,
    current: ElapsedTimer,
    last_submitted: ElapsedTimer
}
impl TelemetryTimers {
    fn new(_data: &mut Vec<TelemetryData>) -> TelemetryTimers {
        //data.push(TelemetryData::new_f64("telemetry avg wait".to_string(), 0.0));
        //data.push(TelemetryData::new_f64("telemetry avg new data".to_string(), 0.0));
        //data.push(TelemetryData::new_f64("telemetry avg publish".to_string(), 0.0));
        TelemetryTimers {
            wait_time_avg: MovingAverage::new(1000),
            new_data_time_avg: MovingAverage::new(1000),
            publish_time_avg: MovingAverage::new(1000),
            current: ElapsedTimer::new(),
            last_submitted: ElapsedTimer::new()
        }
    }
    fn start(&mut self) {
        self.current.duration_reset();
    }
    fn waited(&mut self) {
        self.wait_time_avg.update(self.current.duration_reset().subsec_micros() as f32 / 1000.0);
    }
    fn new_data(&mut self) {
        self.new_data_time_avg.update(self.current.duration_reset().subsec_micros() as f32 / 1000.0);
    }
    fn publish(&mut self) {
        self.publish_time_avg.update(self.current.duration_reset().subsec_micros() as f32 / 1000.0);
    }
    fn reset_last_sent(&mut self) {
        self.last_submitted.duration_reset();
    }
    fn need_to_send(&self) -> bool {
        self.last_submitted.duration().subsec_millis() > 62//16 hz
    }
    fn push(&self, _data: &mut Vec<TelemetryData>) {
        //data[0].data = TelemetryType::F64(self.wait_time_avg.avg() as f64);
        //data[1].data = TelemetryType::F64(self.new_data_time_avg.avg() as f64);
        //data[2].data = TelemetryType::F64(self.publish_time_avg.avg() as f64);
    }
}