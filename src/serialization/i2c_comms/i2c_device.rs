use std::fmt::Debug;
use std::panic::{RefUnwindSafe, UnwindSafe};
use crate::control::hardware::LynxHub;
use crate::control::robot::{Robot, ThreadSafe};
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand;
use crate::serialization::lynx_commands::lynx_commands::{LynxI2CSingleByteWriteCommandData, LynxI2cWriteMultipleBytesCommandData};
use crate::serialization::packet::Packet;

pub enum I2CDeviceResult<T> {
    Data(T),
    Nack(String),
    Packet(Packet)
}
pub trait I2CDevice<T> : Send + Sync + UnwindSafe + RefUnwindSafe {
    fn try_interpret_response(&mut self, packet: Packet) -> I2CDeviceResult<T>;
    fn write_data_i(lynx_hub: &'static LynxHub, i2c_bus: u8, i2c_addr_7bit: u8, register: u8, data: &[u8]) {
        let mut payload = vec![register; data.len() + 1];
        payload[1..].copy_from_slice(data);
        let packet = if data.len() == 1 {
            LynxCommand::LynxI2CSingleByteWriteCommand(LynxI2CSingleByteWriteCommandData {
                i2c_bus,
                i2c_addr_7bit,
                value: payload[0],
            })
        } else {
            LynxCommand::LynxI2cWriteMultipleBytesCommand(LynxI2cWriteMultipleBytesCommandData {
                i2c_bus,
                i2c_addr_7bit,
                payload,
            })
        };
        lynx_hub.send_lynx_packet(packet);
    }
}
pub trait I2CDeviceHandler<Device, T, Target, StateUpdate>: Send + Sync + UnwindSafe + RefUnwindSafe where Device: I2CDevice<T>, Target: ThreadSafe, StateUpdate: ThreadSafe + Debug {
    fn handle(&mut self, robot: &Robot<Target, StateUpdate>, device: &mut Box<Device>, data: &T);
}
pub(crate) struct I2CDevicePair<Device, T, Target, StateUpdate> where Device: I2CDevice<T>, Target: ThreadSafe, StateUpdate: ThreadSafe + Debug {
    pub(crate) device: Box<Device>,
    pub(crate) handlers: Vec<Box<dyn I2CDeviceHandler<Device, T, Target, StateUpdate>>>
}
pub(crate) trait I2CConsumer<Target, StateUpdate>: Send + Sync + UnwindSafe + RefUnwindSafe where Target: ThreadSafe, StateUpdate: ThreadSafe + Debug {
    fn maybe_consume_packet(&mut self, robot: &Robot<Target, StateUpdate>, packet: Packet) -> Option<Packet>;
}
impl<Device, T, Target, StateUpdate> I2CConsumer<Target, StateUpdate> for I2CDevicePair<Device, T, Target, StateUpdate> where Device: I2CDevice<T>, Target: ThreadSafe, StateUpdate: ThreadSafe + Debug {
    fn maybe_consume_packet(&mut self, robot: &Robot<Target, StateUpdate>, packet: Packet) -> Option<Packet> {
        match self.device.try_interpret_response(packet) {
            I2CDeviceResult::Data(it) => {
                for i in &mut self.handlers {
                    i.handle(robot, &mut self.device, &it);
                }
                None
            },
            I2CDeviceResult::Nack(_) => {None},
            I2CDeviceResult::Packet(it) => {Some(it)}
        }
    }
}

pub trait ToLeBytes {
    fn to_le_bytes_vec(self) -> Vec<u8>;
}
macro_rules! impl_to_le_bytes {
    ($($t:ty),+ $(,)?) => {
        $(
            impl ToLeBytes for $t {
                #[inline]
                fn to_le_bytes_vec(self) -> Vec<u8> {
                    self.to_le_bytes().to_vec()
                }
            }
        )+
    };
}
impl_to_le_bytes!(
    u8, u16, u32, u64, u128,
    i8, i16, i32, i64, i128,
    /*f32,*/ f64
);
impl ToLeBytes for f32 {
    fn to_le_bytes_vec(self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

