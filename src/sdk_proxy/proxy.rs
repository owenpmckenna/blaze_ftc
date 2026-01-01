use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crate::control::robot::{Interceptor, Robot};
use crate::sdk_proxy::read_proxy::generate_read_sdk_proxy;
use crate::sdk_proxy::send_proxy::generate_write_sdk_proxy;
use crate::serialization::packet::Packet;

pub struct Proxy {
    running: &'static AtomicBool,
    /**
     * This is the channel that the sdk should send into
     */
    sdk_send: Sender<Packet>,
    /**
     * This is the channel that the sdk should read from
     */
    sdk_receive: Receiver<Packet>,
    /**
     * this sends to sdk_receive. have fun!
     */
    sdk_receive_input: Sender<Packet>,
    sdk_buffer: (Sender<u8>, Receiver<u8>),
    sdk_has_received: AtomicBool,
    ftc_packets: Arc<Mutex<Vec<IdTransform>>>,
    /**
     * these are called for input packets. They can return None to stop a packet. They can also
     * return mutated packets
     */
    interceptors: Mutex<Vec<Box<dyn Interceptor>>>
}
impl Proxy {
    pub fn new(direct_send: Sender<Packet>, direct_receive: Receiver<Packet>, running: &'static AtomicBool) -> (Sender<Packet>, Receiver<Packet>, Proxy) {
        let ftc_packets = Arc::new(Mutex::new(vec![]));
        let (send, sdk_send) = generate_write_sdk_proxy(direct_send, ftc_packets.clone(), running);
        let (receive, sdk_receive, sdk_receive_input) = generate_read_sdk_proxy(direct_receive, ftc_packets.clone(), running);
        (send, receive, Proxy {
            running,
            sdk_send,
            sdk_receive,
            sdk_receive_input,
            sdk_buffer: unbounded(),
            sdk_has_received: AtomicBool::new(false),
            ftc_packets,
            interceptors: Mutex::new(vec![])
        })
    }
    pub fn write(&self, data: Vec<u8>) {
        let packet = Packet::from_data(&data);
        match packet {
            None => {
                log::error!("data from java not complete. discarding...")
            }
            Some(it) => {
                log::trace!("data from java received! ref num:{}", it.0.reference_number);
                let mut lock = self.interceptors.lock().unwrap();
                let mut packet = Some(it.0);
                for interceptor in lock.iter_mut() {
                    if let Some(pack) = packet {
                        //this may return None and swallow the packet
                        packet = interceptor.intercept(pack, &self.sdk_receive_input);
                    } else { return; }
                }
                if let Some(pack) = packet {
                    self.sdk_send.send(pack).unwrap();
                }
            }
        }
    }
    pub fn read(&self, len: usize) -> Vec<u8> {
        let usable = &self.sdk_receive;
        let rx = &self.sdk_buffer.1;
        let tx = &self.sdk_buffer.0;

        if rx.is_empty() {
            log::trace!("waiting on packet for ftc!");
            let data = match usable.recv() {
                Ok(it) => it,
                Err(it) => {
                    log::error!("error waiting for ftc data: {}", it);
                    sleep(Duration::new(1, 0));
                    panic!("error waiting for ftc data: {}", it)
                }
            };
            let id = data.packet_id;
            let data: Vec<u8> = data.into();
            log::trace!("got packet for ftc! p id:{}, len:{}", id, data.len());
            data.into_iter().for_each(|x| tx.send(x).unwrap());
        }

        //this is horribly hacky. The packets become misaligned here because they assume that the frame bytes
        //are actually the number of bytes to read because, well, they just read the frame bytes from the real stream.
        //It has to do with when we are able to switch out the streams and what they've already read.
        //so we drop the first 4 bytes. They think they're already locked so they've seen the length because
        //when they're locked they read 4 to get frame bytes and length
        if !self.sdk_has_received.load(Ordering::SeqCst) {
            log::info!("READ - first read! len asked for {}", len);
            if len == 1 {
                log::info!("READ - evidently FTC reader not locked, not dropping data")
            } else if len == 2 {
                //This should never happen. if they are locked they ask for 4 bytes. just covering bases
                log::info!(
                    "READ - FTC reader status uncertain. Giving packet length since that will force reread if they were looking for frame bytes I think."
                );
                for i in 0..2 {
                    let i = rx.recv().unwrap();
                    log::info!("dropping byte: {}", i);
                }
            } else {
                for i in 0..4 {
                    let i = rx.recv().unwrap();
                    log::info!("dropping byte: {}", i);
                }
            }
            self.sdk_has_received.store(true, Ordering::SeqCst);
        }

        let mut temp: Vec<u8> = vec![];
        while temp.len() < len && !rx.is_empty() {
            temp.push(rx.recv().unwrap())
        }

        temp
    }
    pub fn add_interceptor(&self, interceptor: Box<dyn Interceptor>) {
        self.interceptors.lock().unwrap().push(interceptor);
    }
}
#[derive(Clone)]
pub struct IdTransform {
    pub old_id: u8,
    pub new_id: u8,
    pub old_pack_id: u16,
    pub old_pack: Packet,
}