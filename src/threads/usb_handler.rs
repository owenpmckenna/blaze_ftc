use crate::serialization::packet::{Packet, Packets, FRAME_BYTES};
use crate::telemetry::telemetry::{get_class_loader, load_class};
use crate::{catch, BLAZEFTC_CLASS, JAVA_VM, RUNNING};
use core::slice;
use crossbeam_channel::{unbounded, Receiver, Sender};
use jni::objects::{JByteArray, JByteBuffer, JClass, JObject};
use jni::sys::jint;
use jni::{jni_sig, jni_str, AttachConfig, AttachGuard, Env, JValue, ScopeToken};
use log::log;
use std::error::Error;
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::task::ready;
use std::thread;
use std::time::Duration;
use spin_sleep::sleep;

pub fn start_usb_write_thread() -> Sender<Packets> {
    let (tx, rx) = unbounded();
    thread::spawn(move || {
        let vm = JAVA_VM.get().unwrap();
        let res = vm.attach_current_thread(move |env: &mut Env| -> Result<(), jni::errors::Error> {
            let mut rx = rx;
            let object: &JObject = BLAZEFTC_CLASS.get().unwrap().as_obj();
            let class_loader = get_class_loader(env, object);
            let mut blazeftc_class = load_class(env, &class_loader, "dev.anygeneric.blazeftc.BlazeFTC");
            //no sense making it small
            let mut bytes = vec![0u8; 1024];
            let bytes_buffer = unsafe { env.new_direct_byte_buffer(bytes.as_mut_ptr(), bytes.len()) }
                .expect("could not make direct byte buffer!");
            loop {
                do_write_thread(env, &mut rx, &blazeftc_class, &mut bytes, &bytes_buffer);
            }
        });
        log::info!("USB Write Thread Dead {:?}", res)
    });
    tx
}
fn do_write_thread(env: &mut Env, tx: &mut Receiver<Packets>, class: &JClass, vec: &mut Vec<u8>, buffer: &JByteBuffer) {
    let packets = tx.recv().expect("packet write thread no channel");
    let mut bytes = 0;
    let mut buf = vec.as_mut_slice();
    for i in packets {
        let (add, tbuf) = i.encode(buf);
        buf = tbuf;
        bytes += add;
    }
    //    public static void writeToUsb(byte[] b, int bytes)
    env.call_static_method(class,
                                        jni_str!("writeToUsb"),
                                        jni_sig!("(Ljava/nio/ByteBuffer;I)V"),
                                        &[JValue::Object(buffer), JValue::Int(bytes as jint)])
        .expect("call to writeToUsb fail - usb jni");

}
pub fn start_usb_read_thread() -> Receiver<Packet> {
    let (tx, rx) = unbounded();
    thread::spawn(move || {
        let vm = JAVA_VM.get().unwrap();
        let res = vm.attach_current_thread(move |env: &mut Env| -> Result<(), jni::errors::Error> {
            log::info!("usb read thread started...");
            let mut tx = tx;
            let object: &JObject = BLAZEFTC_CLASS.get().unwrap().as_obj();
            let class_loader = get_class_loader(env, object);
            let blazeftc_class = load_class(env, &class_loader, "dev.anygeneric.blazeftc.BlazeFTC");
            log::info!("usb read classloading done...");
            let mut bytes = vec![0u8; 1024];
            let bytes_buffer = unsafe { env.new_direct_byte_buffer(bytes.as_mut_ptr(), bytes.len()) }
                .expect("could not make direct byte buffer!");
            let mut assume_locked = false;
            log::info!("usb read thread entering loop...");
            loop {
                let pack: Option<Packet> = do_read_thread(env, &blazeftc_class, &mut bytes, &bytes_buffer, &mut assume_locked);
                match pack {
                    None => {log::info!("usb read thread derailed!!!"); assume_locked = false;}
                    Some(it) => {tx.send(it).expect("usb read could not send");}
                }
            }
        });
        log::info!("USB Read Thread Dead {:?}", res)
    });
    rx
}
fn do_read_thread(env: &mut Env, blazeftc: &JClass, vec: &mut Vec<u8>, bytes: &JByteBuffer, assume_lock: &mut bool) -> Option<Packet> {
    let mut pos = 0;
    if !*assume_lock {
        while pos <= 1 {
            while pos == 0 {
                do_read(env, blazeftc, bytes, 0, 1);
                if vec[pos] == FRAME_BYTES[0] {
                    log::info!("usb read good byte 1: {}", vec[0]);
                    pos += 1;
                } else {
                    log::info!("usb read bad byte 1: {}", vec[0])
                }
            }
            do_read(env, blazeftc, bytes, 1, 1);
            if vec[pos] == FRAME_BYTES[1] {
                pos += 1;//should get us out of the loop
            } else {
                log::info!("usb read bad byte 2: {}", vec[1]);
                pos = 0;//reset if bad
            }
        }
        log::info!("usb read good bytes locked");
        *assume_lock = true;
    }
    do_read(env, blazeftc, bytes, pos, 4 - pos);
    let num_to_read = u16::from_le_bytes(vec[2..4].try_into().expect("could not convert bytes type")) as usize;
    do_read(env, blazeftc, bytes, 4, (num_to_read - 4));
    Packet::from_data(&vec[0..num_to_read])
}
fn do_read(env: &mut Env, blaze: &JClass, bytes: &JByteBuffer, pos: usize, len: usize) {
    match env.call_static_method(blaze,
                                     jni_str!("readFromUsbExact"),
                                     jni_sig!("(Ljava/nio/ByteBuffer;II)V"),
                                     &[JValue::Object(bytes), JValue::Int(pos as jint), JValue::Int(len as jint)]) {
        Ok(_) => {}
        Err(it) => {
            log::info!("ERROR: failed on USB read: {}", it);
            RUNNING.store(false, Ordering::SeqCst);//uhh just kill it idk
            std::thread::sleep(Duration::from_secs(2));
            panic!("Failure while: {}", "USB Read Thread")
        }
    }
}