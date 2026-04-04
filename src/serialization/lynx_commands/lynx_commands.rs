use crate::serialization::command_data::CommandData;
use crate::serialization::command_utils::{RESPONSE_BIT};
use num_enum::TryFromPrimitive;
use std::fmt::{Display, Formatter};
use std::io::empty;
use num_traits::real::Real;
use num_traits::ToPrimitive;
use crate::control::MotorPIDF::PIDF;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommandData;

#[derive(Clone, PartialEq)]
pub struct LynxUnknownData {
    pack_id: u16,
    data: Vec<u8>,
}
impl Display for LynxUnknownData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "UnknownCommand[pack_id:{:016b},data:{:?}]",
            self.pack_id, self.data
        )
    }
}
impl Into<Vec<u8>> for LynxUnknownData {
    fn into(self) -> Vec<u8> {
        self.data
    }
}
impl CommandData for LynxUnknownData {
    fn to_packet_id(&self) -> u16 {
        self.pack_id
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        Some(Self {
            pack_id: id,
            data: data.to_vec(),
        })
    }

    fn get_bytes_len(&self) -> usize {
        self.data.len()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct LynxGetADCCommandData {
    channel: ADCCommandChannel,
    mode: ADCCommandMode,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, TryFromPrimitive, Debug)]
pub enum ADCCommandMode {
    ENGINEERING = 0,
    RAW = 1,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, TryFromPrimitive, Debug)]
pub enum ADCCommandChannel {
    USER0 = 0,
    USER1 = 1,
    USER2 = 2,
    USER3 = 3,
    GPIO_CURRENT = 4,
    I2C_BUS_CURRENT = 5,
    SERVO_CURRENT = 6,
    BATTERY_CURRENT = 7,
    MOTOR0_CURRENT = 8,
    MOTOR1_CURRENT = 9,
    MOTOR2_CURRENT = 10,
    MOTOR3_CURRENT = 11,
    FIVE_VOLT_MONITOR = 12,
    BATTERY_MONITOR = 13,
    CONTROLLER_TEMPERATURE = 14,
}
impl Display for LynxGetADCCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "GetADCCommand[{:?}, {:?}]", self.channel, self.mode)
    }
}

impl Into<Vec<u8>> for LynxGetADCCommandData {
    fn into(self) -> Vec<u8> {
        //we're already in little endian right?
        vec![(self.channel as u8).to_le(), (self.mode as u8).to_le()]
    }
}
impl CommandData for LynxGetADCCommandData {
    fn to_packet_id(&self) -> u16 {
        7
    }

    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 7 {
            Some(Self {
                channel: ADCCommandChannel::try_from(data[0]).unwrap(),
                mode: ADCCommandMode::try_from(data[1]).unwrap(),
            })
        } else {
            None
        }
    }

    fn get_bytes_len(&self) -> usize {
        2
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct LynxGetADCResponseData {
    pub value: i16,
}
impl LynxGetADCResponseData {
    fn value_to_volts(&self) -> f32 {
        //TODO figure this out
        0.0
    }
}
impl Display for LynxGetADCResponseData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "GetADCResponse[value:{}]", self.value)
    }
}
impl Into<Vec<u8>> for LynxGetADCResponseData {
    fn into(self) -> Vec<u8> {
        self.value.to_le_bytes().to_vec()
    }
}
impl CommandData for LynxGetADCResponseData {
    fn to_packet_id(&self) -> u16 {
        7 | RESPONSE_BIT
    }
    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == (7 | RESPONSE_BIT) {
            Some(LynxGetADCResponseData {
                value: i16::from_le_bytes(data[0..2].try_into().unwrap()),
            })
        } else {
            log::trace!(
                "not an adc response! id:{:016b} ours:{:016b}",
                id,
                7 | RESPONSE_BIT
            );
            None
        }
    }

    fn get_bytes_len(&self) -> usize {
        2
    }
}


