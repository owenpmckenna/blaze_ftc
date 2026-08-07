use crate::control::robot::{BulkReadHandler, Robot};
use crate::serialization::i2c_comms::i2c_device::I2CDeviceHandler;
use crate::serialization::i2c_comms::pinpoint_i2c::{PinpointI2C, PinpointSnapshot};
use crate::{catch, get_servo_hubs_init_data, BLAZEFTC_CLASS, JAVA_VM};
use crossbeam_channel::{select, unbounded, Receiver, Sender, TryRecvError};
use jni::errors::Error;
use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::{jni_sig, jni_str, Env, JValue};
use std::{str, thread};
use std::time::Duration;
use jni::sys::{jbyte, jsize};
use crate::control::hardware::LynxHub;
use crate::serialization::command_data::CommandData;
use crate::serialization::lynx_commands::lynx_commands::LynxGetBulkDataResponseData;
use crate::telemetry::telemetry::{get_allowed_to_send_dangerous_packets, get_class_loader, load_class};

pub struct JNICrossPinpointHandler {
    snapshot_tx: Sender<PinpointSnapshot>,
    resend_rx: Receiver<bool>,
    sending: bool,
    datas: usize
}
impl I2CDeviceHandler<PinpointI2C, PinpointSnapshot> for JNICrossPinpointHandler {
    fn handle(&mut self, _: &Robot, device: &mut Box<PinpointI2C>, data: &PinpointSnapshot) {
        if self.datas < 5 {
            log::info!("got a pinpoint data! {}", self.datas)
        }
        match self.resend_rx.try_recv() {
            Ok(it) => {
                if self.sending && !it {
                    self.sending = false;
                    log::info!("pinpoint ordered to stop")
                }
                //if !get_allowed_to_send_dangerous_packets() {
                //    self.sending = false;
                //}
            }
            Err(it) => {
                if it.is_disconnected() {
                    //if not disconnected it errored because there's nothing in the channel
                    //on the other hand. if the channel was dc normally, it should have had a normal
                    //false in it which would get processed first so we can ignore this error
                    log::info!("resend_rx null! err: {} ignore", it);
                    //self.sending = false;
                }
            }
        }
        if self.sending {
            device.fire_bulk_read_request();
        }
        if let Err(it) =  self.snapshot_tx.send((*data).clone()) {
            log::info!("snapshot tx null!!! ignoring... {}", it);
            //self.sending = false;
        }
        self.datas += 1;
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
        let mut running = true;
        while running {
            select! {
                recv(kill_rx) -> msg => {
                    let _ = resend_tx.send(false);
                    log::info!("java thread got kill signal!");
                    running = false;
                }
                recv(snapshot_rx) -> mut msg => {
                    while !snapshot_rx.is_empty() {
                        msg = snapshot_rx.recv();
                    }
                    let msg = msg.unwrap();
                    let in_bytes: Vec<jbyte> = msg.to_bytes().into_iter().map(|it| it as jbyte).collect();
                    arr.set_region(env, 0, in_bytes.as_slice())
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
                    output.get_region(env, 0, bytes.as_mut_slice()).expect("could not get region");
                    env.delete_local_ref(output);
                    if bytes.len() == 1 && (bytes[0] as u8) == 0 {
                        let _ = resend_tx.send(false);
                        log::info!("got out bytes 0!!!");
                        running = false;
                    }
                    //so... we shut down if we get a kill channel signal *or* java sends us a [0] array.
                }
            }
        }
        log::info!("java thread returning (pinpoint) dc: {:?}/{:?}", kill_rx.try_recv(), snapshot_rx.try_recv());
    }
    fn new(name: String, robot: &mut Robot) -> JNICrossPinpointHandler {
        log::info!("creating new jni cross handler!");
        let (snapshot_tx, snapshot_rx) = unbounded();
        let (resend_tx, resend_rx) = unbounded();
        let (kill_tx, kill_rx) = unbounded();
        robot.add_kill_signal_sender(kill_tx);
        let _ = thread::spawn(move || {
            catch(move || {
                log::info!("spawning java talk thread...");
                let vm = JAVA_VM.get().unwrap();
                log::info!("java thread result: {:?}", vm.attach_current_thread(move |env: &mut Env| -> Result<(), Error> {
                    Self::java_thread(env, name, resend_tx, snapshot_rx, kill_rx);
                    log::info!("java thread shutting down...");
                    Ok(())
                }));
                log::info!("JAVA THREAD EXITED");
            }, "java closure thread");
        });
        JNICrossPinpointHandler { snapshot_tx, resend_rx, sending: true, datas: 0 }
    }
}

