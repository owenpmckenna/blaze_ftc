use crate::serialization::packet::{Packet, Packets, FRAME_BYTES};
use crate::telemetry::telemetry::{get_class_loader, load_class};
use crate::{catch, BLAZEFTC_CLASS, JAVA_VM, RUNNING};
use core::slice;
use crossbeam_channel::{unbounded, Receiver, Sender};
use jni::objects::{JByteArray, JByteBuffer, JClass, JObject};
use jni::sys::{jbyte, jint, jsize};
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
            let mut bytes = vec![0u8; 2048];
            loop {
                do_write_thread(env, &mut rx, &blazeftc_class, &mut bytes);
            }
        });
        log::info!("USB Write Thread Dead {:?}", res)
    });
    tx
}
fn do_write_thread(env: &mut Env, tx: &mut Receiver<Packets>, class: &JClass, vec: &mut Vec<u8>) {
    let packets = tx.recv().expect("packet write thread no channel");
    let packets_len = packets.len();
    let mut bytes = 0;
    let mut buf = vec.as_mut_slice();
    for i in packets {
        let (add, tbuf) = i.encode(buf);
        buf = tbuf;
        bytes += add;
    }
    log::trace!("usb writing {} packets as {} bytes", packets_len, bytes);
    let arr = env.byte_array_from_slice(&vec[0..bytes]).expect("could not create byte array from slice");
    //    public static void writeToUsb(byte[] b, int bytes)
    env.call_static_method(class,
                                        jni_str!("writeToUsb"),
                                        jni_sig!("([B)V"),
                                        &[JValue::Object(&arr)])
        .expect("call to writeToUsb fail - usb jni");
    env.delete_local_ref(arr);
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
            const LEN: usize = 1024;
            let mut bytes = [0u8; LEN];
            let bytes_buffer = env.new_byte_array(LEN).expect("could not make direct byte buffer!");
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
fn do_read_thread(env: &mut Env, blazeftc: &JClass, vec: &mut [u8], bytes: &JByteArray, assume_lock: &mut bool) -> Option<Packet> {
    let mut pos = 0;
    if !*assume_lock {
        while pos <= 1 {
            while pos == 0 {
                do_read(env, blazeftc, vec, bytes, 0, 1);
                if vec[pos] == FRAME_BYTES[0] {
                    log::info!("usb read good byte 1: {}", vec[0]);
                    pos = 1;
                } else {
                    log::info!("usb read bad byte 1: {}", vec[0]);
                }
            }
            do_read(env, blazeftc, vec, bytes, 1, 1);
            if vec[pos] == FRAME_BYTES[1] {
                pos = 2;
            } else {
                log::info!("usb read bad byte 2: {}", vec[1]);
                pos = 0;//reset if bad
            }
        }
        log::info!("usb read good bytes locked");
        *assume_lock = true;
    }
    //either the next two bytes (the length, a u16), or all four if we were already locked
    do_read(env, blazeftc, vec, bytes, pos, 4 - pos);
    let packet_len = u16::from_le_bytes(vec[2..4].try_into().expect("could not convert bytes type")) as usize;
    do_read(env, blazeftc, vec, bytes, 4, (packet_len - 4));
    let pack = Packet::from_data(&vec[0..packet_len]);
    println!("USB READ PACKET!!! {:?}", pack);
    pack
}
fn do_read(env: &mut Env, blaze: &JClass, vec: &mut [u8], bytes: &JByteArray, pos: usize, len: usize) {
    let mut tmp_vec = Vec::with_capacity(len);
    match env.call_static_method(blaze,
                                     jni_str!("readFromUsbExact"),
                                     jni_sig!("([BII)V"),
                                     &[JValue::Object(bytes), JValue::Int(pos as jint), JValue::Int(len as jint)])
        .map(|_| {
            let mut jb = vec![0 as jbyte; len];
            bytes.get_region(env, pos as jsize, &mut jb)?;
            Ok(jb)
        }).flatten() {
        Ok(it) => {
            for i in 0..len {
                tmp_vec.push(it[i] as u8);
                vec[pos + i] = it[i] as u8
            }
        }
        Err(it) => {
            log::info!("ERROR: failed on USB read: {}", it);
            RUNNING.store(false, Ordering::SeqCst);//uhh just kill it idk
            thread::sleep(Duration::from_secs(2));
            panic!("Failure while: {}", "USB Read Thread")
        }
    }
    println!("just read from usb bytes: {:?}", tmp_vec)
}