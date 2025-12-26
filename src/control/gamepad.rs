use std::fmt::Binary;
use std::io::{Cursor, Read};
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};

#[derive(Copy, Clone, PartialEq)]
pub struct Gamepad {
    pub id: i32,
    pub timestamp: i64,
    pub left_stick_x: f32,
    pub left_stick_y: f32,
    pub right_stick_x: f32,
    pub right_stick_y: f32,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,
    pub guide: bool,//idk what this is tbh
    pub start: bool ,
    pub back: bool ,
    pub left_bumper: bool ,
    pub right_bumper: bool ,
    pub left_stick_button: bool ,
    pub right_stick_button: bool ,
    pub left_trigger: f32,
    pub right_trigger: f32 ,
    pub circle: bool ,//PS4 things?
    pub cross: bool ,
    pub triangle: bool ,
    pub square: bool ,
    pub share: bool ,
    pub options: bool,
    pub touchpad: bool,//wtf controller has a touchpad
    pub touchpad_finger_1: bool,
    pub touchpad_finger_2: bool,
    pub touchpad_finger_1_x: f32,
    pub touchpad_finger_1_y: f32,
    pub touchpad_finger_2_x: f32,
    pub touchpad_finger_2_y: f32,
    pub ps: bool,//Playstation, probably
}
impl Gamepad {
    pub fn new() -> Gamepad {
        Gamepad {
            id: 0,
            timestamp: 0,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            right_stick_x: 0.0,
            right_stick_y: 0.0,
            dpad_up: false,
            dpad_down: false,
            dpad_left: false,
            dpad_right: false,
            a: false,
            b: false,
            x: false,
            y: false,
            guide: false,
            start: false,
            back: false,
            left_bumper: false,
            right_bumper: false,
            left_stick_button: false,
            right_stick_button: false,
            left_trigger: 0.0,
            right_trigger: 0.0,
            circle: false,
            cross: false,
            triangle: false,
            square: false,
            share: false,
            options: false,
            touchpad: false,
            touchpad_finger_1: false,
            touchpad_finger_2: false,
            touchpad_finger_1_x: 0.0,
            touchpad_finger_1_y: 0.0,
            touchpad_finger_2_x: 0.0,
            touchpad_finger_2_y: 0.0,
            ps: false,
        }
    }
    fn print_bits(arr: &[u8]) {
        let str = arr.iter()
            .map(|&byte| format!("{:08b}", byte))  // Convert each byte to its 8-bit binary representation
            .collect::<Vec<String>>()             // Collect them into a Vec of Strings
            .join(" ");                       // Join them into a single string without spaces
        log::info!("gamepad: {}", str);
    }
    pub fn read_into(&mut self, buf: &[u8]) {
        //Self::print_bits(buf);
        let mut cursor: Cursor<&[u8]> = Cursor::new(buf);
        /*cursor.read_u8().unwrap();
        cursor.read_u8().unwrap();
        cursor.read_u8().unwrap();
        cursor.read_u8().unwrap();
        cursor.read_u8().unwrap();

        cursor.read_u8().unwrap();//read the id, i think this is always 5
        self.id = cursor.read_i32::<LittleEndian>().unwrap();
        self.timestamp = cursor.read_i64::<LittleEndian>().unwrap();*/
        let mut buffer = [0u8; 18];
        cursor.read_exact(&mut buffer).unwrap();
        self.left_stick_x = cursor.read_f32::<BigEndian>().unwrap();
        self.left_stick_y = cursor.read_f32::<BigEndian>().unwrap();
        self.right_stick_x = cursor.read_f32::<BigEndian>().unwrap();
        self.right_stick_y = cursor.read_f32::<BigEndian>().unwrap();
        self.left_trigger = cursor.read_f32::<BigEndian>().unwrap();
        self.right_trigger = cursor.read_f32::<BigEndian>().unwrap();
        let buttons = cursor.read_i32::<BigEndian>().unwrap();
        self.touchpad_finger_1  = (buttons & 0x20000) != 0;
        self.touchpad_finger_2  = (buttons & 0x10000) != 0;
        self.touchpad           = (buttons & 0x08000) != 0;
        self.left_stick_button  = (buttons & 0x04000) != 0;
        self.right_stick_button = (buttons & 0x02000) != 0;
        self.dpad_up            = (buttons & 0x01000) != 0;
        self.dpad_down          = (buttons & 0x00800) != 0;
        self.dpad_left          = (buttons & 0x00400) != 0;
        self.dpad_right         = (buttons & 0x00200) != 0;
        self.a                  = (buttons & 0x00100) != 0;
        self.b                  = (buttons & 0x00080) != 0;
        self.x                  = (buttons & 0x00040) != 0;
        self.y                  = (buttons & 0x00020) != 0;
        self.guide              = (buttons & 0x00010) != 0;
        self.start              = (buttons & 0x00008) != 0;
        self.back               = (buttons & 0x00004) != 0;
        self.left_bumper        = (buttons & 0x00002) != 0;
        self.right_bumper       = (buttons & 0x00001) != 0;
    }
}


/*
 * Note: code based on the Gamepad code found in the ftc sdk. In the interest of transparency,
 * here is their copyright notice, which may apply to file:
 * 
 * Copyright (c) 2014, 2015 Qualcomm Technologies Inc
 *
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without modification, are permitted
 * (subject to the limitations in the disclaimer below) provided that the following conditions are
 * met:
 *
 * Redistributions of source code must retain the above copyright notice, this list of conditions
 * and the following disclaimer.
 *
 * Redistributions in binary form must reproduce the above copyright notice, this list of conditions
 * and the following disclaimer in the documentation and/or other materials provided with the
 * distribution.
 *
 * Neither the name of Qualcomm Technologies Inc nor the names of its contributors may be used to
 * endorse or promote products derived from this software without specific prior written permission.
 *
 * NO EXPRESS OR IMPLIED LICENSES TO ANY PARTY'S PATENT RIGHTS ARE GRANTED BY THIS LICENSE. THIS
 * SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
 * WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 * FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
 * LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
 * (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA,
 * OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF
 * THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */