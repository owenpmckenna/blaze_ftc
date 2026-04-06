use std::fmt::{Debug, Display, Formatter};
use crate::serialization::command::Command;
use crate::serialization::commands::{NackData};

pub const FRAME_BYTES: [u8; 2] = [68u8, 75u8];
pub fn bytes_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    return a
        .iter()
        .enumerate()
        .find(|(i, val)| {
            **val != b[*i] //true if it failed
        })
        .is_none();
}
#[derive(Clone, PartialEq)]
pub struct Packet {
    pub packet_length: u16,
    pub dest_module_addr: u8,
    pub src_module_addr: u8,
    /**
     * ok so this is set by the ctrl hub before sending. when the packet is received, the msg num value has been switched to the ref num. no idea why
     */
    pub message_number: u8,
    /**
     * this is a hub-internal id, used for matching up requests with their responses.
     * no idea how it's different from a message number tbh
     */
    pub reference_number: u8,
    pub packet_id: u16, //yes this is unsigned don't worry. also, they add some offset to it which is why it's so huge
    pub payload_data: Command,
    pub checksum: u8,
}
impl Display for Packet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f,
            "Packet[len:{}, dest:{}, src:{}, msg num:{}, ref num:{}, id:{}, cmd:{}, chksum:{}]",
            self.packet_length,
            self.dest_module_addr,
            self.src_module_addr,
            self.message_number,
            self.reference_number,
            self.packet_id,
            self.payload_data,
            self.checksum
        )
    }
}
impl Debug for Packet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}
impl Packet {
    pub(crate) fn null() -> Packet {
        Packet {
            packet_length: 0,
            dest_module_addr: 0,
            src_module_addr: 0,
            message_number: 0,
            reference_number: 0,
            packet_id: 0,
            payload_data: Command::Nack(NackData {reason_code: 255}),
            checksum: 0,
        }
    }

    pub fn new(command: Command, dest_module_addr: u8, message_number: u8) -> Self {
        Self::new_full(command, dest_module_addr, 0, 0, message_number)
    }
    pub fn new_full(command: Command, dest_module_addr: u8, src_module_addr: u8, reference_number: u8, message_number: u8) -> Self {
        let packet_id = command.to_packet_id();
        let packet_length = (11 + command.get_bytes_len()) as u16;
        let mut packet = Packet {
            packet_length,
            dest_module_addr,
            src_module_addr,
            message_number,
            reference_number,
            packet_id,
            payload_data: command,
            checksum: 0,
        };
        packet.checksum = packet.checksum();
        packet
    }
    pub fn is_start(data: &[u8]) -> bool {
        bytes_equal(&data[0..2], &FRAME_BYTES)
    }
    pub fn from_data(data: &[u8]) -> Option<Packet> {
        if !Self::is_start(data) {
            //check 0,1
            return None;
        }
        let len_bytes = data[2..4].try_into().expect("could not convert bytes type 0");
        let len = u16::from_le_bytes(len_bytes); //2,3
        let dest_module_addr = u8::from_le_bytes([data[4]]); //4
        let src_module_addr = u8::from_le_bytes([data[5]]); //5
        let message_number = u8::from_le_bytes([data[6]]); //6
        let reference_number = u8::from_le_bytes([data[7]]); //7
        let packet_id_bytes = data[8..10].try_into().expect("could not convert bytes type 1"); //8,9
        let packet_id = u16::from_le_bytes(packet_id_bytes);
        let checksum = u8::from_le_bytes([data[len as usize - 1]]); //last byte
        let our_checksum = BSChecksum::new()
            .consume_bytes(&data[0..len as usize - 1])
            .value;
        if our_checksum != checksum {
            log::info!(
                "failed to validate checksum from bytes on wire! ours:{}, theirs:{}",
                our_checksum,
                checksum
            );
            return None; //maybe handle this???
        } //all except checksum byte are checked against checksum byte
        let command = Command::from_bytes(packet_id, &data[10..len as usize - 1], src_module_addr, dest_module_addr);
        let packet = Packet {
            packet_length: len,
            dest_module_addr,
            src_module_addr,
            message_number,
            reference_number,
            packet_id,
            payload_data: command,
            checksum,
        };
        log::trace!(
            "Just deserialized packet. wire chksm:{} our calc chksm:{}. ref:{}",
            checksum,
            packet.checksum(),
            reference_number
        );
        Some(packet)
    }
    pub fn checksum(&self) -> u8 {
        BSChecksum::new()
            .consume_bytes(&FRAME_BYTES) //forgot this the first time!
            .consume_u16(self.packet_length)
            .consume_u8(self.dest_module_addr)
            .consume_u8(self.src_module_addr)
            .consume_u8(self.message_number)
            .consume_u8(self.reference_number)
            .consume_u16(self.packet_id)
            .consume_vec_bytes(<Command as Into<Vec<u8>>>::into(
                self.payload_data.clone().into(),
            ))
            .into()
    }
}
impl Into<Vec<u8>> for Packet {
    fn into(mut self) -> Vec<u8> {
        self.checksum = self.checksum();
        let mut data = vec![0u8; 10];
        data[0..2].copy_from_slice(&FRAME_BYTES); //0,1
        data[2..4].copy_from_slice(&self.packet_length.to_le_bytes()); //2,3
        data[4] = self.dest_module_addr.to_le_bytes()[0]; //4
        data[5] = self.src_module_addr; //5
        data[6] = self.message_number; //6
        data[7] = self.reference_number; //7
        data[8..10].copy_from_slice(&self.packet_id.to_le_bytes()); //8,9
        let payload: Vec<u8> = self.payload_data.into();
        data.extend_from_slice(payload.as_slice());
        //data[10..10+payload.len()].copy_from_slice(payload.as_slice());//10 maybe
        //data[10+payload.len()] = self.checksum;//
        data.push(self.checksum);
        data
    }
}
pub struct BSChecksum {
    pub value: u8,
}
impl BSChecksum {
    fn new() -> BSChecksum {
        BSChecksum { value: 0 }
    }
    fn consume_u8(mut self, i: u8) -> BSChecksum {
        self.value = self.value.overflowing_add(i).0;
        self
    }
    fn consume_u16(self, i: u16) -> BSChecksum {
        self.consume_bytes(&i.to_le_bytes())
    }
    fn consume_bytes(mut self, bytes: &[u8]) -> BSChecksum {
        for b in bytes {
            self = self.consume_u8(*b)
        }
        self
    }
    fn consume_vec_bytes(mut self, bytes: Vec<u8>) -> BSChecksum {
        for b in bytes {
            self = self.consume_u8(b);
        }
        self
    }
}
impl Into<u8> for BSChecksum {
    fn into(self) -> u8 {
        self.value
    }
}
