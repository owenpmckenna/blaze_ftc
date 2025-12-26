use crate::serialization::command::Command;
use crate::serialization::packet::Packet;
use crossbeam_channel::{Receiver, RecvError, Sender, select, unbounded};
use jni::objects::JValue::Int;
use log::{info, log};
use std::backtrace::Backtrace;
use std::io::Write;
use std::ops::Sub;
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use std::{panic, thread};
use crate::catch;

const BYTES_PER_SEC: f64 = 460800f64 / 10f64;
const BYTES_PER_MILLIS: f64 = BYTES_PER_SEC / 1000f64;
pub static SEND_SATURATION: AtomicU64 = AtomicU64::new(0);//fraction. when using this, conv to f32/64 and div by 256
const PERCENTAGE: f64 = 0.002;//not a percentage, it's out of SendSaturationType::MAX
//something is scuffed. we were seeing like, 200% write speeds that *cannot* happen.

//~ 43 bytes per millis
pub fn generate_write_threads<T>(mut port: T, running: &'static AtomicBool) -> Sender<Packet>
where
    T: Write + Send + 'static + UnwindSafe,
{
    let (write_sender, write_receiver) = unbounded::<Packet>();
    thread::spawn(move || {
        catch(move || {
            let mut start = Instant::now();
            let mut lens = [(Instant::now(), 0usize); 100];
            let mut lens_id = 0usize;
            while running.load(Ordering::SeqCst) {
                let x = match write_receiver.recv() {
                    Ok(it) => it,
                    Err(it) => {
                        log::info!("RECV ERROR: {}", it);
                        return;
                    }
                };
                log::trace!(
                    "writing packet: ref num:{} len:{}, command:{}",
                    x.reference_number,
                    x.packet_length,
                    x.payload_data
                );
                let datas: Vec<u8> = x.into();
                log::trace!("-EVADEBUG- write bytes to lynx: {:?}", datas);
                let len = datas.len();
                port.write_all(datas.as_slice()).unwrap();

                lens[lens_id % lens.len()] = (Instant::now(), len);
                lens_id += 1;
                const WINDOW: u64 = 10;
                const LIMIT: Duration = Duration::from_millis(WINDOW);
                let mut bytes_sent_in_duration = 0;
                for i in lens {
                    if i.0.elapsed() > LIMIT {
                        continue
                    }
                    bytes_sent_in_duration += i.1;
                }
                let avg_saturation = (bytes_sent_in_duration as f64 / WINDOW as f64) / BYTES_PER_MILLIS;
                /*let elapsed = start.elapsed();
                let ms = elapsed.as_secs_f64() * 1000.0 + elapsed.subsec_micros() as f64 / 1000.0;
                let bytes_per_millis = datas.len() as f64 / ms;
                let saturation = bytes_per_millis / BYTES_PER_MILLIS;
                let avg_saturation = saturation * PERCENTAGE
                    + f64::from_bits(SEND_SATURATION.load(Ordering::SeqCst)) * (1.0 - PERCENTAGE);*/
                SEND_SATURATION.store(avg_saturation.to_bits(), Ordering::SeqCst);
                start = Instant::now();
            }
        }, "hw send");
    });
    write_sender
}
