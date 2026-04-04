use crate::serialization::commands::{QueryInterfaceData, QueryInterfaceResponseData};
use crate::serialization::packet::Packet;
use num_enum::TryFromPrimitive;
use std::convert::Into;
use crossbeam_channel::{Receiver, RecvError, Sender};
use crate::serialization::command::Command;

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
    StartDownload = 0b0111111100001000,
    DownloadChunk = 0b0111111100001001,
    SetModuleLedColor = 0b0111111100001010,
    GetModuleLedColor = 0b0111111100001011,
    SetModuleLedPattern = 0b0111111100001100,
    GetModuleLedPattern = 0b0111111100001101,
    DebugLogLevel = 0b0111111100001110,
    Discovery = 0b0111111100001111,
}

pub const FIRST: u16 = StandardCommands::ACK as u16;
pub const LAST: u16 = StandardCommands::Discovery as u16;
pub const RESPONSE_BIT: u16 = 0b1000000000000000;
pub fn is_standard_command(pack_id: u16) -> bool {
    pack_id >= FIRST && pack_id <= LAST
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
    fn try_get_packet(rec: &Receiver<Packet>) -> QueryInterfaceResponseData {
        loop {
            let data = match rec.recv() {
                Ok(it) => {it}
                Err(it) => {panic!("could not receive query! {}", it)}
            };
            if let Command::QueryInterfaceResponse(it) = data.payload_data {
                log::info!("got packet that was response data! cmf:{}", it.command_number_first);
                return it;
            } else {log::info!("got packet that wasn't a interface response data! {}", data);}
        }
    }
    pub fn generate_module(id: u8, is_parent: bool, out: &Sender<Packet>, receiver: &Receiver<Packet>) -> Module {
        let req_cmd = Command::QueryInterface(QueryInterfaceData::new_deka());
        let req_packet = Packet::new(req_cmd, id, 0);
        out.send(req_packet).unwrap();
        let pack = Self::try_get_packet(receiver);
        Self::from_deka_discovery(id, &pack, is_parent)
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
