use crate::serialization::command::Command;
use crate::serialization::packet::{FRAME_BYTES, Packet, bytes_equal};
use crossbeam_channel::{Receiver, Sender, unbounded};
use log::log;
use std::backtrace::Backtrace;
use std::cmp::PartialEq;
use std::io::{Error, Read};
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{panic, thread};
use crate::catch;
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
        let port = port;
        catch(move || initial_read_thread(port, tx, running), "hw read thread");
    });
    rx
}
fn initial_read_thread<T>(mut port: T, send: Sender<Packet>, running: &AtomicBool)
where
    T: Read + Send + UnwindSafe,
{
    let mut locked = false;
    let mut was_locked = locked;
    while running.load(Ordering::SeqCst) {
        log::trace!("initial read. locked:{}", locked);
        if !locked {
            was_locked = locked;
            locked = match attempt_lock(&mut port) {
                Ok(it) => it,
                Err(err) => {
                    log::trace!("Error while directly reading: {}", err);
                    return;
                }
            };
            continue;
        }
        log::trace!("reading 2 bytes: {}", was_locked == locked);
        let mut reading = vec![0u8, 0u8];
        if was_locked != locked {
            //we just ran attempt_lock and consumed the frame bytes
            reading.copy_from_slice(&FRAME_BYTES);
        } else {
            port.read_exact(reading.as_mut_slice()).unwrap(); //umm... no err handling for now
        }
        if !bytes_equal(reading.as_ref(), &FRAME_BYTES) {
            log::trace!("bytes not equal!");
            locked = false;
            continue;
        }
        reading.resize(4, 0);
        log::trace!("extending to 4 and then reading...");
        port.read_exact(&mut reading.as_mut_slice()[2..4]).unwrap(); //2,3: the ones we just added
        let num_to_read = u16::from_le_bytes(reading.as_slice()[2..4].try_into().unwrap());
        log::trace!("reading {} bytes", num_to_read);
        reading.resize(num_to_read as usize, 0);
        //get slice from 4 to end, we have written to 0-3 already
        let slice = &mut reading.as_mut_slice()[4..num_to_read as usize];
        port.read_exact(slice).unwrap();
        let packet = Packet::from_data(&reading);
        log::trace!("-EVADEBUG- read bytes from lynx: {:?}", reading);
        match packet {
            None => {
                log::info!("failed to read packet");
                locked = false;
                continue;
            }
            Some((packet, _)) => {
                log::trace!(
                    "did read packet! ref num: {}, msg num:{}",
                    packet.reference_number,
                    packet.message_number
                );
                send.send(packet).unwrap();
            } //we ignore the Vec<u8> it shouldn't matter
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
