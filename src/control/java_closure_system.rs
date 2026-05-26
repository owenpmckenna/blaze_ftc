use crate::control::robot::Robot;
use crate::serialization::i2c_comms::i2c_device::I2CDeviceHandler;
use crate::serialization::i2c_comms::pinpoint_i2c::{PinpointI2C, PinpointSnapshot};
use crate::{BLAZEFTC_CLASS, JAVA_VM};
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use jni::errors::Error;
use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::{jni_sig, jni_str, Env, JValue};
use std::thread;
use jni::sys::{jbyte, jsize};
use crate::telemetry::telemetry::{get_class_loader, load_class};

pub struct JNICrossPinpointHandler {
    snapshot_tx: Sender<PinpointSnapshot>,
    resend_rx: Receiver<bool>,
    sending: bool,
    first: bool
}
impl I2CDeviceHandler<PinpointI2C, PinpointSnapshot> for JNICrossPinpointHandler {
    fn handle(&mut self, _: &Robot, device: &mut Box<PinpointI2C>, data: &PinpointSnapshot) {
        if self.first {
            log::info!("got a pinpoint data!")
        }
        match self.resend_rx.try_recv() {
            Ok(it) => {if self.sending && !it { self.sending = false; }}
            Err(it) => {
                log::info!("resend_rx null! err: {}", it);
                //self.sending = false; //TODO uncomment
            }
        }
        if self.sending {
            device.fire_bulk_read_request();
        }
        if let Err(it) =  self.snapshot_tx.send((*data).clone()) {
            log::info!("snapshot tx null!!! {}", it);
            //self.sending = false;
        }
        self.first = false;
    }
    //o=outer, i=inner
}
impl JNICrossPinpointHandler {
    pub fn put_on_robot(robot: &mut Robot) -> Option<()> {
        log::info!("running jni put on robot: {:?}, {:?}, {:?}",
            robot.get_property("internalPinpointHub"),
            robot.get_property("internalPinpointBus"),
            robot.get_property("internalPinpointCallbackName")
        );
        let hub = if robot.get_property("internalPinpointHub")?.eq_ignore_ascii_case("hub0") {
            robot.hub_0
        } else { robot.hub_1? };

        log::info!("abt to get pinpoint bus id");
        let pinpoint = if let Ok(bus_id) = robot.get_property("internalPinpointBus")?.parse::<u8>() {
            //0 <= bus_id <= 4, I think
            //i2c addr is hardcoded
            PinpointI2C::new(hub, bus_id, 49)
        } else {return None;};
        log::info!("firing pinpoint!");
        pinpoint.fire_bulk_read_request();

        let cb_name = robot.get_property("internalPinpointCallbackName")?;
        let handler = Self::new(cb_name, robot);
        robot.add_i2c_device(Box::new(pinpoint), vec![Box::new(handler)]);
        Some(())
    }
    fn java_thread(env: &mut Env, name: String, resend_tx: Sender<bool>, snapshot_rx: Receiver<PinpointSnapshot>, kill_rx: Receiver<()>) {
        log::info!("running java thread!!!");
        let resend_tx = resend_tx;//force keep it? idk.
        let object: &JObject = BLAZEFTC_CLASS.get().unwrap().as_obj();
        let class_loader = get_class_loader(env, object);
        let blazeftc_class = load_class(env, &class_loader, "dev.anygeneric.blazeftc.BlazeFTC");

        let jstr: JString = env
            .new_string(name)
            .expect("could not create java string - pinpoint jni");

        let arr = env.byte_array_from_slice(&vec![0b0; 40])
            .expect("could not create java bytes - pinpoint jni");
        loop {
            select! {
                recv(kill_rx) -> msg => {
                    let _ = resend_tx.send(false);
                    log::info!("java thread got kill signal!");
                    return;
                }
                recv(snapshot_rx) -> msg => {
                    let msg = msg.unwrap();
                    let in_bytes: Vec<jbyte> = msg.to_bytes().into_iter().map(|it| it as jbyte).collect();
                    arr.set_region(env, in_bytes.len() as jsize, in_bytes.as_slice())
                        .expect("could not set region in pinpoint 0");

                    let output = env.call_static_method(&blazeftc_class, jni_str!("sendBytes"),
                            jni_sig!("(Ljava/lang/String;[B)[B"),
                            &[JValue::Object(&jstr), JValue::Object(&arr)])
                        .expect("call to sendBytes fail - pinpoint jni")
                        .l().expect("could not get obj from sendBytes - pinpoint jni");
                    let output = JByteArray::cast_local(env, output)
                        .expect("could not get bytes - pinpoint jni");
                    let byte_len = output.len(env).expect("could not get byte len");
                    let mut bytes = vec![0b0 as jbyte; byte_len];
                    output.get_region(env, byte_len as jsize, bytes.as_mut_slice()).expect("");
                    env.delete_local_ref(output);
                    if bytes.len() == 1 && (bytes[0] as u8) == 0 {
                        let _ = resend_tx.send(false);
                        log::info!("got out bytes 0!!!");
                        return;
                    }
                    //so... we shut down if we get a kill channel signal *or* java sends us a [0] array.
                }
            }
        }
    }
    fn new(name: String, robot: &mut Robot) -> JNICrossPinpointHandler {
        log::info!("creating new jni cross handler!");
        let (snapshot_tx, snapshot_rx) = unbounded();
        let (resend_tx, resend_rx) = unbounded();
        let (kill_tx, kill_rx) = unbounded();
        robot.add_kill_signal_sender(kill_tx);
        let _ = thread::spawn(move || {
            log::info!("spawning java talk thread...");
            let vm = JAVA_VM.get().unwrap();
            vm.attach_current_thread(move |env: &mut Env| -> Result<(), Error> {
                Self::java_thread(env, name, resend_tx, snapshot_rx, kill_rx);
                Ok(())
            }).unwrap();
            log::info!("JAVA THREAD EXITED");
        });
        JNICrossPinpointHandler { snapshot_tx, resend_rx, sending: true, first: true }
    }
}