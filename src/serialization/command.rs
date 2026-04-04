use crate::serialization::command::Command::{
    Ack, KeepAlive, LynxCommand, Nack, QueryInterface, QueryInterfaceResponse, Unknown,
};
use crate::serialization::command_data::CommandData;
use crate::serialization::command_utils::*;
use crate::serialization::commands::{
    AckData, KeepAliveData, NackData, QueryInterfaceData, QueryInterfaceResponseData, UnknownData,
};
use crate::serialization::lynx_commands::base_lynx_command::LynxCommandData;
use crate::serialization::lynx_commands::lynx_commands::LynxUnknownData;
use crate::serialization::packet::Packet;
use log::log;
use std::fmt::{Display, Formatter, Pointer, write};
use std::io::Error;

#[derive(Clone, PartialEq)]
pub enum Command {
    //MotorSetPower,
    //Standard Commands
    Ack(AckData),
    Nack(NackData),
    KeepAlive(KeepAliveData),
    QueryInterface(QueryInterfaceData), //DEKA\0
    QueryInterfaceResponse(QueryInterfaceResponseData),
    LynxCommand(LynxCommandData),
    //LynxModule ln 200 has the responses
    Unknown(UnknownData),
}
//NOTE: there are standard commands, and interface commands.

impl Command {
    pub(crate) fn log_pack_id(refid: u8, p1: &Packet, p2: &Packet) {
        log::trace!(
            "logging for packet ref:{}. command:{:016b}, response:{:016b}",
            refid,
            p1.packet_id,
            p2.packet_id
        );
        log::trace!("cmd: {}", p1.payload_data);
        log::trace!("rsp: {}", p2.payload_data);
    }
}

impl Command {
    pub fn to_packet_id(&self) -> u16 {
        match self {
            //MotorSetPower => { 15 }
            Ack(x) => x.to_packet_id(),
            Nack(x) => x.to_packet_id(),
            KeepAlive(x) => x.to_packet_id(),
            QueryInterface(x) => x.to_packet_id(),
            QueryInterfaceResponse(x) => x.to_packet_id(),
            LynxCommand(x) => x.to_packet_id(),
            Unknown(x) => x.to_packet_id(),
        }
    }
    pub fn from_bytes(id: u16, data: &[u8], src_addr: u8, dest_addr: u8) -> Command {
        if let Some(x) = AckData::from_bytes(id, data, dest_addr) {
            return Ack(x);
        }
        if let Some(x) = NackData::from_bytes(id, data, dest_addr) {
            return Nack(x);
        }
        if let Some(x) = KeepAliveData::from_bytes(id, data, dest_addr) {
            return KeepAlive(x);
        }
        if let Some(x) = QueryInterfaceData::from_bytes(id, data, dest_addr) {
            return QueryInterface(x);
        }
        if let Some(x) = QueryInterfaceResponseData::from_bytes(id, data, dest_addr) {
            return QueryInterfaceResponse(x);
        }
        if let Some(x) = LynxCommandData::from_bytes(id, data, src_addr, dest_addr) {
            return LynxCommand(x);
        }
        Unknown(UnknownData::from_bytes(id, data, dest_addr).unwrap())
    }
    pub fn get_bytes_len(&self) -> usize {
        match self {
            //MotorSetPower => { 0 }
            Ack(x) => x.get_bytes_len(),
            Nack(x) => x.get_bytes_len(),
            KeepAlive(x) => x.get_bytes_len(),
            QueryInterface(x) => x.get_bytes_len(),
            QueryInterfaceResponse(x) => x.get_bytes_len(),
            LynxCommand(x) => x.get_bytes_len(),
            Unknown(x) => x.get_bytes_len(), //do these later
        }
    }
    pub const fn command_number(pack_id: u16) -> u16 {
        let x = RESPONSE_BIT;
        pack_id & !x //clear response bit
    }
    pub const fn is_response(pack_id: u16) -> bool {
        let x = RESPONSE_BIT;
        (pack_id & x) > 0
        //is first bit set? if yes, this is a response packet. probably.
    }
}
impl Display for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Ack(x) => x.fmt(f),
            Nack(x) => x.fmt(f),
            KeepAlive(x) => x.fmt(f),
            QueryInterface(x) => x.fmt(f),
            QueryInterfaceResponse(x) => x.fmt(f),
            LynxCommand(x) => x.fmt(f),
            Unknown(x) => x.fmt(f),
        }
    }
}
pub fn null_term_to_string(x: &Vec<u8>) -> String {
    String::from_utf8(x[..x.iter().position(|&b| b == 0).unwrap_or(x.len())].to_vec()).unwrap()
}
impl Into<Vec<u8>> for Command {
    fn into(self) -> Vec<u8> {
        if self.get_bytes_len() == 0 {
            return vec![];
        }
        match self {
            Ack(x) => x.into(),
            Nack(x) => x.into(),
            KeepAlive(x) => x.into(),
            QueryInterface(x) => x.into(),
            QueryInterfaceResponse(x) => x.into(),
            LynxCommand(x) => x.into(),
            Unknown(x) => x.into(),
        }
    }
}
