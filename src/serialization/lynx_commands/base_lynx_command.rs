use crate::serialization::command_data::CommandData;
use crate::serialization::command_utils::Module;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand::*;
use crate::serialization::lynx_commands::lynx_commands::*;
use std::fmt::{Display, Formatter};
use std::sync::Mutex;
use crate::serialization::command::Command;

#[derive(Clone, Debug, PartialEq)]
pub enum LynxCommand {
    LynxGetADCCommand(LynxGetADCCommandData),
    LynxGetADCResponse(LynxGetADCResponseData),
    LynxGetBulkDataCommand(LynxGetBulkDataCommandData),
    LynxGetBulkDataResponse(LynxGetBulkDataResponseData),
    LynxSetMotorPowerCommand(LynxSetMotorPowerCommandData),
    LynxSetServoPositionCommand(LynxSetServoPositionCommandData),
    LynxUnknownCommand(LynxUnknownData),
}
#[derive(Clone, Debug, PartialEq)]
pub struct LynxCommandData {
    pub module: Module,
    pub command: LynxCommand,
}
pub(crate) static MODULES: Mutex<Vec<Module>> = Mutex::new(vec![]);
impl Display for LynxCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LynxCommand[module:{},command:[",
            self.module.module_addr
        )?;
        match self.command.clone() {
            LynxGetADCResponse(x) => x.fmt(f),
            LynxGetADCCommand(x) => x.fmt(f),
            LynxGetBulkDataCommand(x) => x.fmt(f),
            LynxGetBulkDataResponse(x) => x.fmt(f),
            LynxSetMotorPowerCommand(x) => x.fmt(f),
            LynxSetServoPositionCommand(x) => x.fmt(f),
            LynxUnknownCommand(x) => x.fmt(f),
        }?;
        write!(f, "]]")
    }
}

impl Into<Vec<u8>> for LynxCommandData {
    fn into(self) -> Vec<u8> {
        match self.command {
            LynxGetADCCommand(it) => it.into(),
            LynxGetADCResponse(it) => it.into(),
            LynxGetBulkDataCommand(it) => it.into(),
            LynxGetBulkDataResponse(it) => it.into(),
            LynxSetMotorPowerCommand(it) => it.into(),
            LynxSetServoPositionCommand(it) => it.into(),
            LynxUnknownCommand(it) => it.into(),
        }
    }
}

impl CommandData for LynxCommandData {
    fn to_packet_id(&self) -> u16 {
        (match self.command.clone() {
            LynxGetADCCommand(x) => x.to_packet_id(),
            LynxGetADCResponse(x) => x.to_packet_id(),
            LynxGetBulkDataCommand(x) => x.to_packet_id(),
            LynxGetBulkDataResponse(x) => x.to_packet_id(),
            LynxSetMotorPowerCommand(x) => x.to_packet_id(),
            LynxSetServoPositionCommand(x) => x.to_packet_id(),
            LynxUnknownCommand(x) => x.to_packet_id(),
        }) + self.module.number_command_first
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        todo!("DO NOT CALL THIS IT NEEDS TO KNOW ABOUT SRC AND DEST ADDR");
    }

    fn get_bytes_len(&self) -> usize {
        match &self.command {
            LynxGetADCCommand(it) => it.get_bytes_len(),
            LynxGetADCResponse(it) => it.get_bytes_len(),
            LynxGetBulkDataCommand(it) => it.get_bytes_len(),
            LynxGetBulkDataResponse(it) => it.get_bytes_len(),
            LynxSetMotorPowerCommand(it) => it.get_bytes_len(),
            LynxSetServoPositionCommand(it) => it.get_bytes_len(),
            LynxUnknownCommand(it) => it.get_bytes_len(),
        }
    }
}
impl LynxCommandData {
    pub fn from_bytes(id: u16, data: &[u8], src_addr: u8, dest_addr: u8) -> Option<Self> {
        let lock = MODULES.lock().unwrap();
        if let Some(x) = lock.iter().find(|x| x.module_addr == dest_addr) {
            if let Some(m) = x.is_module_command(id) {
                Some(LynxCommandData {
                    module: x.clone(),
                    command: Self::to_command(m, data, dest_addr),
                })
            } else {
                None
            }
        } else if let Some(x) = lock.iter().find(|x| x.module_addr == src_addr) {
            if let Some(m) = x.is_module_response(id) {
                Some(LynxCommandData {
                    module: x.clone(),
                    command: Self::to_command(m, data, src_addr),
                })
            } else {
                None
            }
        } else {
            None
        }
    }
    fn to_command(id: u16, data: &[u8], dest_addr: u8) -> LynxCommand {
        //this id has been stripped of the like, Module offset bit thing
        if let Some(cmd) = LynxGetADCCommandData::from_bytes(id, data, dest_addr) {
            LynxGetADCCommand(cmd)
        } else if let Some(cmd) = LynxGetADCResponseData::from_bytes(id, data, dest_addr) {
            LynxGetADCResponse(cmd)
        } else if let Some(cmd) = LynxGetBulkDataCommandData::from_bytes(id, data, dest_addr) {
            LynxGetBulkDataCommand(cmd)
        } else if let Some(cmd) = LynxGetBulkDataResponseData::from_bytes(id, data, dest_addr) {
            LynxGetBulkDataResponse(cmd)
        } else if let Some(cmd) = LynxSetMotorPowerCommandData::from_bytes(id, data, dest_addr) {
            LynxSetMotorPowerCommand(cmd)
        } else if let Some(cmd) = LynxSetServoPositionCommandData::from_bytes(id, data, dest_addr) {
            LynxSetServoPositionCommand(cmd)
        } else {
            LynxUnknownCommand(LynxUnknownData::from_bytes(id, data, dest_addr).unwrap())
        }
    }
    fn new(module: &Module, lynx_command: LynxCommand) -> Command {
        let data = LynxCommandData { module: module.clone(), command: lynx_command };
        Command::LynxCommand(data)
    }
}

impl LynxCommand {
    pub fn to_command(self, module: &Module) -> Command {
        LynxCommandData::new(module, self)
    }
}