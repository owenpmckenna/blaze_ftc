use crate::control::robot::Robot;
use crate::serialization::i2c_comms::i2c_device::I2CDeviceHandler;
use crate::serialization::i2c_comms::pinpoint_i2c::{PinpointI2C, PinpointSnapshot};
use crate::{BLAZEFTC_CLASS, JAVA_VM};
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use jni::errors::Error;
use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::{jni_sig, jni_str, Env, JValue};
use std::thread;
use crate::telemetry::telemetry::{get_class_loader, load_class};

pub struct JNICrossPinpointHandler {
    snapshot_tx: Sender<PinpointSnapshot>,
    resend_rx: Receiver<bool>,
    sending: bool
}
impl I2CDeviceHandler<PinpointI2C, PinpointSnapshot> for JNICrossPinpointHandler {
    fn handle(&mut self, _: &Robot, device: &mut Box<PinpointI2C>, data: &PinpointSnapshot) {
        match self.resend_rx.try_recv() {
            Ok(it) => {if self.sending && !it { self.sending = false; }}
            Err(_) => {self.sending = false;}
        }
        if self.sending {
            device.fire_bulk_read_request();
        }
        if let Err(_) =  self.snapshot_tx.send((*data).clone()) {
            self.sending = false;
        }
    }
    //o=outer, i=inner
}
impl JNICrossPinpointHandler {
    pub fn put_on_robot(robot: &mut Robot) -> Option<()> {
        let hub = if robot.get_property("internalPinpointHub")?.eq_ignore_ascii_case("hub0") {
            robot.hub_0
        } else { robot.hub_1? };

        let pinpoint = if let Ok(bus_id) = robot.get_property("internalPinpointBus")?.parse::<u8>() {
            //0 <= bus_id <= 4, I think
            //i2c addr is hardcoded
            PinpointI2C::new(hub, bus_id, 49)
        } else {return None;};
        pinpoint.fire_bulk_read_request();

        let cb_name = robot.get_property("internalPinpointCallbackName")?;
        let handler = Self::new(cb_name, robot);
        robot.add_i2c_device(Box::new(pinpoint), vec![Box::new(handler)]);
        Some(())
    }
    fn java_thread(env: &mut Env, name: String, resend_tx: Sender<bool>, snapshot_rx: Receiver<PinpointSnapshot>, kill_rx: Receiver<()>) {
        let object: &JObject = BLAZEFTC_CLASS.get().unwrap().as_obj();
        let class_loader = get_class_loader(env, object);
        let blazeftc_class = load_class(env, &class_loader, "dev.anygeneric.blazeftc.BlazeFTC");

        let jstr: JString = env
            .new_string(name)
            .expect("could not create java string - pinpoint jni");
        loop {
            select! {
                recv(kill_rx) -> msg => {
                    let _ = resend_tx.send(false);
                    return;
                }
                recv(snapshot_rx) -> msg => {
                    let msg = msg.unwrap();
                    let arr = msg.to_bytes();
                    let arr = env.byte_array_from_slice(&arr)
                        .expect("could not create java bytes - pinpoint jni");

                    let output = env.call_static_method(&blazeftc_class, jni_str!("sendBytes"),
                            jni_sig!("(Ljava/lang/String;[B)[B"),
                            &[JValue::Object(&jstr), JValue::Object(&arr)])
                        .expect("call to sendBytes fail - pinpoint jni")
                        .l().expect("could not get obj from sendBytes - pinpoint jni");
                    let bytes = JByteArray::cast_local(env, output)
                        .expect("could not get bytes - pinpoint jni");
                    let out_bytes = env.convert_byte_array(bytes).expect("could not get bytes 2 - pinpoint jni");
                    if out_bytes.len() == 1 && out_bytes[0] == 0 {
                        let _ = resend_tx.send(false);
                        return;
                    }
                    //so... we shut down if we get a kill channel signal *or* java sends us a [0] array.
                }
            }
        }
    }
    fn new(name: String, robot: &mut Robot) -> JNICrossPinpointHandler {
        let (snapshot_tx, snapshot_rx) = unbounded();
        let (resend_tx, resend_rx) = unbounded();
        let (kill_tx, kill_rx) = unbounded();
        robot.add_kill_signal_sender(kill_tx);
        let _ = thread::spawn(move || {
            let vm = JAVA_VM.get().unwrap();
            vm.attach_current_thread(move |env: &mut Env| -> Result<(), Error> {
                Self::java_thread(env, name, resend_tx, snapshot_rx, kill_rx);
                Ok(())
            }).unwrap();
        });
        JNICrossPinpointHandler { snapshot_tx, resend_rx, sending: true }
    }
}