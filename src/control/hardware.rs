use std::cmp::PartialEq;
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicU32, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use wait_on_address::AtomicWait;
use crate::control::hardware::Direction::{Backwards, Forwards};
use crate::sdk_proxy::proxy::Proxy;
use crate::serialization::command::Command::LynxCommand;
use crate::serialization::command_utils::Module;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand::{LynxGetBulkDataCommand, LynxGetBulkDataResponse, LynxSetMotorChannelModeCommand, LynxSetMotorPowerCommand, LynxSetServoPulseWidthCommand};
use crate::serialization::lynx_commands::base_lynx_command::LynxCommandData;
use crate::serialization::lynx_commands::lynx_commands::{DcMotorRunMode, DcMotorZeroPowerBehavior, LynxGetBulkDataCommandData, LynxGetBulkDataResponseData, LynxSetMotorChannelModeCommandData, LynxSetMotorPowerCommandData, LynxSetServoPulseWidthCommandData};
use crate::serialization::packet::Packet;
use crate::{HUB_0, HUB_1};
use crate::sdk_proxy::send_proxy::register_packet;
use crate::serialization::command::Command;

///this should be on by default, it represents whether we do motor power caching or not.
/// it should only be turned off for debugging purposes, where you want to test worst-case timing
pub static DO_MOTOR_CACHING: AtomicBool = AtomicBool::new(true);
pub static MOTOR_CACHING_THRESHOLD: AtomicI16 = AtomicI16::new(0);
type Data = LynxGetBulkDataResponseData;
#[derive(Debug)]
pub struct LynxHub {
    pub module: Module,
    last_motor_powers: [AtomicI16; 4],
    ///true = forward
    motor_directions: [AtomicBool; 4],
    motor_zero_power_behaviors: [AtomicU8; 4],
    motor_modes: [AtomicU8; 4],
    pub(crate) is_over_rs: Option<(Sender<()>, Receiver<()>)>,
    pub sender: Sender<Packet>,
    pub sdk_proxy: UnderlyingHw,
    pub receiver: Receiver<Packet>
}
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    Forwards,
    Backwards
}
#[derive(Debug)]
pub enum UnderlyingHw {
    DirectProxy(Proxy),
    OtherHub(&'static LynxHub)
}
impl Direction {
    fn mult(&self) -> f32 {
        match self {
            Forwards => {1.0}
            Backwards => {-1.0}
        }
    }
}

impl LynxHub {
    pub fn new(module: Module, out: Sender<Packet>, sdk_proxy: UnderlyingHw, receiver: Receiver<Packet>, is_over_rs: bool) -> LynxHub {
        let bd = bounded(1);
        bd.0.send(()).unwrap();
        LynxHub {
            module,
            last_motor_powers: [AtomicI16::new(0), AtomicI16::new(0), AtomicI16::new(0), AtomicI16::new(0)],
            motor_directions: [AtomicBool::new(true), AtomicBool::new(true), AtomicBool::new(true), AtomicBool::new(true)],
            sender: out,
            sdk_proxy,
            receiver,
            motor_zero_power_behaviors: [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)],
            motor_modes: [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)],
            is_over_rs: match is_over_rs {
                true => {Some(bd)}
                false => {None}
            }
        }
    }
    pub fn get_motor_direction(&self, motor: usize) -> Direction {
        if self.motor_directions[motor].load(Ordering::SeqCst) {
            Forwards
        } else {Backwards}
    }
    pub fn should_consume(&self, data: &Packet) -> Option<Data> {
        if data.src_module_addr == self.module.module_addr {
            match &data.payload_data {
                LynxCommand(it) => {
                    match it.command {
                        LynxGetBulkDataResponse(mut it) => {
                            for i in 0..self.motor_directions.len() {
                                if self.get_motor_direction(i) == Backwards {
                                    it.motors[i].position *= -1;
                                    it.motors[i].velocity *= -1;
                                }
                            }
                            Some(it)
                        }
                        _ => None
                    }
                }
                _ => { None }
            }
        } else { None }
    }
    pub fn notify_receive_packet(&'static self) {
        if let Some(w) = &self.is_over_rs {
            if w.0.is_full() {
                log::info!("already have data in rs485 unblocker!")
            } else {
                w.0.send(()).expect("could not send rs485 unblocker");
            }
            log::trace!("notify all called! is full? {}", w.0.is_full())
        }
    }
    pub fn notify_send_packet(&'static self, pck: &Packet) {
        if let Some(w) = &self.is_over_rs {
            log::trace!("maybe going to wait on rs485, nm:{}, data:{}", pck.message_number, pck.payload_data);
            w.1.recv().expect("could not receive from rs485 blocker");
            log::trace!("done waiting on rs485, nm:{}", pck.message_number);
        }
    }
    pub fn send_packet(&'static self, mut packet: Packet) -> u8 {
        let proxy = self.get_proxy();
        let num = register_packet(&proxy.message_list);
        packet.message_number = num;
        packet.reference_number = num;
        packet.checksum = packet.checksum();
        log::trace!("waiting for packet: d:{} --- {}", packet.dest_module_addr, packet);
        self.notify_send_packet(&packet);
        log::trace!("done waiting for packet: {:?}", packet.dest_module_addr);
        self.sender.send(packet).expect("could not send packet in hub!");
        num
    }
    pub fn get_proxy(&'static self) -> &'static Proxy {
        match &self.sdk_proxy {
            UnderlyingHw::DirectProxy(it) => it,
            UnderlyingHw::OtherHub(it) => it.get_proxy()
        }
    }
    ///construct and then send a lynx command packet. return the message number the response will have
    pub fn send_lynx_packet(&'static self, lynx_command: crate::serialization::lynx_commands::base_lynx_command::LynxCommand) -> u8 {
        let resp = Command::LynxCommand(LynxCommandData { module: &self.module, command: lynx_command });
        let packet = Packet::new_full(resp, self.module.module_addr, 0, 0, 0);
        log::trace!("lynx writing packet {}", packet);
        self.send_packet(packet)
    }
    pub fn send_motor_command(&'static self, motor: u8, mut power: f32) {
        power = self.get_motor_direction(motor as usize).mult() * power;
        if power.abs() > 1.0 {
            log::info!("attempted to fire motor w/ power > 1");
            power = if power > 1.0 {1.0} else {-1.0};
        }
        let calculated = (power * (i16::MAX as f32)) as i16;
        self.send_motor_command_i16(motor, calculated);
    }
    pub fn send_motor_command_i16(&'static self, motor: u8, power: i16) {
        if (self.last_motor_powers[motor as usize].load(Ordering::SeqCst) - power).abs() <= MOTOR_CACHING_THRESHOLD.load(Ordering::SeqCst) {
            if DO_MOTOR_CACHING.load(Ordering::SeqCst) {
                return; //don't need to send this it's literally the same value!
            }
        }
        self.last_motor_powers[motor as usize].store(power, Ordering::SeqCst);
        let lynx_command = LynxSetMotorPowerCommand(LynxSetMotorPowerCommandData { motor, power});
        self.send_lynx_packet(lynx_command);
        //let packet = Packet::new(lynx_command.to_command(&self.module), self.module.module_addr, 0);
        //self.send_packet(packet);
    }
    ///here be dragons! This is an entirely untested method, so... idk just don't expect this to work at all.
    pub fn send_servo_command(&'static self, servo: u8, position: f32) {
        let pwm = position * 1800.0 + 600.0;//these taken from the default pwm values. I think
        let pwm = 65535_f32.min(pwm).max(1_f32) as u16;
        let lynx_command = LynxSetServoPulseWidthCommand(LynxSetServoPulseWidthCommandData { servo, pulse_width: pwm});
        self.send_lynx_packet(lynx_command);
    }
    pub fn send_bulk_read(&'static self) {
        let lynx_command = LynxGetBulkDataCommand(LynxGetBulkDataCommandData {});
        self.send_lynx_packet(lynx_command);
    }
    pub fn set_direction(&self, motor: u8, direction: Direction) {
        self.motor_directions[motor as usize].store(direction == Forwards, Ordering::SeqCst);
    }

    pub fn set_zero_power_behavior(&'static self, motor: u8, zpb: DcMotorZeroPowerBehavior) {
        self.set_behavior(motor, self.get_motor_mode(motor), zpb);
    }
    pub fn set_motor_mode(&'static self, motor: u8, mode: DcMotorRunMode) {
        self.set_behavior(motor, mode, self.get_zero_power_behavior(motor));
    }
    pub fn set_behavior(&'static self, motor: u8, run_mode: DcMotorRunMode, zero_power_behavior: DcMotorZeroPowerBehavior) {
        let lynx_command = LynxSetMotorChannelModeCommand(LynxSetMotorChannelModeCommandData {
            motor,
            run_mode,
            zero_power_behavior,
        });
        self.send_lynx_packet(lynx_command);

        self.set_motor_mode_inner_(motor, run_mode);
        self.set_zero_power_behavior_inner_(motor, zero_power_behavior);
    }
    ///Returns whether the packet was actually sent
    pub(crate) fn send_from_sdk(&'static self, packet: Packet, from_another_proxy: Option<&'static LynxHub>) -> bool {
        //this function sends on data we just got from the sdk. it's here because the ex hub may not
        //have the same underlying serial port as the ctrl hub, so we determine which one to look at
        //and call it.
        match &self.sdk_proxy {
            UnderlyingHw::DirectProxy(sdk_proxy) => {
                sdk_proxy.write(packet, from_another_proxy)
            }
            UnderlyingHw::OtherHub(it) => {
                if !it.send_from_sdk(packet, if self.is_over_rs.is_some() {Some(self)} else {None}) {
                    //self.notify_receive_packet(); //TODO
                }
                true
            }
        }
    }
    pub(crate) fn read_for_sdk(&self, len: i32) -> Vec<u8> {
        match &self.sdk_proxy {
            UnderlyingHw::DirectProxy(it) => {it.read(len as usize)}
            UnderlyingHw::OtherHub(it) => {it.read_for_sdk(len)}
        }
    }
    pub fn get_for_id(id: u8) -> &'static LynxHub {
        Self::get_for_id_careful(id).expect("get_for_id_careful found no hub 0!")
    }
    pub fn get_for_id_careful(id: u8) -> Option<&'static LynxHub> {
        if let Some(hub) = HUB_1.get() {
            if hub.module.module_addr == id {
                Some(hub)
            } else {
                Some(HUB_0.get().expect("no hub 0!"))
            }
        } else {
            HUB_0.get()
        }
    }

    pub fn get_zero_power_behavior(&self, motor: u8) -> DcMotorZeroPowerBehavior {
        let num = self.motor_zero_power_behaviors[motor as usize].load(Ordering::SeqCst);
        DcMotorZeroPowerBehavior::try_from(num).expect("invalid zero power behavior")
    }
    pub fn get_motor_mode(&self, motor: u8) -> DcMotorRunMode {
        let num = self.motor_modes[motor as usize].load(Ordering::SeqCst);
        DcMotorRunMode::try_from(num).expect("invalid motor run mode")
    }
    pub fn set_zero_power_behavior_inner_(&self, motor: u8, zpb: DcMotorZeroPowerBehavior) {
        self.motor_zero_power_behaviors[motor as usize].store(zpb as u8, Ordering::SeqCst);
    }
    pub fn set_motor_mode_inner_(&self, motor: u8, mm: DcMotorRunMode) {
        self.motor_modes[motor as usize].store(mm as u8, Ordering::SeqCst);
    }
}