#[derive(Clone, Copy, PartialEq)]
pub struct LynxGetBulkDataCommandData {}
impl Display for LynxGetBulkDataCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxGetBulkDataCommand")
    }
}
impl Into<Vec<u8>> for LynxGetBulkDataCommandData {
    fn into(self) -> Vec<u8> {
        vec![]
    }
}
impl CommandData for LynxGetBulkDataCommandData {
    fn to_packet_id(&self) -> u16 {
        0
    }
    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == 0 {
            Some(LynxGetBulkDataCommandData {})
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        0
    }
}

/*
Ok: this is direct from ftcsdk
        uint8_t     digitalInputs;
        int32_t     motor0position_enc;
        int32_t     motor1position_enc;
        int32_t     motor2position_enc;
        int32_t     motor3position_enc;
        uint8_t     motorStatus;
        int16_t     motor0velocity_cps;  // counts per second
        int16_t     motor1velocity_cps;
        int16_t     motor2velocity_cps;
        int16_t     motor3velocity_cps;
        int16_t     analog0_mV;
        int16_t     analog1_mV;
        int16_t     analog2_mV;
        int16_t     analog3_mV;
*/
#[derive(Clone, Copy, PartialEq)]
pub struct LynxGetBulkDataResponseData {
    pub digital_inputs: u8,
    pub motor_status: u8,
    pub motors: [MotorData; 4],
    pub analog: [i16; 4]
}
#[derive(Copy, Clone, PartialEq)]
pub struct MotorData {
    pub position: i32,
    /**
     * Counts per second
     */
    pub velocity: i16
}
impl MotorData {
    fn new(position: i32, velocity: i16) -> MotorData { MotorData { position, velocity } }
    fn from_vec(positions: Vec<i32>, velocity: Vec<i16>) -> Vec<MotorData> {
        positions.into_iter().enumerate()
            .map(|(i, p)| MotorData::new(p, velocity[i]))
            .collect()
    }
}
impl Display for LynxGetBulkDataResponseData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let m = self.motors;
        write!(f, "LynxGetBulkDataResponse[m0p:{}m0v:{},m1p:{},m2p{}, m3p:{}]", m[0].position, m[0].velocity, m[1].position, m[2].position, m[3].position)
    }
}
impl LynxGetBulkDataResponseData {
    pub(crate) fn default() -> Self {
        Self {
            digital_inputs: 0,
            motor_status: 0,
            motors: [MotorData::new(0, 0); 4],
            analog: [0; 4],
        }
    }
    fn extend_into_bytes<D>(data: &[D], bytes: &mut Vec<u8>, func: fn(&D) -> Vec<u8>) where D: Sized {
        data.iter().map(func).for_each(|x| bytes.extend(x))
    }
    /*fn read<T: Pod>(data: &[u8], offset: usize, len: usize) -> Vec<T> {
        let start = offset;
        let end = start + len * size_of::<T>();
        cast_slice(&data[start..end]).to_vec()
    }*/
    fn read_i32s(data: &[u8], offset: usize, len: usize) -> Vec<i32> {
        let mut out = vec![0i32; len];
        for x in 0..len {
            let start = (x) * size_of::<i32>();
            let end = (x+1) * size_of::<i32>();
            out[x] = i32::from_le_bytes(data[offset + start..offset + end].try_into().unwrap());
        }
        out
    }
    fn read_i16s(data: &[u8], offset: usize, len: usize) -> Vec<i16> {
        let mut out = vec![0i16; len];
        for x in 0..len {
            let start = x * size_of::<i16>();
            let end = start + size_of::<i16>();
            out[x] = i16::from_le_bytes(data[offset + start..offset + end].try_into().unwrap());
        }
        out
    }

}
impl Into<Vec<u8>> for LynxGetBulkDataResponseData {
    fn into(self) -> Vec<u8> {
        //write digital inputs, motor positions, motor statuses, motor velocities, analog inputs
        let mut v = self.digital_inputs.to_le_bytes().to_vec();
        Self::extend_into_bytes(&self.motors, &mut v, |x| x.position.to_le_bytes().to_vec());
        v.extend_from_slice(&[self.motor_status]);
        Self::extend_into_bytes(&self.motors, &mut v, |x| x.velocity.to_le_bytes().to_vec());
        Self::extend_into_bytes(&self.analog, &mut v, |x| x.to_le_bytes().to_vec());
        v
    }
}
impl CommandData for LynxGetBulkDataResponseData {
    fn to_packet_id(&self) -> u16 {
        0 | RESPONSE_BIT
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == (0 | RESPONSE_BIT) {
            let digital_inputs = data[0];//read 1 byte
            let positions = Self::read_i32s(data, 1, 4);//read 16 bytes
            let motor_status = data[17];//read 1
            let velocities = Self::read_i16s(data, 18, 4);//read 8 bytes
            let analogs = Self::read_i16s(data, 26, 4);//read 8 bytes
            Some(
                LynxGetBulkDataResponseData {
                    digital_inputs,
                    motor_status,
                    motors: MotorData::from_vec(positions, velocities).as_slice().try_into().unwrap(),
                    analog: analogs.as_slice().try_into().unwrap()
                }
            )
        } else {None}
    }

