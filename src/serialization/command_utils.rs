use crate::serialization::commands::QueryInterfaceResponseData;
use crate::serialization::packet::Packet;
use num_enum::TryFromPrimitive;
use std::convert::Into;

#[repr(u16)]
#[derive(TryFromPrimitive, Copy, Clone, Debug, PartialEq, Eq)]
pub enum StandardCommands {
    ACK = 0b0111111100000001,
    NACK = 0b0111111100000010,
    GetModuleStatus = 0b0111111100000011,
    KeepAlive = 0b0111111100000100,
    FailSafe = 0b0111111100000101,
    SetNewModuleAddr = 0b0111111100000110,
    QueryInterface = 0b0111111100000111,
    Discovery = 0b0111111100001111,
}

pub const FIRST: u16 = StandardCommands::ACK as u16;
pub const LAST: u16 = StandardCommands::Discovery as u16;
pub const RESPONSE_BIT: u16 = 0b1000000000000000;
pub fn is_standard_command(pack_id: u16) -> bool {
    pack_id > FIRST && pack_id < LAST
}

#[derive(Clone, PartialEq, Debug)]
pub struct Module {
    pub module_addr: u8,
    pub module_name: String, //used for printing only
    pub number_command_first: u16,
    pub number_of_commands: u16,
    pub is_parent: bool, //not sure exactly what this means
    pub module_type: ModuleType,
}
impl Module {
    pub fn null() -> Module {
        Module {
            module_addr: 0,
            module_name: "HOST".to_string(),
            number_command_first: 0,
            number_of_commands: 0, //don't accept anything
            is_parent: false,
            module_type: ModuleType::Host,
        }
    }
    pub fn from_deka_discovery(
        id: u8,
        data: &QueryInterfaceResponseData,
        is_parent: bool,
    ) -> Module {
        Module {
            module_addr: id,
            module_name: "LYNX/DEKA".to_string(),
            number_command_first: data.command_number_first,
            number_of_commands: data.number_of_commands,
            is_parent,
            module_type: ModuleType::Lynx,
        }
    }
    pub const fn is_module(&self, packet: &Packet) -> bool {
        packet.dest_module_addr == self.module_addr
    }
    pub const fn is_module_response(&self, packet_id: u16) -> Option<u16> {
        if self.is_module_command(packet_id).is_some() {
            return None; //One or the other
        }
        let cmd_id = command_number(packet_id);
        if cmd_id > self.number_command_first + self.number_of_commands
            || cmd_id < self.number_command_first
        {
            None
        } else {
            Some(packet_id - self.number_command_first)
        }
    }
    pub const fn is_module_command(&self, packet_id: u16) -> Option<u16> {
        if packet_id > self.number_command_first + self.number_of_commands
            || packet_id < self.number_command_first
        {
            None
        } else {
            Some(packet_id - self.number_command_first)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModuleType {
    Lynx,
    Host,
}
const fn command_number(pack_id: u16) -> u16 {
    let x = 0b1000000000000000u16;
    pack_id & !x //clear response bit
}
