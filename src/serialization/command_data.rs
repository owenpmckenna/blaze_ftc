use crate::serialization::command_utils::RESPONSE_BIT;
use std::fmt::{Debug, Display};

pub trait CommandData: Sized + Send + Clone + Display + Into<Vec<u8>> + PartialEq {
    fn to_packet_id(&self) -> u16;
    fn from_bytes(id: u16, data: &[u8], dest_addr: u8) -> Option<Self>;
    fn get_bytes_len(&self) -> usize;
    fn command_number(pack_id: u16) -> u16 {
        let x = 0b1000000000000000u16;
        pack_id & !x //clear response bit
    }
    fn is_response(pack_id: u16) -> bool {
        let x = RESPONSE_BIT;
        (pack_id & x) > 0
        //is first bit set? if yes, this is a response packet. probably.
    }
}