    fn get_bytes_len(&self) -> usize {
        34
    }
}


#[derive(Clone, Copy, PartialEq)]
pub struct LynxSetMotorPowerCommandData {
    pub motor: u8,
    pub power: i16
}
impl Display for LynxSetMotorPowerCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxSetMotorPower[motor:{},power:{}]", self.motor, self.power)
    }
}
impl Into<Vec<u8>> for LynxSetMotorPowerCommandData {
    fn into(self) -> Vec<u8> {
        let mut v = vec![self.motor; 1];
        v.extend_from_slice(&self.power.to_le_bytes());
        v
    }
}
impl CommandData for LynxSetMotorPowerCommandData {
    fn to_packet_id(&self) -> u16 {
        15
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 15 {
            Some(LynxSetMotorPowerCommandData { motor: data[0], power: i16::from_le_bytes(data[1..3].try_into().unwrap()) })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        3
    }
}


#[derive(Clone, Copy, PartialEq)]
pub struct LynxSetServoPulseWidthCommandData {
    pub servo: u8,
    pub pulse_width: u16
}
impl Display for LynxSetServoPulseWidthCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxSetServoPulseWidth[servo:{},pulse_width:{}]", self.servo, self.pulse_width)
    }
}
impl Into<Vec<u8>> for LynxSetServoPulseWidthCommandData {
    fn into(self) -> Vec<u8> {
        let mut v = vec![self.servo; 1];
        v.extend_from_slice(&self.pulse_width.to_le_bytes());
        v
    }
}
impl CommandData for LynxSetServoPulseWidthCommandData {
    fn to_packet_id(&self) -> u16 {
        33
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 33 {
            Some(LynxSetServoPulseWidthCommandData { servo: data[0], pulse_width: u16::from_le_bytes(data[1..3].try_into().unwrap()) })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        3
    }
}


#[derive(Clone, Copy, PartialEq)]
pub struct LynxSetMotorChannelEnableCommandData {
    pub motor: u8,
    pub enabled: bool
}
impl Display for LynxSetMotorChannelEnableCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxSetMotorChannelEnable[motor:{},enabled:{}]", self.motor, self.enabled)
    }
}
impl Into<Vec<u8>> for LynxSetMotorChannelEnableCommandData {
    fn into(self) -> Vec<u8> {
        let mut v = vec![self.motor; 1];
        let enabled: u8 = match self.enabled { true => {1} false => {0} };
        v.extend_from_slice(&enabled.to_le_bytes());
        v
    }
}
impl CommandData for LynxSetMotorChannelEnableCommandData {
    fn to_packet_id(&self) -> u16 {
        10
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 10 {
            Some(LynxSetMotorChannelEnableCommandData { motor: data[0], enabled: data[1] != 0 })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        2
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct LynxGetMotorChannelModeCommandData {
    pub motor: u8,
}
impl Display for LynxGetMotorChannelModeCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxGetMotorChannelMode[motor:{}]", self.motor)
    }
}
impl Into<Vec<u8>> for LynxGetMotorChannelModeCommandData {
    fn into(self) -> Vec<u8> {
        vec![self.motor; 1]
    }
}
impl CommandData for LynxGetMotorChannelModeCommandData {
    fn to_packet_id(&self) -> u16 {
        9
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 9 {
            Some(LynxGetMotorChannelModeCommandData { motor: data[0] })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        1
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, TryFromPrimitive, Debug)]
pub enum DcMotorRunMode {
    RunWithoutEncoder = 0,
    RunUsingEncoder = 1,
    RunToPosition = 2
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, TryFromPrimitive, Debug)]
pub enum DcMotorZeroPowerBehavior {
    Brake = 0,
    Float = 1
}
#[derive(Clone, Copy, PartialEq)]
pub struct LynxGetMotorChannelModeResponseData {
    pub run_mode: DcMotorRunMode,
    pub zero_power_behavior: DcMotorZeroPowerBehavior
}
impl Display for LynxGetMotorChannelModeResponseData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxGetMotorChannelModeResponse[mode:{:?},zeroPowerBehavior:{:?}]", self.run_mode, self.zero_power_behavior)
    }
}
impl Into<Vec<u8>> for LynxGetMotorChannelModeResponseData {
    fn into(self) -> Vec<u8> {
        vec![self.run_mode as u8, self.zero_power_behavior as u8]
    }
}
impl CommandData for LynxGetMotorChannelModeResponseData {
    fn to_packet_id(&self) -> u16 {
        9 | RESPONSE_BIT
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == (9 | RESPONSE_BIT) {
            Some(LynxGetMotorChannelModeResponseData {
                run_mode: DcMotorRunMode::try_from(data[0]).expect("dc rm fail"),
                zero_power_behavior: DcMotorZeroPowerBehavior::try_from(data[1]).expect("dc zpm fail")
            })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        2
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct LynxSetMotorChannelModeCommandData {
    pub motor: u8,
    pub run_mode: DcMotorRunMode,
    pub zero_power_behavior: DcMotorZeroPowerBehavior
}
impl Display for LynxSetMotorChannelModeCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxSetMotorChannelModeCommand[motor:{},mode:{:?},zeroPowerBehavior:{:?}]", self.motor, self.run_mode, self.zero_power_behavior)
    }
}
impl Into<Vec<u8>> for LynxSetMotorChannelModeCommandData {
    fn into(self) -> Vec<u8> {
        vec![self.motor, self.run_mode as u8, self.zero_power_behavior as u8]
    }
}
impl CommandData for LynxSetMotorChannelModeCommandData {
    fn to_packet_id(&self) -> u16 {
        8
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 8 {
            Some(LynxSetMotorChannelModeCommandData {
                motor: data[0],
                run_mode: DcMotorRunMode::try_from(data[1]).expect("dc rm fail"),
                zero_power_behavior: DcMotorZeroPowerBehavior::try_from(data[2]).expect("dc zpm fail")
            })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        3
    }
}

#[derive(Clone, PartialEq)]
pub struct LynxI2cWriteMultipleBytesCommandData {
    pub i2c_bus: u8,
    pub i2c_addr_7bit: u8,
    pub payload: Vec<u8>
}
impl Display for LynxI2cWriteMultipleBytesCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2CWriteMultipleBytes[bus:{},7bitaddr:{},payload:{:?}]", self.i2c_bus, self.i2c_addr_7bit, self.payload)
    }
}
impl Into<Vec<u8>> for LynxI2cWriteMultipleBytesCommandData {
    fn into(self) -> Vec<u8> {
        if self.payload.len() > u8::MAX as usize {
            panic!("i2c payload too long!")
        }
        let mut init = vec![self.i2c_bus, self.i2c_addr_7bit, self.payload.len() as u8];
        init.extend_from_slice(self.payload.as_slice());
        init
    }
}
impl CommandData for LynxI2cWriteMultipleBytesCommandData {
    fn to_packet_id(&self) -> u16 {
        38
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 38 {
            let length = data[2] as usize;
            let payload = Vec::from(&data[3..length + 3]);
            Some(LynxI2cWriteMultipleBytesCommandData { i2c_bus: data[0], i2c_addr_7bit: data[1], payload })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        3 + self.payload.len()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct LynxI2CSingleByteWriteCommandData {
    pub i2c_bus: u8,
    pub i2c_addr_7bit: u8,
    pub value: u8
}
impl Display for LynxI2CSingleByteWriteCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2CSingleByteWrite[bus:{},7bitaddr: {}, value:{}]", self.i2c_bus, self.i2c_addr_7bit, self.value)
    }
}
impl Into<Vec<u8>> for LynxI2CSingleByteWriteCommandData {
    fn into(self) -> Vec<u8> {
        vec![self.i2c_bus, self.i2c_addr_7bit, self.value]
    }
}
impl CommandData for LynxI2CSingleByteWriteCommandData {
    fn to_packet_id(&self) -> u16 {
        37
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 37 {
            Some(LynxI2CSingleByteWriteCommandData { i2c_bus: data[0], i2c_addr_7bit: data[1], value: data[2] })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        3
    }
}


#[derive(Clone, Copy, PartialEq)]
pub struct LynxI2CReadStatusQueryCommandData {
    pub i2c_bus: u8,
}
impl Display for LynxI2CReadStatusQueryCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxI2CReadStatusQuery[bus:{}]", self.i2c_bus)
    }
}
impl Into<Vec<u8>> for LynxI2CReadStatusQueryCommandData {
    fn into(self) -> Vec<u8> {
        vec![self.i2c_bus]
    }
}
impl CommandData for LynxI2CReadStatusQueryCommandData {
    fn to_packet_id(&self) -> u16 {
        41
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 41 {
            Some(LynxI2CReadStatusQueryCommandData { i2c_bus: data[0] })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        1
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct LynxI2CWriteReadMultipleBytesCommandData {
    pub i2c_bus: u8,
    pub i2c_addr_7bit: u8,
    pub bytes_to_read: u8,
    pub i2c_start_addr: u8
}
impl Display for LynxI2CWriteReadMultipleBytesCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2CWriteReadMultipleBytes[bus:{},7bitaddr: {},btr:{},start:{}]", self.i2c_bus, self.i2c_addr_7bit, self.bytes_to_read, self.i2c_start_addr)
    }
}
impl Into<Vec<u8>> for LynxI2CWriteReadMultipleBytesCommandData {
    fn into(self) -> Vec<u8> {
        vec![self.i2c_bus, self.i2c_addr_7bit, self.bytes_to_read, self.i2c_start_addr]
    }
}
impl CommandData for LynxI2CWriteReadMultipleBytesCommandData {
    fn to_packet_id(&self) -> u16 {
        52
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 52 {
            Some(LynxI2CWriteReadMultipleBytesCommandData { i2c_bus: data[0], i2c_addr_7bit: data[1], bytes_to_read: data[2], i2c_start_addr: data[3] })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        4
    }
}

#[derive(Clone, PartialEq)]
pub struct LynxI2CReadStatusQueryResponseData {
    pub i2c_status: u8,
    pub data: Vec<u8>
}
impl Display for LynxI2CReadStatusQueryResponseData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2CReadStatusQueryResponse[i2c_status:{},data:{:?}]", self.i2c_status, self.data)
    }
}
impl Into<Vec<u8>> for LynxI2CReadStatusQueryResponseData {
    fn into(self) -> Vec<u8> {
        let mut t = vec![self.i2c_status, self.data.len() as u8];
        t.extend_from_slice(&self.data);
        t
    }
}
impl CommandData for LynxI2CReadStatusQueryResponseData {
    fn to_packet_id(&self) -> u16 {
        41 | RESPONSE_BIT
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == (41 | RESPONSE_BIT) {
            let len = data[1] as usize;
            Some(LynxI2CReadStatusQueryResponseData { i2c_status: data[0], data: Vec::from(&data[2..2+len]) })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        2 + self.data.len()
    }
}
/*
uint8_t channel;
uint8_t motorMode;
int32_t proportional_16q16; // signed
int32_t integral_16q16;     // signed
int32_t derivative_16q16;   // signed
int32_t feedforward_16q16;  // signed
uint8_t motorControlAlgorithm; // see MotorControlAlgorithm
*/
#[derive(Clone, PartialEq, )]
pub struct LynxSetMotorPIDFCommandData {
    pub motor: u8,
    pub mode: DcMotorRunMode,
    pub proportional: f64,//all of these an i32 on wire
    pub integral: f64,
    pub derivative: f64,
    pub feedforward: f64,
    pub algorithm: u8//irrelevant. pid vs pidf. use 1 always
}
impl LynxSetMotorPIDFCommandData {
    fn coefficient_to_internal(coefficient: f64) -> i32 {
        (coefficient.abs() * 65536.0 + 0.5).to_i32().expect("pidf coef. bad0") * coefficient.signum().to_i32().expect("pidf coef. bad1")
    }
    fn internal_to_coefficient(internal: i32) -> f64 {
        internal.to_f64().unwrap() / 65536.0f64
    }
    pub fn to_pidf(&self) -> PIDF {
        (self.proportional as f32, self.integral as f32, self.derivative as f32, self.feedforward as f32)
    }
    pub fn is_vel(&self) -> bool {
        self.mode == DcMotorRunMode::RunUsingEncoder
    }
    pub fn is_pos(&self) -> bool {
        self.mode == DcMotorRunMode::RunToPosition
    }
}
impl Into<Vec<u8>> for LynxSetMotorPIDFCommandData {
    fn into(self) -> Vec<u8> {
        let mut t = vec![self.motor, self.mode as u8];
        t.extend_from_slice(&Self::coefficient_to_internal(self.proportional).to_le_bytes());
        t.extend_from_slice(&Self::coefficient_to_internal(self.integral).to_le_bytes());
        t.extend_from_slice(&Self::coefficient_to_internal(self.derivative).to_le_bytes());
        t.extend_from_slice(&Self::coefficient_to_internal(self.feedforward).to_le_bytes());
        t.extend_from_slice(&[self.algorithm]);
        t
    }
}
impl Display for LynxSetMotorPIDFCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SetMotorPIDFCmd[motor:{},mode:{:?},p:{},i:{},d:{},f:{},algo:{}]", self.motor, self.mode, self.proportional, self.integral, self.derivative, self.feedforward, self.algorithm)
    }
}
impl CommandData for LynxSetMotorPIDFCommandData {
    fn to_packet_id(&self) -> u16 {
        51
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == 51 {
            let p = i32::from_le_bytes(data[2..6].try_into().unwrap());
            let i = i32::from_le_bytes(data[6..10].try_into().unwrap());
            let d = i32::from_le_bytes(data[10..14].try_into().unwrap());
            let f = i32::from_le_bytes(data[14..18].try_into().unwrap());
            Some(LynxSetMotorPIDFCommandData {
                motor: data[0],
                mode: DcMotorRunMode::try_from_primitive(data[1]).expect("pid mode bad"),
                proportional: Self::internal_to_coefficient(p),
                integral: Self::internal_to_coefficient(i),
                derivative: Self::internal_to_coefficient(d),
                feedforward: Self::internal_to_coefficient(f),
                algorithm: data[18],
            })
        } else { None }
    }

