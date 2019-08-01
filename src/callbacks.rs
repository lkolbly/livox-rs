use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use num_traits::FromPrimitive;

use crate::enums::*;
use crate::datapacket::DataPacket;

lazy_static! {
    pub static ref BROADCAST_PIPE: Mutex<Option<Sender<livox_sys::BroadcastDeviceInfo>>> = Mutex::new(None);
}

pub extern fn broadcast_cb(devinfo: *const livox_sys::BroadcastDeviceInfo) {
    let maybe_sender = BROADCAST_PIPE.lock().unwrap();
    match &*maybe_sender {
        Some(sender) => {
            let devinfo = unsafe { (*devinfo).clone() };
            sender.send(devinfo).unwrap();
        },
        None => {
            panic!("Broadcast pipe not setup but broadcast_cb called");
        }
    }
}

lazy_static! {
    pub static ref DEVICE_STATES: Mutex<HashMap<u8, LidarState>> = Mutex::new(HashMap::new());
}

pub extern fn device_state_update_cb(devinfo: *const livox_sys::DeviceInfo, _event: livox_sys::DeviceEvent) {
    let devinfo = unsafe { (*devinfo).clone() };
    (*DEVICE_STATES.lock().unwrap()).insert(
        devinfo.handle,
        match LidarState::from_u32(devinfo.state) {
            Some(x) => { x },
            None => {
                let state = devinfo.state;
                panic!("Got unknown state {}", state);
            },
        },
    );
}

lazy_static! {
    pub static ref DATA_PIPES: Mutex<HashMap<u8, Sender<DataPacket>>> = Mutex::new(HashMap::new());
}

pub extern fn data_cb(handle: u8, data: *mut livox_sys::LivoxEthPacket, data_size: u32, _user_data: *mut std::ffi::c_void) {
    match &(*DATA_PIPES.lock().unwrap()).get(&handle) {
        Some(sender) => {
            let dp = DataPacket::from((data, data_size));
            sender.send(dp).unwrap();
        }
        None => {
            // This can happen after the data stream is closed
        }
    }
}

pub extern fn common_command_cb(status: u8, handle: u8, response: u8, _client_data: *mut std::ffi::c_void) {
    println!("Command callback says: status={}, handle={}, response={}", status, handle, response);
}
