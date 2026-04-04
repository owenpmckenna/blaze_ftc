use std::sync::{Arc, LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use crossbeam_channel::{unbounded, Receiver, Sender};
use log::log;
use crate::control::hardware::{LynxHub, UnderlyingHw};
use crate::{catch, HUB_0, HUB_1};
use crate::sdk_proxy::proxy::{IdTransform, TIMING_TRACKER};
use crate::serialization::command::Command;
use crate::serialization::packet::Packet;
use crate::threads::timing_analyzer::TimingAnalyzer;

/**
 * First returned is for regular reads. Second is to be read by ftc sdk
 */
pub fn generate_read_sdk_proxy(
    to_read: Receiver<Packet>,
    packets_to_watch: Arc<Mutex<Vec<IdTransform>>>,
    running: &'static AtomicBool,
) -> (Receiver<Packet>, Receiver<Packet>, Sender<Packet>) {
    let (regular_read_sender, regular_read_receiver) = unbounded::<Packet>();
    let (ftcsdk_read_sender, ftcsdk_read_receiver) = unbounded::<Packet>();
    let sdk_receive_input = ftcsdk_read_sender.clone();
    thread::spawn(move || {
        catch(move || {
            let regular_read_sender = regular_read_sender.clone();
            let mut first = true;
            let mut first_packet = None;
            let mut had_second_packet = false;
            while running.load(Ordering::SeqCst) {
                let mut d = to_read.recv().unwrap();
                if let Command::QueryInterfaceResponse(it) = &d.payload_data {
                    log::info!("Got query interface response data! Writing directly to our code!");
                    regular_read_sender.send(d).unwrap();
                    continue;
                }
                if first {
                    log::info!("received first packet. not overwriting.");
                    first_packet = Some(d.clone());
                    first = false;
                    ftcsdk_read_sender.send(d).unwrap();
                    continue;
                }
                if !had_second_packet {
                    let val = first_packet.as_ref().unwrap();
                    if *val == d {
                        log::info!("haven't received a different packet yet! reading normally...");
                        ftcsdk_read_sender.send(d).unwrap(); //if we haven't received another packet, they are still looking for this one and we should keep sending it
                        continue;
                    } else {
                        log::info!("finally received second packet in read proxy!");
                        had_second_packet = true; //continue!
                    }
                }
                let mut lock = packets_to_watch.lock().unwrap();
                log::trace!(
                    "read: searching for packet ref num {} in list. (len {})",
                    d.reference_number,
                    lock.len()
                );
                let reference = lock.iter().enumerate().find_map(|(i, x)| {
                    if (x.new_id) == d.reference_number {
                        Some((i, x.clone()))
                    } else {
                        None
                    }
                });
                //so we're choosing which one to use, pretty much
                match reference {
                    None => {
                        log::trace!(
                            "forwarding message from proxy to our code id:{},cmd:{}",
                            d.reference_number,
                            d.payload_data
                        );
                        regular_read_sender.send(d).unwrap();
                    }
                    Some(it) => {
                        TIMING_TRACKER.update(it.1.sent_time.elapsed());
                        log::trace!(
                            "forwarding message from proxy to ftc old_id:{}, cmd:{}",
                            it.1.old_id,
                            d.payload_data
                        );
                        lock.remove(it.0);
                        d.message_number = it.1.old_id; //TODO: again, is this a good idea? idk what msg # does for sdk
                        d.reference_number = it.1.old_id;
                        Command::log_pack_id(d.reference_number, &it.1.old_pack, &d);
                        ftcsdk_read_sender.send(d).unwrap();
                    }
                }
            }
        }, "Read Sdk Proxy Method");
    });
    (regular_read_receiver, ftcsdk_read_receiver, sdk_receive_input)
}