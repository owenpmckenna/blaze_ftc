use crate::serialization::command::Command;
use crate::serialization::command_data::CommandData;
use crate::serialization::command_utils::Module;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand::*;
use crate::serialization::lynx_commands::lynx_commands::*;
use crate::{HUB_0, HUB_1};
use std::fmt::{Debug, Display, Formatter, Pointer};
use std::sync::OnceLock;

#[derive(Clone, PartialEq)]
pub enum LynxCommand {
    LynxGetADCCommand(LynxGetADCCommandData),
    LynxGetADCResponse(LynxGetADCResponseData),
    LynxGetBulkDataCommand(LynxGetBulkDataCommandData),
    LynxGetBulkDataResponse(LynxGetBulkDataResponseData),
    LynxSetMotorPowerCommand(LynxSetMotorPowerCommandData),
    LynxSetServoPulseWidthCommand(LynxSetServoPulseWidthCommandData),
    LynxSetMotorChannelEnableCommand(LynxSetMotorChannelEnableCommandData),
    LynxGetMotorChannelModeCommand(LynxGetMotorChannelModeCommandData),
    LynxGetMotorChannelModeResponse(LynxGetMotorChannelModeResponseData),
    LynxSetMotorChannelModeCommand(LynxSetMotorChannelModeCommandData),
    LynxI2CSingleByteWriteCommand(LynxI2CSingleByteWriteCommandData),
    LynxI2cWriteMultipleBytesCommand(LynxI2cWriteMultipleBytesCommandData),
    LynxI2CWriteReadMultipleBytesCommand(LynxI2CWriteReadMultipleBytesCommandData),
    LynxI2CReadStatusQueryCommand(LynxI2CReadStatusQueryCommandData),
    LynxI2CReadStatusQueryResponse(LynxI2CReadStatusQueryResponseData),
    LynxSetMotorPIDFCommand(LynxSetMotorPIDFCommandData),
    LynxSetMotorVelocityTargetCommand(LynxSetMotorVelocityTargetCommandData),
    LynxSetMotorTargetPositionCommand(LynxSetMotorTargetPositionCommandData),
    LynxI2CWriteStatusQueryCommand(LynxI2CWriteStatusQueryCommandData),
    LynxI2CWriteStatusQueryResponse(LynxI2CWriteStatusQueryResponseData),
    LynxI2CConfigureChannelCommand(LynxI2CConfigureChannelCommandData),
    LynxUnknownCommand(LynxUnknownData),
}
#[derive(Clone, PartialEq)]
pub struct LynxCommandData {
    pub module: &'static Module,
    pub command: LynxCommand,
}

macro_rules! with_command_data {
    ($cmd:expr, |$x:ident| $body:expr) => {
        match $cmd {
            LynxCommand::LynxGetADCCommand($x) => $body,
            LynxCommand::LynxGetADCResponse($x) => $body,
            LynxCommand::LynxGetBulkDataCommand($x) => $body,
            LynxCommand::LynxGetBulkDataResponse($x) => $body,
            LynxCommand::LynxSetMotorPowerCommand($x) => $body,
            LynxCommand::LynxSetServoPulseWidthCommand($x) => $body,
            LynxCommand::LynxSetMotorChannelEnableCommand($x) => $body,
            LynxCommand::LynxGetMotorChannelModeCommand($x) => $body,
            LynxCommand::LynxGetMotorChannelModeResponse($x) => $body,
            LynxCommand::LynxSetMotorChannelModeCommand($x) => $body,
            LynxCommand::LynxI2CSingleByteWriteCommand($x) => $body,
            LynxCommand::LynxI2cWriteMultipleBytesCommand($x) => $body,
            LynxCommand::LynxI2CWriteReadMultipleBytesCommand($x) => $body,
            LynxCommand::LynxI2CReadStatusQueryCommand($x) => $body,
            LynxCommand::LynxI2CReadStatusQueryResponse($x) => $body,
            LynxCommand::LynxSetMotorPIDFCommand($x) => $body,
            LynxCommand::LynxSetMotorVelocityTargetCommand($x) => $body,
            LynxCommand::LynxSetMotorTargetPositionCommand($x) => $body,
            LynxCommand::LynxI2CWriteStatusQueryCommand($x) => $body,
            LynxCommand::LynxI2CWriteStatusQueryResponse($x) => $body,
            LynxCommand::LynxI2CConfigureChannelCommand($x) => $body,
            LynxCommand::LynxUnknownCommand($x) => $body,
        }
    };
}
impl Display for LynxCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LynxCommand[module:{},command:[",
            self.module.module_addr
        )?;
        with_command_data!(&self.command, |x| x.fmt(f))?;
        write!(f, "]]")
    }
}
/*impl Display for LynxCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LynxCommand[module:{},command:[",
            self.module.module_addr
        )?;
        with_command_data!(&self.command, |x| x.fmt(f))?;
        write!(f, "]]")
    }
}*/

impl Into<Vec<u8>> for LynxCommandData {
    fn into(self) -> Vec<u8> {
        with_command_data!(self.command, |x| x.into())
    }
}