    fn get_bytes_len(&self) -> usize {
        19
    }
}

#[derive(Clone, PartialEq, )]
pub struct LynxSetMotorVelocityTargetCommandData {
    pub motor: u8,
    pub velocity: i16,
}
impl Into<Vec<u8>> for LynxSetMotorVelocityTargetCommandData {
    fn into(self) -> Vec<u8> {
        let mut t = vec![self.motor];
        t.extend_from_slice(&self.velocity.to_le_bytes());
        t
    }
}
impl Display for LynxSetMotorVelocityTargetCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SetMotorVelCmd[motor:{},vel:{}]", self.motor, self.velocity)
    }
}
impl CommandData for LynxSetMotorVelocityTargetCommandData {
    fn to_packet_id(&self) -> u16 {
        17
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == 17 {
            Some(LynxSetMotorVelocityTargetCommandData {
                motor: data[0],
                velocity: i16::from_le_bytes(data[1..3].try_into().unwrap())
            })
        } else { None }
    }

    fn get_bytes_len(&self) -> usize {
        3
    }
}

#[derive(Clone, PartialEq, )]
pub struct LynxSetMotorTargetPositionCommandData {
    pub motor: u8,
    pub position: i32,
    pub tolerance: u16
}
impl Into<Vec<u8>> for LynxSetMotorTargetPositionCommandData {
    fn into(self) -> Vec<u8> {
        let mut t = vec![self.motor];
        t.extend_from_slice(&self.position.to_le_bytes());
        t.extend_from_slice(&self.tolerance.to_le_bytes());
        t
    }
}
impl Display for LynxSetMotorTargetPositionCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SetMotorPosCmd[motor:{},pos:{},tol:{}]", self.motor, self.position, self.tolerance)
    }
}
impl CommandData for LynxSetMotorTargetPositionCommandData {
    fn to_packet_id(&self) -> u16 {
        19
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == 19 {
            Some(Self {
                motor: data[0],
                position: i32::from_le_bytes(data[1..5].try_into().unwrap()),
                tolerance: u16::from_le_bytes(data[5..7].try_into().unwrap())
            })
        } else { None }
    }

