use std::any::Any;
use std::cmp::PartialEq;
use std::sync::atomic::{AtomicI16, Ordering};
use crossbeam_channel::Sender;
use crate::control::hardware::Direction::Backwards;
use crate::serialization::command::Command::LynxCommand;
use crate::serialization::command_utils::Module;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand::{LynxGetBulkDataCommand, LynxGetBulkDataResponse, LynxSetMotorPowerCommand, LynxSetServoPositionCommand};
use crate::serialization::lynx_commands::base_lynx_command::LynxCommandData;
use crate::serialization::lynx_commands::lynx_commands::{LynxGetBulkDataCommandData, LynxGetBulkDataResponseData, LynxSetMotorPowerCommandData, LynxSetServoPositionCommandData};
use crate::serialization::packet::Packet;

type Data = LynxGetBulkDataResponseData;
pub struct LynxHub {
    pub module: Module,
    last_motor_powers: [AtomicI16; 4],
    motor_directions: [Direction; 4],
    pub sender: Sender<Packet>
}
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Direction {
    Forwards,
    Backwards
}
impl Direction {
    fn mult(&self) -> f32 {
        match self {
            Direction::Forwards => {1.0}
            Backwards => {-1.0}
        }
    }
}

impl LynxHub {
    pub fn new(module: &Module, out: &Sender<Packet>) -> LynxHub {
        LynxHub {
            module: module.clone(),
            last_motor_powers: [AtomicI16::new(0), AtomicI16::new(0), AtomicI16::new(0), AtomicI16::new(0)],
            motor_directions: [Direction::Forwards; 4],
            sender: out.clone()
        }
    }
    pub fn should_consume(&self, data: &Packet) -> Option<Data> {
        if data.src_module_addr == self.module.module_addr {
            match &data.payload_data {
                LynxCommand(it) => {
                    match it.command {
                        LynxGetBulkDataResponse(mut it) => {
                            for i in 0..self.motor_directions.len() {
                                if self.motor_directions[i] == Backwards {
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
    pub fn from_modules(modules: &'static Vec<Module>, out: &Sender<Packet>) -> Vec<Self> {
        modules.iter()
            .map(|x| Self::new(x, out))
            .collect()
    }
    pub fn send_motor_command(&self, motor: u8, mut power: f32) {
        power = self.motor_directions[motor as usize].mult() * power;
        if power.abs() > 1.0 {
            log::info!("attempted to fire motor w/ power > 1");
            power = if power > 1.0 {1.0} else {-1.0};
        }
        let calculated = (power * (i16::MAX as f32)) as i16;
        if self.last_motor_powers[motor as usize].swap(calculated, Ordering::SeqCst) == calculated {
            return;//don't need to send this it's literally the same value!
        }
        let lynx_command = LynxSetMotorPowerCommand(LynxSetMotorPowerCommandData { motor, power: calculated});
        let packet = Packet::new(lynx_command.to_command(&self.module), self.module.module_addr, 0);
        self.sender.send(packet).unwrap();
    }
    ///here be dragons! This is an entirely untested method, so... idk just don't expect this to work at all.
    pub fn send_servo_command(&self, servo: u8, position: f32) {
        let pwm = position * 1800.0 + 600.0;//these taken from the default pwm values. I think
        let pwm = 65535_f32.min(pwm).max(1_f32) as u16;
        let lynx_command = LynxSetServoPositionCommand(LynxSetServoPositionCommandData { servo, power: pwm});
        let packet = Packet::new(lynx_command.to_command(&self.module), self.module.module_addr, 0);
        self.sender.send(packet).unwrap();
    }
    pub fn send_bulk_read(&self) {
        let lynx_command = LynxGetBulkDataCommand(LynxGetBulkDataCommandData {});
        let packet = Packet::new(lynx_command.to_command(&self.module), self.module.module_addr, 0);
        self.sender.send(packet).unwrap();
    }
    pub fn set_direction(&mut self, motor: u8, direction: Direction) {
        self.motor_directions[motor as usize] = direction;
    }
}