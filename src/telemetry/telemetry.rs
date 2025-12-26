use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use jni::AttachGuard;
use jni::descriptors::Desc;
use jni::objects::{GlobalRef, JClass, JObject, JValue};
use log::log;
use crate::{catch, ElapsedTimer, MovingAverage, BLAZEFTC_CLASS, JAVA_VM};

//note: "timer" is mostly pointless debug code sorry
#[derive(Clone)]
pub struct Telemetry {
    send: Sender<TelemetryData>
}
impl Telemetry {
    pub fn new(running: &'static AtomicBool) -> Telemetry {
        let (tx, rx) = unbounded();
        thread::spawn(move || {
            catch(move || {
                log::info!("started telemetry!");
                let vm = JAVA_VM.get().unwrap();
                let env = vm.attach_current_thread().unwrap();
                //let class = env.find_class("dev/anygeneric/blazeftc/BlazeFTC")?;
                let object: JObject = BLAZEFTC_CLASS.get().unwrap().as_obj();
                let mut data = vec![TelemetryData::new_f64("".to_string(), 0f64); 0];
                let mut timer = TelemetryTimers::new(&mut data);
                while running.load(Ordering::SeqCst) {
                    Telemetry::execute(&rx, &mut data, &mut timer, &env, &object);
                }
                log::info!("ended telemetry!")
            }, "telemetry thread");
        });
        Telemetry {send: tx}
    }
    pub fn add_string(&self, name: &str, data: &str) {
        self.send.send(
            TelemetryData::new_string(name.to_string(), data.to_string())
        ).unwrap();
    }
    pub fn add_f64(&self, name: &str, data: f64) {
        self.send.send(
            TelemetryData::new_f64(name.to_string(), data)
        ).unwrap();
    }
    pub fn add_i64(&self, name: &str, data: i64) {
        self.send.send(
            TelemetryData::new_i64(name.to_string(), data)
        ).unwrap();
    }
    fn execute(rx: &Receiver<TelemetryData>, cache: &mut Vec<TelemetryData>, timer: &mut TelemetryTimers, env: &AttachGuard, obj: &JObject) {
        timer.start();
        let mut data = match rx.recv_timeout(Duration::from_millis(62)) { //16 hz. ds processes at 4 hz so this means you do get updated data
            Ok(it) => {Some(it)}
            Err(it) => {None}
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
                Self::send_all_telemetry(cache, env, obj).unwrap();
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
    fn send_all_telemetry(cache: &mut Vec<TelemetryData>, env: &AttachGuard, obj: &JObject) -> Result<(), Box<dyn std::error::Error>> {
        /*send all telemetry*/
        for i in cache {
            log::trace!("logging data: {}", i.name);
            let j_name: JValue = if i.string_jni_loc.is_none() {
                let name = env.new_string(i.name.clone())?;
                i.string_jni_loc = Some(env.new_global_ref(name)?);
                name.into()
            } else {
                i.string_jni_loc.as_ref().unwrap().as_obj().into()
            };
            match &i.data {
                TelemetryType::String(it) => {
                    let sig = "(Ljava/lang/String;Ljava/lang/Object;)V";
                    let j_data = env.new_string(it.clone())?;
                    env.call_method(*obj, "addData", sig, &[j_name, j_data.into()])?;
                    env.delete_local_ref(j_data.into())?;
                }
                TelemetryType::F64(it) => {
                    let sig = "(Ljava/lang/String;D)V";
                    env.call_method(*obj, "addData", sig, &[j_name, (*it).into()])?;
                }
                TelemetryType::I64(it) => {
                    let sig = "(Ljava/lang/String;J)V";
                    env.call_method(*obj, "addData", sig, &[j_name.into(), (*it).into()])?;
                }
            }
        }
        env.call_method(*obj, "update", "()V", &[])?;
        Ok(())
    }
}
#[derive(Clone)]
struct TelemetryData {
    name: String,
    string_jni_loc: Option<GlobalRef>,
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
    fn new(data: &mut Vec<TelemetryData>) -> TelemetryTimers {
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
    fn push(&self, data: &mut Vec<TelemetryData>) {
        //data[0].data = TelemetryType::F64(self.wait_time_avg.avg() as f64);
        //data[1].data = TelemetryType::F64(self.new_data_time_avg.avg() as f64);
        //data[2].data = TelemetryType::F64(self.publish_time_avg.avg() as f64);
    }
}