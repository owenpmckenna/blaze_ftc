use crate::serialization::command_data::CommandData;
use crate::serialization::command_utils::{RESPONSE_BIT};
use num_enum::TryFromPrimitive;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LynxGetADCCommandData {
    channel: ADCCommandChannel,
    mode: ADCCommandMode,
}
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, TryFromPrimitive)]
pub enum ADCCommandMode {
    ENGINEERING = 0,
    RAW = 1,
}
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, TryFromPrimitive)]
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

#[derive(Copy, Clone, Debug, PartialEq)]
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


#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LynxGetBulkDataResponseData {
    pub digital_inputs: u8,
    pub motor_status: u8,
    pub motors: [MotorData; 4],
    pub analog: [i16; 4]
}
#[derive(Copy, Clone, Debug, PartialEq)]
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


#[derive(Clone, Copy, Debug, PartialEq)]
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


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LynxSetServoPositionCommandData {
    pub servo: u8,
    pub power: u16
}
impl Display for LynxSetServoPositionCommandData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LynxSetServoPosition[servo:{},power:{}]", self.servo, self.power)
    }
}
impl Into<Vec<u8>> for LynxSetServoPositionCommandData {
    fn into(self) -> Vec<u8> {
        let mut v = vec![self.servo; 1];
        v.extend_from_slice(&self.power.to_le_bytes());
        v
    }
}
impl CommandData for LynxSetServoPositionCommandData {
    fn to_packet_id(&self) -> u16 {
        34
    }
    fn from_bytes(id: u16, data: &[u8], _: u8) -> Option<Self> {
        if id == 34 {
            Some(LynxSetServoPositionCommandData { servo: data[0], power: u16::from_le_bytes(data[1..3].try_into().unwrap()) })
        } else { None }
    }
    fn get_bytes_len(&self) -> usize {
        3
    }
}
