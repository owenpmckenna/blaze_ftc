use crate::serialization::packet::{FRAME_BYTES, Packet, bytes_equal};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::io::Read;
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use thread_priority::{get_current_thread_priority, set_current_thread_priority, ThreadPriority};
use crate::catch;
use crate::control::hardware::LynxHub;

/**
 * FYI the port is generic because I was switching out serialport implementations a lot at the beginning
 * and wanted to stop having to fix other code when I tried something
 */
pub fn generate_read_threads<T>(port: T, running: &'static AtomicBool) -> Receiver<Packet>
where
    T: Read + Send + UnwindSafe + 'static,
{
    let (tx, rx) = unbounded::<Packet>();
    thread::spawn(move || {
        catch(move || initial_read_thread(port, tx, running), "hw read thread");
    });
    rx
}
fn initial_read_thread<T>(mut port: T, send: Sender<Packet>, running: &AtomicBool)
where
    T: Read + Send + UnwindSafe,
{
    {
        let core_ids = core_affinity::get_core_ids().unwrap();
        let worked = core_affinity::set_for_current(core_ids[0]);
        log::info!("read thread just attempted to pin to core {}. return: {}", core_ids[0].id, worked);
        log::info!("just set read thread priority: old: {:?} err: {:?}, new: {:?}",
            get_current_thread_priority().expect("could not get thread priority read"),
            set_current_thread_priority(ThreadPriority::Max),
            get_current_thread_priority().expect("could not get thread priority read - 2"),
        );
    }
    let mut locked = false;
    let mut was_locked = locked;
    let mut reading = vec![0u8; 128];//much larger than any packet we should receive
    while running.load(Ordering::SeqCst) {
        //log::info!("initial read. locked:{}", locked);
        if !locked {
            was_locked = false;
            //should only run once, at the start of the program
            locked = match attempt_lock(&mut port) {
                Ok(it) => it,
                Err(err) => {
                    log::trace!("Error while directly reading: {}", err);
                    return;
                }
            };
            continue;
        }
        //log::info!("read 1 locked:{}", locked);
        if was_locked != locked {
            //we just ran attempt_lock and consumed the frame bytes. read the length only
            reading[0..2].copy_from_slice(&FRAME_BYTES[0..2]);//0,1
            port.read_exact(&mut reading[2..4]).expect("could not read packet header!");
        } else {
            //read frames and also length
            port.read_exact(&mut reading[0..4]).expect("could not read packet header!");
        }
        //log::info!("read 2");
        if !bytes_equal(&reading[0..2], &FRAME_BYTES) {
            log::trace!("bytes not equal!");
            locked = false;
            continue;
        }
        //log::info!("read 3");
        let num_to_read = u16::from_le_bytes(reading.as_slice()[2..4].try_into().expect("could not convert bytes type"));
        //log::info!("reading {} bytes", num_to_read);
        if reading.len() < num_to_read as usize {
            reading.resize(num_to_read as usize, 0);
        }
        //get slice from 4 to end, we have written to 0-3 already
        let slice = &mut reading.as_mut_slice()[4..num_to_read as usize];
        port.read_exact(slice).expect("failed to read exact bytes of packet len");
        //log::info!("read 4, creating packets");
        let packet = Packet::from_data(&reading);
        //log::trace!("-EVADEBUG- read bytes from lynx: {:?}", reading);
        match packet {
            None => {
                //log::info!("failed to read packet");
                locked = false;
                continue;
            }
            Some(packet) => {
                if let Some(hub) = LynxHub::get_for_id_careful(packet.src_module_addr) {
                    hub.notify_receive_packet();
                }
                log::trace!("did read packet! pack:{:?}",packet);
                send.send(packet).expect("failed to send to channel!");
            }
        }
        was_locked = locked;
    }
}
fn attempt_lock<T>(port: &mut T) -> Result<bool, std::io::Error>
where
    T: Read,
{
    let mut buf = [0u8; 1];
    log::trace!("attempt lock");
    port.read_exact(&mut buf)?;
    if FRAME_BYTES[0] != buf[0] {
        log::info!("fail: buf:{} != FB:{}", buf[0], FRAME_BYTES[0]);
        return Ok(false);
    }
    port.read_exact(&mut buf)?;
    if FRAME_BYTES[1] != buf[0] {
        log::info!("fail2: buf:{} != FB:{}", buf[0], FRAME_BYTES[1]);
        return Ok(false);
    }
    Ok(true)
}