    fn get_bytes_len(&self) -> usize {
        7
    }
}

#[derive(Clone, PartialEq)]
pub struct LynxI2CWriteStatusQueryCommandData {
    pub i2c_bus: u8,
}
impl Into<Vec<u8>> for LynxI2CWriteStatusQueryCommandData {
    fn into(self) -> Vec<u8> {
        let mut t = vec![];
        t.extend_from_slice(&self.i2c_bus.to_le_bytes());
        t
    }
}
impl Display for LynxI2CWriteStatusQueryCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2CWriteStatusQueryCommand[i2c_addr:{}]", self.i2c_bus)
    }
}
impl CommandData for LynxI2CWriteStatusQueryCommandData {
    fn to_packet_id(&self) -> u16 {
        42
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == 42 {
            Some(Self {
                i2c_bus: data[0],
            })
        } else { None }
    }

    fn get_bytes_len(&self) -> usize {
        1
    }
}



#[derive(Clone, PartialEq)]
pub struct LynxI2CWriteStatusQueryResponseData {
    pub i2c_status: u8,
    pub bytes_written: u8
}
impl Display for LynxI2CWriteStatusQueryResponseData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2CWriteStatusQueryResponse[i2c_status:{},bytes_written:{}]", self.i2c_status, self.bytes_written)
    }
}
impl Into<Vec<u8>> for LynxI2CWriteStatusQueryResponseData {
    fn into(self) -> Vec<u8> {
        let t = vec![self.i2c_status, self.bytes_written];
        t
    }
}
impl CommandData for LynxI2CWriteStatusQueryResponseData {
    fn to_packet_id(&self) -> u16 {
        42 | RESPONSE_BIT
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == (42 | RESPONSE_BIT) {
            Some(Self { i2c_status: data[0], bytes_written: data[1] })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        2
    }
}

#[repr(u8)]
#[derive(Clone, PartialEq, Debug, TryFromPrimitive)]
pub enum I2CSpeedCode {
    UNKNOWN = 255,
    Stand100k = 0,
    Fast400K = 1,
    Fastplus1M = 2,
    High3_4M = 3
}
#[derive(Clone, PartialEq)]
pub struct LynxI2CConfigureChannelCommandData {
    pub i2c_bus: u8,
    pub speed_code: I2CSpeedCode
}
impl Into<Vec<u8>> for LynxI2CConfigureChannelCommandData {
    fn into(self) -> Vec<u8> {
        let mut t = vec![];
        t.extend_from_slice(&self.i2c_bus.to_le_bytes());
        t.extend_from_slice(&(self.speed_code as u8).to_le_bytes());
        t
    }
}
impl Display for LynxI2CConfigureChannelCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2CConfigureChannelCommand[i2c_addr:{},speed:{:?}]", self.i2c_bus, self.speed_code)
    }
}
impl CommandData for LynxI2CConfigureChannelCommandData {
    fn to_packet_id(&self) -> u16 {
        43
    }

    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self> {
        if id == 43 {
            let speed_code = if let Ok(it) = I2CSpeedCode::try_from(data[1]) {
                it
            } else {I2CSpeedCode::UNKNOWN};
            
            Some(Self {
                i2c_bus: data[0],
                speed_code
            })
        } else { None }
    }

    fn get_bytes_len(&self) -> usize {
        2
    }
}
