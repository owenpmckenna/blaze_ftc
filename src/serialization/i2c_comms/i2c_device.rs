use std::fmt::Debug;
use std::ops::Add;
use crate::control::hardware::LynxHub;
use crate::control::robot::Robot;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand;
use crate::serialization::lynx_commands::lynx_commands::{LynxI2CReadStatusQueryCommandData, LynxI2CSingleByteWriteCommandData, LynxI2cWriteMultipleBytesCommandData};
use crate::serialization::packet::Packet;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::Sender;
use crate::serialization::command::Command;
use crate::serialization::i2c_comms::pinpoint_i2c::{PinpointRegister, PinpointSnapshot};

pub enum I2CDeviceResult<T> {
    Data(T),
    Nack(String),
    Packet(Packet)
}
pub trait I2CDevice<T, R> : Send + Sync + UnwindSafe + RefUnwindSafe where R: Into<u8>, T: From<Vec<u8>> {
    //fn try_interpret_response(&mut self, packet: Packet) -> I2CDeviceResult<T>;
    ///hub, bus, i2c addr
    fn get_location(&self) -> (&'static LynxHub, u8, u8);
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
    fn write_data(&self, register: u8, data: &[u8]) -> &Self {
        thread::sleep(Duration::from_millis(6));
        let (hub, bus, i2c_addr) = self.get_location();
        log::trace!("writing i2c data: bus{} addr{} reg{} dat{:?}", bus, i2c_addr, register, data);
        Self::write_data_i(hub, bus, i2c_addr, register, data);
        thread::sleep(Duration::from_millis(6));//wait so the command will go through. this is a kludge
        self
    }
    fn write_register<B>(&self, register: R, number: B) -> &Self where B: ToLeBytes + Debug {
        self.write_data(register.into(), &number.to_le_bytes_vec())
    }
    fn get_packet_utils(&self) -> (&Arc<Mutex<Vec<u8>>>, &Option<Sender<Instant>>);
    fn add_pif(&self, id: u8) {
        let (packets_in_flight, _) = self.get_packet_utils();
        log::trace!("adding pif {}", id);
        let mut pif = packets_in_flight.lock().expect("could not lock pif list");
        pif.push(id);
    }
    fn is_pif(&self, id: u8) -> bool {
        let (packets_in_flight, _) = self.get_packet_utils();
        let mut pif = packets_in_flight.lock().expect("could not lock i2c pif list 0");
        if let Some(index) = pif.iter().position(|&x| x == id) {
            pif.remove(index);
            true
        } else { false }
    }
    fn try_interpret_response(&mut self, packet: Packet) -> I2CDeviceResult<T> {
        log::trace!("trying to interpret i2c data... from packet rn{} {:?}", packet.reference_number, packet);
        if self.is_pif(packet.reference_number) {
            if let Command::LynxCommand(cmd) = &packet.payload_data {
                log::trace!("got i2c PIF {} w/ data", packet.reference_number);
                if let LynxCommand::LynxI2CReadStatusQueryResponse(_) = &cmd.command {
                    let resp = if let Command::LynxCommand(it) = packet.payload_data && let LynxCommand::LynxI2CReadStatusQueryResponse(it) = it.command {
                        it
                    } else {panic!("wierdness has happened")};
                    let data = T::from(resp.data);
                    //log::trace!("i2c packet {} was data! {:?}", packet.reference_number, data);
                    return I2CDeviceResult::Data(data);
                }
            } else if let Command::Nack(reason) = &packet.payload_data {
                log::trace!("got i2c PIF {} w/ NACK", packet.reference_number);
                let (_, mistake_alert_sender) = self.get_packet_utils();
                //i2c writing not done (fire_read failed). consume and send another packet
                //remind scheduler to not send any packets for the next couple millis.
                if let Some(channel) = &mistake_alert_sender {
                    log::trace!("Got i2c NACK, warning scheduler to slow down.");
                    let _ = channel.send(Instant::now().add(Duration::from_millis(3)));
                }
                self.fire_read();
                return I2CDeviceResult::Nack(reason.to_string())
            } else if let Command::Ack(_) = &packet.payload_data {
                log::trace!("got i2c PIF {} w/ ACK", packet.reference_number);
                //i2c writing done (fire_bulk_read succeeded). consume and send another packet
                self.fire_read();
                return I2CDeviceResult::Nack("".to_string())
            }
            log::trace!("got i2c packet that has no data and isn't a nack, seemingly. packet: {:?}", packet)
        }
        I2CDeviceResult::Packet(packet)
    }
    fn fire_read(&self) {
        log::trace!("firing i2c read req");
        let (hub, bus, _) = self.get_location();
        let cmd = LynxCommand::LynxI2CReadStatusQueryCommand(LynxI2CReadStatusQueryCommandData {i2c_bus: bus});
        self.add_pif(hub.send_lynx_packet(cmd));
    }
}
pub trait I2CDeviceHandler<Device, T, R>: Send + Sync + UnwindSafe + RefUnwindSafe where Device: I2CDevice<T, R>, R: Into<u8>, T: From<Vec<u8>> {
    fn handle(&mut self, robot: &Robot, device: &mut Box<Device>, data: &T);
}
pub(crate) struct I2CDevicePair<Device, T, R> where Device: I2CDevice<T, R>, R: Into<u8>, T: From<Vec<u8>> {
    pub(crate) device: Box<Device>,
    pub(crate) handlers: Vec<Box<dyn I2CDeviceHandler<Device, T, R>>>
}
pub(crate) trait I2CConsumer: Send + Sync + UnwindSafe + RefUnwindSafe {
    fn maybe_consume_packet(&mut self, robot: &Robot, packet: Packet) -> Option<Packet>;
}
impl<Device, T, R> I2CConsumer for I2CDevicePair<Device, T, R> where Device: I2CDevice<T, R>, R: Into<u8>, T: From<Vec<u8>> {
    fn maybe_consume_packet(&mut self, robot: &Robot, packet: Packet) -> Option<Packet> {
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

