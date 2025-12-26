use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{panic, thread};
use std::time::Instant;
use crossbeam_channel::{select, unbounded, Sender};
use crate::sdk_proxy::proxy::IdTransform;
use crate::serialization::packet::Packet;

/**
 * ok so the first return is the normal one, the second is the wierd one (for ftc sdk).
 */
pub fn generate_write_sdk_proxy(
    to_write: Sender<Packet>,
    packets_to_watch: Arc<Mutex<Vec<IdTransform>>>,
    running: &'static AtomicBool,
) -> (Sender<Packet>, Sender<Packet>) {
    let (regular_tx, regular_rx) = unbounded::<Packet>();
    let (ftcsdk_tx, ftcsdk_rx) = unbounded::<Packet>();
    thread::spawn(move || {
        match panic::catch_unwind(move || {
            let mut current_things = vec![Instant::now(); u8::MAX as usize];
            while running.load(Ordering::SeqCst) {
                let (redo_id, mut packet) = select! {
                    recv(regular_rx) -> msg => {
                        log::trace!("ftc write proxy: got our packet");
                        (false, msg.unwrap())
                    }
                    recv(ftcsdk_rx) -> msg => {
                        let x = msg.unwrap();
                        log::trace!("ftc write proxy: got FTC packet, msg num:{}, ref num:{}", x.message_number, x.reference_number);
                        (true, x)
                    }
                };

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
                if redo_id {
                    let mut lock = packets_to_watch.lock().unwrap();
                    let transform = IdTransform {
                        old_id: packet.message_number,
                        new_id: x as u8,
                        old_pack_id: packet.packet_id,
                        old_pack: packet.clone(),
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
                    log::trace!("rewriting our packet! new ref number:{}", x);
                    packet.reference_number = x as u8;
                    packet.message_number = x as u8; //TODO: determine if this is... a good idea
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
    (regular_tx, ftcsdk_tx)
}
