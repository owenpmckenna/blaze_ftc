use crate::serialization::command::null_term_to_string;
use crate::serialization::command_data::CommandData;
use crate::serialization::command_utils::{RESPONSE_BIT, StandardCommands};
use nix::NixPath;
use std::fmt::{Display, Formatter};
use num_enum::{TryFromPrimitive, TryFromPrimitiveError};

#[derive(Clone, Debug, PartialEq)]
pub struct UnknownData {
    pack_id: u16,
    data: Vec<u8>,
}
impl Display for UnknownData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown[len:{},data:{:?}]", self.data.len(), self.data)
    }
}
impl Into<Vec<u8>> for UnknownData {
    fn into(self) -> Vec<u8> {
        self.data
    }
}
impl CommandData for UnknownData {
    fn to_packet_id(&self) -> u16 {
        self.pack_id
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        Some(Self {
            pack_id: id,
            data: data.to_vec(),
        })
    }
    fn get_bytes_len(&self) -> usize {
        self.data.len()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AckData {
    pub attention_required: bool, //did *not* know this was here at first
}
impl Display for AckData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ack[attention required:{}]", self.attention_required)
    }
}
impl Into<Vec<u8>> for AckData {
    fn into(self) -> Vec<u8> {
        vec![if self.attention_required { 1 } else { 0 }]
    }
}
impl CommandData for AckData {
    fn to_packet_id(&self) -> u16 {
        StandardCommands::ACK as u16
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if Self::command_number(id) == StandardCommands::ACK as u16 {
            Some(Self {
                attention_required: data[0] != 0,
            })
        } else {
            None
        }
    }
    fn get_bytes_len(&self) -> usize {
        1
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, TryFromPrimitive)]
pub enum StandardReasonCode {
    Param0 = 0,
    Param1 = 1,
    Param2 = 2,
    Param3 = 3,
    Param4 = 4,
    Param5 = 5,
    Param6 = 6,
    Param7 = 7,
    Param8 = 8,
    Param9 = 9,

    GpioOut0 = 10,
    GpioOut1 = 11,
    GpioOut2 = 12,
    GpioOut3 = 13,
    GpioOut4 = 14,
    GpioOut5 = 15,
    GpioOut6 = 16,
    GpioOut7 = 17,
    GpioNoOutput = 18,

    GpioIn0 = 20,
    GpioIn1 = 21,
    GpioIn2 = 22,
    GpioIn3 = 23,
    GpioIn4 = 24,
    GpioIn5 = 25,
    GpioIn6 = 26,
    GpioIn7 = 27,
    GpioNoInput = 28,

    ServoNotConfigBeforeEnabled = 30,
    BatteryTooLowToRunServo = 31,

    I2cMasterBusy = 40,
    I2cOperationInProgress = 41,
    I2cNoResultsPending = 42,
    I2cQueryMismatch = 43,
    I2cTimeoutSdaStuck = 44,
    I2cTimeoutSckStuck = 45,
    I2cTimeoutUnknownCause = 46,

    MotorNotConfigBeforeEnabled = 50,
    CommandInvalidForMotorMode = 51,
    BatteryTooLowToRunMotor = 52,

    CommandImplPending = 253,
    CommandRoutingError = 254,
    PacketTypeIdUnknown = 255,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NackData {
    pub(crate) reason_code: u8
}
impl NackData {
    pub fn reason(&self) -> StandardReasonCode {
        StandardReasonCode::try_from(self.reason_code).expect("could not get reason code")
    }
}
impl Display for NackData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let reason = match StandardReasonCode::try_from(self.reason_code) {
            Ok(it) => {it}
            Err(_) => {panic!("could not parse NACK reason code {}", self.reason_code)}
        };
        write!(f, "Nack[code:{},reason:{:?}]", self.reason_code, reason)
    }
}
impl Into<Vec<u8>> for NackData {
    fn into(self) -> Vec<u8> {
        vec![self.reason_code]
    }
}
impl CommandData for NackData {
    fn to_packet_id(&self) -> u16 {
        StandardCommands::NACK as u16
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if Self::command_number(id) == StandardCommands::NACK as u16 {
            Some(Self {
               reason_code: data[0]
            })
        } else {
            None
        }
    }
    fn get_bytes_len(&self) -> usize {
        1
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeepAliveData {}
impl Display for KeepAliveData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Keepalive")
    }
}
impl Into<Vec<u8>> for KeepAliveData {
    fn into(self) -> Vec<u8> {
        vec![]
    }
}
impl CommandData for KeepAliveData {
    fn to_packet_id(&self) -> u16 {
        StandardCommands::KeepAlive as u16
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if Self::command_number(id) == StandardCommands::KeepAlive as u16 {
            Some(Self {})
        } else {
            None
        }
    }
    fn get_bytes_len(&self) -> usize {
        0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryInterfaceData {
    name: Vec<u8>, //this is null terminated. It's the name of the device. Usually, "DEKA\0"
}
impl Display for QueryInterfaceData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "QueryInterface: [{}]", null_term_to_string(&self.name))
    }
}
impl Into<Vec<u8>> for QueryInterfaceData {
    fn into(self) -> Vec<u8> {
        self.name
    }
}
impl CommandData for QueryInterfaceData {
    fn to_packet_id(&self) -> u16 {
        StandardCommands::QueryInterface as u16
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == StandardCommands::QueryInterface as u16 {
            Some(Self {
                name: data.to_vec(),
            })
        } else {
            None
        }
    }
    fn get_bytes_len(&self) -> usize {
        self.name.len()
    }
}
impl QueryInterfaceData {
    pub fn new_deka() -> QueryInterfaceData {
        let mut name = "DEKA".as_bytes().to_vec();
        name.extend_from_slice(&[0]);
        QueryInterfaceData { name }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryInterfaceResponseData {
    pub command_number_first: u16,
    pub number_of_commands: u16,
}
impl Display for QueryInterfaceResponseData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "QueryInterfaceResp: [first cmd num:{:016b}, num cmds: {}]",
            self.command_number_first, self.number_of_commands
        )
    }
}
impl Into<Vec<u8>> for QueryInterfaceResponseData {
    fn into(self) -> Vec<u8> {
        let mut v = vec![];
        v.extend_from_slice(&self.command_number_first.to_le_bytes());
        v.extend_from_slice(&self.number_of_commands.to_le_bytes());
        v
    }
}
impl CommandData for QueryInterfaceResponseData {
    fn to_packet_id(&self) -> u16 {
        StandardCommands::QueryInterface as u16 | RESPONSE_BIT
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == (StandardCommands::QueryInterface as u16 | RESPONSE_BIT) {
            Some(Self {
                command_number_first: u16::from_le_bytes(data[0..2].try_into().unwrap()),
                number_of_commands: u16::from_le_bytes(data[2..4].try_into().unwrap()),
            })
        } else {
            None
        }
    }
    fn get_bytes_len(&self) -> usize {
        4
    }
}
