use std::fmt::{Debug};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use num_enum::TryFromPrimitive;
use crate::control::hardware::{Direction, LynxHub};
use crate::serialization::command::Command;
use crate::serialization::i2c_comms::i2c_device::{I2CDevice, I2CDeviceResult, ToLeBytes};
use crate::serialization::lynx_commands::base_lynx_command::{LynxCommand, LynxCommandData};
use crate::serialization::lynx_commands::lynx_commands::{LynxI2CReadStatusQueryCommandData, LynxI2CWriteReadMultipleBytesCommandData};
use crate::serialization::packet::Packet;
use PinpointRegister::*;

pub struct PinpointI2C {
    hub: &'static LynxHub,
    bus: u8,
    i2c_addr: u8,
    packets_in_flight: Mutex<Vec<u8>>
}
impl PinpointI2C {
    fn write_register<T>(&self, register: PinpointRegister, number: T) -> &Self where T: ToLeBytes + Debug {
        log::trace!("writing pinpoint register {:?} with {:?}", register, number);
        self.write_data(register as u8, &number.to_le_bytes_vec())
    }
    fn write_data(&self, register: u8, data: &[u8]) -> &Self {
        thread::sleep(Duration::from_millis(6));
        log::trace!("writing pinpoint data: bus{} addr{} reg{} dat{:?}", self.bus, self.i2c_addr, register, data);
        Self::write_data_i(self.hub, self.bus, self.i2c_addr, register, data);
        thread::sleep(Duration::from_millis(6));//wait so the command will go through. this is a kludge
        self
    }
}
impl I2CDevice<PinpointSnapshot> for PinpointI2C {
    fn try_interpret_response(&mut self, packet: Packet) -> I2CDeviceResult<PinpointSnapshot> {
        log::trace!("trying to interpret pinpoint data... from packet rn{} {:?}", packet.reference_number, packet);
        if self.is_pif(packet.reference_number) {
            if let Command::LynxCommand(cmd) = &packet.payload_data {
                log::trace!("got pinpoint PIF {} w/ data", packet.reference_number);
                if let LynxCommand::LynxI2CReadStatusQueryResponse(resp) = &cmd.command {
                    let data = PinpointSnapshot::new(&resp.data);
                    log::trace!("pinpoint packet {} was data! {:?}", packet.reference_number, data);
                    return I2CDeviceResult::Data(data);
                }
            } else if let Command::Nack(reason) = &packet.payload_data {
                log::trace!("got pinpoint PIF {} w/ NACK", packet.reference_number);
                //i2c writing not done. consume and send another packet
                self.fire_read();
                return I2CDeviceResult::Nack(reason.to_string())
            } else if let Command::Ack(_) = &packet.payload_data {
                log::trace!("got pinpoint PIF {} w/ ACK", packet.reference_number);
                //i2c writing not done. consume and send another packet
                self.fire_read();
                return I2CDeviceResult::Nack("".to_string())
            }
            log::trace!("got pinpoint packet that has no data and isn't a nack, seemingly. packet: {:?}", packet)
        }
        I2CDeviceResult::Packet(packet)
    }
}
impl PinpointI2C {
    const IN_TO_MM: f32 = 25.4;
    pub fn new(hub: &'static LynxHub, bus: u8, i2c_addr: u8) -> PinpointI2C {
        PinpointI2C {
            hub,
            bus,
            i2c_addr,
            packets_in_flight: Mutex::new(vec![]),
        }
    }
    fn add_pif(&self, id: u8) {
        log::trace!("adding pinpoint pif {}", id);
        let mut pif = self.packets_in_flight.lock().expect("could not lock pinpoint pif list");
        pif.push(id);
    }
    fn is_pif(&self, id: u8) -> bool {
        let mut pif = self.packets_in_flight.lock().expect("could not lock pinpoint pif list 0");
        if let Some(index) = pif.iter().position(|&x| x == id) {
            pif.remove(index);
            true
        } else { false }
    }
    pub fn fire_bulk_read_request(&self) {
        log::trace!("firing pinpoint bulk read req");
        let cmd = LynxCommand::LynxI2CWriteReadMultipleBytesCommand(LynxI2CWriteReadMultipleBytesCommandData {
            i2c_bus: self.bus,
            i2c_addr_7bit: self.i2c_addr,
            bytes_to_read: 40,
            i2c_start_addr: 18,//magic value idk
        });
        self.add_pif(self.hub.send_lynx_packet(cmd));
    }
    fn fire_read(&self) {
        log::trace!("firing pinpoint read req");
        let cmd = LynxCommand::LynxI2CReadStatusQueryCommand(LynxI2CReadStatusQueryCommandData {i2c_bus: self.bus});
        self.add_pif(self.hub.send_lynx_packet(cmd));
    }