pub struct JNICrossBulkReadHandler {
    bulk_data_tx: Sender<LynxGetBulkDataResponseData>,
    resend_rx: Receiver<bool>,
    sending: bool,
    datas: usize,
    hub: &'static LynxHub
}
impl BulkReadHandler for JNICrossBulkReadHandler {
    fn update(&mut self, robot: &Robot, data: &LynxGetBulkDataResponseData) {
        if self.datas < 5 {
            log::info!("got a br data! {}", self.datas)
        }
        match self.resend_rx.try_recv() {
            Ok(it) => {
                if self.sending && !it {
                    self.sending = false;
                    log::info!("br ordered to stop")
                }
                //if !get_allowed_to_send_dangerous_packets() {
                //    self.sending = false;
                //}
            }
            Err(it) => {
                if it.is_disconnected() {
                    //if not disconnected it errored because there's nothing in the channel
                    //on the other hand. if the channel was dc normally, it should have had a normal
                    //false in it which would get processed first so we can ignore this error
                    log::info!("resend_rx null! err: {} ignore", it);
                    //self.sending = false;
                }
            }
        }
        if self.sending {
            self.hub.send_bulk_read();
        }
        if let Err(it) =  self.bulk_data_tx.send((*data).clone()) {
            log::info!("snapshot tx null!!! ignoring... {}", it);
            //self.sending = false;
        }
        self.datas += 1;
    }
}
impl JNICrossBulkReadHandler {
    pub fn put_on_robot(robot: &mut Robot, is_ctrl: bool) -> Option<()> {
        let ctrl = if is_ctrl { "chub" } else { "exhub" };
        let num = robot.get_property(&format!("attachBulkRead{}", ctrl))?
            .parse().unwrap_or(1);
        let callback_name = robot.get_property(&format!("bulkReadCallbackName{}", ctrl))?;
        log::info!("running jni put on robot: {:?}, {:?}",
            num,
            callback_name
        );
        let hub = if is_ctrl {
            robot.hub_0
        } else { robot.hub_1? };

        log::info!("firing bulk reads!");
        for _ in 0..num {
            hub.send_bulk_read();
            thread::sleep(Duration::from_millis(1));//start them staggered
        }

        let handler = Self::new(callback_name, robot, hub);
        if is_ctrl {robot.add_hub_0_handler(handler)} else {robot.add_hub_1_handler(handler)};
        Some(())
    }
    fn java_thread(env: &mut Env, name: String, resend_tx: Sender<bool>, bulk_read_rx: Receiver<LynxGetBulkDataResponseData>, kill_rx: Receiver<()>) {
        //log::info!("running java thread!!!");
        let resend_tx = resend_tx;//force keep it? idk.
        let object: &JObject = BLAZEFTC_CLASS.get().unwrap().as_obj();
        let class_loader = get_class_loader(env, object);
        let blazeftc_class = load_class(env, &class_loader, "dev.anygeneric.blazeftc.BlazeFTC");

        let jstr: JString = env
            .new_string(name)
            .expect("could not create java string - br jni");

        let arr = env.byte_array_from_slice(&vec![0b0; 34])
            .expect("could not create java bytes - br jni");
        let mut running = true;
        while running {
            select! {
                recv(kill_rx) -> msg => {
                    let _ = resend_tx.send(false);
                    log::info!("java thread got kill signal!");
                    running = false;
                }
                recv(bulk_read_rx) -> mut msg => {
                    while !bulk_read_rx.is_empty() {
                        msg = bulk_read_rx.recv();
                    }
                    let msg = match msg {
                        Ok(it) => {it}
                        Err(it) => {log::info!("br thread dead??? {}", it);
                        let _ = resend_tx.send(false);
                        return;
                        }
                    };
                    let bytes: Vec<u8> = msg.into();
                    let in_bytes: Vec<jbyte> = bytes.into_iter().map(|it| it as jbyte).collect();
                    arr.set_region(env, 0, in_bytes.as_slice())
                        .expect("could not set region in br 0");

                    let output = env.call_static_method(&blazeftc_class, jni_str!("sendBytes"),
                            jni_sig!("(Ljava/lang/String;[B)[B"),
                            &[JValue::Object(&jstr), JValue::Object(&arr)])
                        .expect("call to sendBytes fail - br jni")
                        .l().expect("could not get obj from sendBytes - br jni");
                    let output = JByteArray::cast_local(env, output)
                        .expect("could not get bytes - br jni");
                    let byte_len = output.len(env).expect("could not get byte len");
                    let mut bytes = vec![0b0 as jbyte; byte_len];
                    output.get_region(env, 0, bytes.as_mut_slice()).expect("could not get region");
                    env.delete_local_ref(output);
                    if bytes.len() == 1 && (bytes[0] as u8) == 0 {
                        let _ = resend_tx.send(false);
                        log::info!("got out bytes 0!!!");
                        running = false;
                    }
                    //so... we shut down if we get a kill channel signal *or* java sends us a [0] array.
                }
            }
        }
        //log::info!("java thread returning (br) dc: {:?}/{:?}", kill_rx.try_recv(), bulk_read_rx.try_recv());
    }
    fn new(name: String, robot: &mut Robot, hub: &'static LynxHub) -> Self {
        log::info!("creating new jni cross handler! (br)");
        let (bulk_data_tx, bulk_data_rx) = unbounded();
        let (resend_tx, resend_rx) = unbounded();
        let (kill_tx, kill_rx) = unbounded();
        robot.add_kill_signal_sender(kill_tx);
        let _ = thread::spawn(move || {
            catch(move || {
                log::info!("spawning java talk thread... br");
                let vm = JAVA_VM.get().unwrap();
                log::info!("java br thread result: {:?}", vm.attach_current_thread(move |env: &mut Env| -> Result<(), Error> {
                    Self::java_thread(env, name, resend_tx, bulk_data_rx, kill_rx);
                    log::info!("java br thread shutting down...");
                    Ok(())
                }));
                log::info!("JAVA THREAD EXITED br");
            }, "java closure thread");
        });
        Self { bulk_data_tx, resend_rx, sending: true, datas: 0, hub }
    }
}