impl CommandData for LynxCommandData {
    fn to_packet_id(&self) -> u16 {
        with_command_data!(self.command.clone(), |x| x.to_packet_id()) + self.module.number_command_first
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        unimplemented!("DO NOT CALL THIS IT NEEDS TO KNOW ABOUT SRC AND DEST ADDR");
    }

    fn get_bytes_len(&self) -> usize {
        with_command_data!(&self.command, |x| x.get_bytes_len())
    }
}
static NULL_MODULE: OnceLock<Module> = OnceLock::new();
impl LynxCommandData {
    pub fn from_bytes(id: u16, data: &[u8], src_addr: u8, dest_addr: u8) -> Option<Self> {
        let lock = if let Some(hub_1) = HUB_1.get() {
            vec![&HUB_0.get().expect("NO HUB 0").module, &hub_1.module]
        } else if let Some(hub_0) = HUB_0.get() {
            vec![&hub_0.module]
        } else {
            return Some(LynxCommandData {
                module: NULL_MODULE.get_or_init(|| Module::null()),//Will this break something???
                command: LynxUnknownCommand(LynxUnknownData::from_bytes(id, data, dest_addr).unwrap()),
            });
        };
        if let Some(x) = lock.iter().find(|x| x.module_addr == dest_addr) {
            if let Some(m) = x.is_module_command(id) {
                Some(LynxCommandData {
                    module: x,
                    command: Self::to_command(m, data, dest_addr),
                })
            } else {
                None
            }
        } else if let Some(x) = lock.iter().find(|x| x.module_addr == src_addr) {
            if let Some(m) = x.is_module_response(id) {
                Some(LynxCommandData {
                    module: x,
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
        } else if let Some(cmd) = LynxSetServoPulseWidthCommandData::from_bytes(id, data, dest_addr) {
            LynxSetServoPulseWidthCommand(cmd)
        } else if let Some(cmd) = LynxSetMotorChannelEnableCommandData::from_bytes(id, data, dest_addr) {
            LynxSetMotorChannelEnableCommand(cmd)
        } else if let Some(cmd) = LynxGetMotorChannelModeCommandData::from_bytes(id, data, dest_addr) {
            LynxGetMotorChannelModeCommand(cmd)
        } else if let Some(cmd) = LynxGetMotorChannelModeResponseData::from_bytes(id, data, dest_addr) {
            LynxGetMotorChannelModeResponse(cmd)
        } else if let Some(cmd) = LynxSetMotorChannelModeCommandData::from_bytes(id, data, dest_addr) {
            LynxSetMotorChannelModeCommand(cmd)
        } else if let Some(cmd) = LynxI2CSingleByteWriteCommandData::from_bytes(id, data, dest_addr) {
            LynxI2CSingleByteWriteCommand(cmd)
        } else if let Some(cmd) = LynxI2cWriteMultipleBytesCommandData::from_bytes(id, data, dest_addr) {
            LynxI2cWriteMultipleBytesCommand(cmd)
        } else if let Some(cmd) = LynxI2CWriteReadMultipleBytesCommandData::from_bytes(id, data, dest_addr) {
            LynxI2CWriteReadMultipleBytesCommand(cmd)
        } else if let Some(cmd) = LynxI2CReadStatusQueryCommandData::from_bytes(id, data, dest_addr) {
            LynxI2CReadStatusQueryCommand(cmd)
        } else if let Some(cmd) = LynxI2CReadStatusQueryResponseData::from_bytes(id, data, dest_addr) {
            LynxI2CReadStatusQueryResponse(cmd)
        } else if let Some(cmd) = LynxSetMotorPIDFCommandData::from_bytes(id, data, dest_addr) {
            LynxSetMotorPIDFCommand(cmd)
        } else if let Some(cmd) = LynxSetMotorVelocityTargetCommandData::from_bytes(id, data, dest_addr) {
            LynxSetMotorVelocityTargetCommand(cmd)
        } else if let Some(cmd) = LynxSetMotorTargetPositionCommandData::from_bytes(id, data, dest_addr) {
            LynxSetMotorTargetPositionCommand(cmd)
        } else if let Some(cmd) = LynxI2CWriteStatusQueryCommandData::from_bytes(id, data, dest_addr) {
            LynxI2CWriteStatusQueryCommand(cmd)
        } else if let Some(cmd) = LynxI2CWriteStatusQueryResponseData::from_bytes(id, data, dest_addr) {
            LynxI2CWriteStatusQueryResponse(cmd)
        } else if let Some(cmd) = LynxI2CConfigureChannelCommandData::from_bytes(id, data, dest_addr) {
            LynxI2CConfigureChannelCommand(cmd)
        } else {
            LynxUnknownCommand(LynxUnknownData::from_bytes(id, data, dest_addr).unwrap())
        }
    }
    fn new(module: &'static Module, lynx_command: LynxCommand) -> Command {
        let data = LynxCommandData { module, command: lynx_command };
        Command::LynxCommand(data)
    }
}

impl LynxCommand {
    pub fn to_command(self, module: &'static Module) -> Command {
        LynxCommandData::new(module, self)
    }
}