    pub fn set_pod_offsets(&self, xoffset: f32, yoffset: f32) -> &Self {
        self.write_register(XPodOffset, xoffset * Self::IN_TO_MM);
        self.write_register(YPodOffset, yoffset * Self::IN_TO_MM)
    }
    const SWINGARM_POD: f32 = 13.26291192f32; //ticks per mm for the goBILDA Swingarm Pods
    const FOUR_BAR_POD: f32 = 19.89436789f32; //ticks per mm for the goBILDA 4-Bar Pods --- this taken from the drivers
    pub fn set_encoder_resolution(&self, er: EncoderResolution) -> &Self {
        let out = match er {
            EncoderResolution::SWINGARM_POD => {Self::SWINGARM_POD}
            EncoderResolution::FOURBAR => {Self::FOUR_BAR_POD}
            EncoderResolution::CUSTOM(it) => {it}
        };
        self.write_register(TicksPerMm, out)
    }
    pub fn recalibrate_imu(&self) {
        self.write_register(PinpointRegister::DeviceControl, DeviceControl::RecalibrateImu);
    }
    pub fn reset_pos_and_imu(&self) {
        self.write_register(PinpointRegister::DeviceControl, DeviceControl::ResetPosAndImu);
    }
    pub fn set_encoder_direction(&self, pod: Pod, direction: Direction) -> &Self {
        let dc = if pod == Pod::X {
            match direction {
                Direction::Forwards => {DeviceControl::SetXEncoderForward}
                Direction::Backwards => {DeviceControl::SetXEncoderReversed}
            }
        } else {
            match direction {
                Direction::Forwards => {DeviceControl::SetYEncoderForward}
                Direction::Backwards => {DeviceControl::SetYEncoderReversed}
            }
        };
        self.write_register(PinpointRegister::DeviceControl, dc)
    }
    pub fn set_yaw_resolution(&self, resolution: f32) -> &Self {
        self.write_register(YawScalar, resolution)
    }
    pub fn set_pos(&self, pod: Pod, pos_in: f32) -> &Self {
        self.write_register(match pod { Pod::X => {XPosition} Pod::Y => {YPosition} }, pos_in * Self::IN_TO_MM)
    }
    pub fn set_heading(&self, heading_rad: f32) -> &Self {
        self.write_register(HOrientation, heading_rad)
    }
}
#[derive(Debug)]
pub struct PinpointSnapshot {
    pub device_status: i32,
    pub loop_time: i32,
    pub x_encoder_position: i32,
    pub y_encoder_position: i32,
    pub x_position: f32,
    pub y_position: f32,
    pub heading: f32,
    pub x_velocity: f32,
    pub y_velocity: f32,
    pub h_velocity: f32
}
impl PinpointSnapshot {
    const MM_TO_IN: f32 = 1f32/25.4f32;
    fn new(data: &Vec<u8>) -> PinpointSnapshot {
        let device_status = i32::from_le_bytes(data[0..4].try_into().unwrap());
        let loop_time = i32::from_le_bytes(data[4..8].try_into().unwrap());
        let x_encoder_position = i32::from_le_bytes(data[8..12].try_into().unwrap());
        let y_encoder_position = i32::from_le_bytes(data[12..16].try_into().unwrap());
        let x_position = f32::from_le_bytes(data[16..20].try_into().unwrap()) * Self::MM_TO_IN;
        let y_position = f32::from_le_bytes(data[20..24].try_into().unwrap()) * Self::MM_TO_IN;
        let heading = f32::from_le_bytes(data[24..28].try_into().unwrap());
        let x_velocity = f32::from_le_bytes(data[28..32].try_into().unwrap()) * Self::MM_TO_IN;
        let y_velocity = f32::from_le_bytes(data[32..36].try_into().unwrap()) * Self::MM_TO_IN;
        let h_velocity = f32::from_le_bytes(data[36..40].try_into().unwrap());
        PinpointSnapshot {
            device_status,
            loop_time,
            x_encoder_position,
            y_encoder_position,
            x_position,
            y_position,
            heading,
            x_velocity,
            y_velocity,
            h_velocity,
        }
    }
}
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFromPrimitive)]
enum PinpointRegister {
    DeviceId = 1,
    DeviceVersion = 2,
    DeviceStatus = 3,
    DeviceControl = 4,
    LoopTime = 5,
    XEncoderValue = 6,
    YEncoderValue = 7,
    XPosition = 8,
    YPosition = 9,
    HOrientation = 10,
    XVelocity = 11,
    YVelocity = 12,
    HVelocity = 13,
    TicksPerMm = 14,
    XPodOffset = 15,
    YPodOffset = 16,
    YawScalar = 17,
    BulkRead = 18,
}
///This in ticks per mm
pub enum EncoderResolution {
    SWINGARM_POD,
    FOURBAR,
    CUSTOM(f32)
}
#[derive(Eq, PartialEq)]
pub enum Pod {
    X,
    Y
}
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
enum DeviceControl {
    RecalibrateImu       = 1 << 0,
    ResetPosAndImu       = 1 << 1,
    SetYEncoderReversed  = 1 << 2,
    SetYEncoderForward   = 1 << 3,
    SetXEncoderReversed  = 1 << 4,
    SetXEncoderForward   = 1 << 5,
}
impl ToLeBytes for DeviceControl {
    fn to_le_bytes_vec(self) -> Vec<u8> { (self as u32).to_le_bytes_vec() }
}