use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{panic, thread};
use std::time::Instant;
use crossbeam_channel::{select, unbounded, Sender};
use crate::sdk_proxy::proxy::IdTransform;
use crate::serialization::command::Command;
use crate::serialization::lynx_commands::base_lynx_command::LynxCommand;
use crate::serialization::packet::Packet;
use crate::telemetry::telemetry::get_allowed_to_send_dangerous_packets;

pub(crate) type MessageList = Mutex<Vec<Instant>>;
pub(crate) fn register_packet(mg: &MessageList) -> u8 {
    let mut current_things = mg.lock().expect("could not lock MessageList");

    let mut x = 0usize;
    let mut old = Instant::now();
    for i in 0..current_things.len() {
        if current_things[i] < old {
            x = i;
            old = current_things[i];
        }
    }
    log::trace!("got oldest id: {}", x);
    //just use the oldest one. this will... probably work
    current_things[x] = Instant::now();
    x as u8
}
/**
 * ok so the first return is the normal one, the second is the wierd one (for ftc sdk).
 */
pub fn generate_write_sdk_proxy(
    to_write: Sender<Packet>,
    packets_to_watch: Arc<Mutex<Vec<IdTransform>>>,
    running: &'static AtomicBool,
) -> (Sender<Packet>, Sender<Packet>, Arc<MessageList>) {
    let (regular_tx, regular_rx) = unbounded::<Packet>();
    let (ftcsdk_tx, ftcsdk_rx) = unbounded::<Packet>();
    let current_things_old = Arc::new(Mutex::new(vec![Instant::now(); u8::MAX as usize]));
    let current_things = current_things_old.clone();
    thread::spawn(move || {
        match panic::catch_unwind(move || {
            while running.load(Ordering::SeqCst) {
                let (redo_id, mut packet) = select! {
                    recv(regular_rx) -> msg => {
                        log::trace!("ftc write proxy: got our packet");
                        (false, msg.expect("could not receive from regular rx"))
                    }
                    recv(ftcsdk_rx) -> msg => {
                        let x = msg.expect("could not receive from ftcsdk tx");
                        log::trace!("ftc write proxy: got FTC packet, msg num:{}, ref num:{}", x.message_number, x.reference_number);
                        (true, x)
                    }
                };
                if redo_id {
                    let x = register_packet(&current_things);
                    let mut lock = packets_to_watch.lock().unwrap();
                    let transform = IdTransform {
                        old_id: packet.message_number,
                        new_id: x,
                        old_pack_id: packet.packet_id,
                        old_pack: packet.clone(),
                        sent_time: Instant::now()
                    }; //TODO: ok. this should be ref num but we're testing with msg num
                    log::trace!(
                        "rewriting ftc packet: old ref:{}, old id:{}, new ref:{}",
                        packet.reference_number,
                        packet.message_number,
                        transform.new_id
                    );
                    packet.reference_number = transform.new_id;
                    packet.message_number = transform.new_id; //TODO: determine if this is... a good idea
                    packet.checksum = packet.checksum(); //set checksum whoops
                    to_write.send(packet).unwrap();
                    lock.push(transform);
                } else {
                    //log::trace!("rewriting our packet! new ref number:{}", x);
                    packet.checksum = packet.checksum();
                    to_write.send(packet).unwrap();
                }
            }
        }) {
            Ok(_) => {}
            Err(it) => {
                log::info!("ERROR IN WRITE SDK PROXY");
                if let Some(s) = it.downcast_ref::<&str>() {
                    log::info!("Caught panic: {}", s);
                } else if let Some(s) = it.downcast_ref::<String>() {
                    log::info!("Caught panic: {}", s);
                } else {
                    log::info!("Caught unknown panic type");
                }
            }
        };
        println!("write sdk proxy exiting");
    });
    //note to self. put MessageList in the proxy impl, and make a LynxHub method that turns LynxCommands into packets and sends them, returning the message id.
    (regular_tx, ftcsdk_tx, current_things_old)
}

fn is_legal(pack: &mut Packet) -> bool {
    if get_allowed_to_send_dangerous_packets() {
        return true
    }
    match &mut pack.payload_data {
        Command::LynxCommand(it) => {
            match &mut it.command {
                LynxCommand::LynxSetMotorPowerCommand(it) => {
                    it.power = 0;
                    true
                },
                LynxCommand::LynxSetServoPulseWidthCommand(_) => true,
                _ => true
            }
        }
        _ => {true}
    }
}
