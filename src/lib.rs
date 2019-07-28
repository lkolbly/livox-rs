//use std::time::{Duration, Instant};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::collections::HashMap;
//use std::fs::File;
//use std::io::Write;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

#[repr(C)]
#[derive(Debug, Clone)]
struct BroadcastDeviceInfo {
    broadcast_code: [u8; 16],
    dev_type: u8,
    rsvd: u16,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub enum LidarState {
    LidarStateInit = 0,
    LidarStateNormal = 1,
    LidarStatePowerSaving = 2,
    LidarStateStandBy = 3,
    LidarStateError = 4,
    LidarStateUnknown = 5,
}

#[repr(C)]
#[derive(Debug, Clone)]
enum LidarFeature {
    LidarFeatureNone = 0,
    LidarFeatureRainFog = 1,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
struct DeviceInfo {
    broadcast_code: [u8; 16],
    handle: u8,
    slot: u8,
    id: u8,
    //rsvd: u8,
    device_type: u32,
    data_port: u16,
    cmd_port: u16,
    ip: [u8; 16],
    state: LidarState,
    feature: LidarFeature,
    status: u32,
}

#[repr(C)]
#[derive(Debug)]
enum DeviceEvent {
    EventConnect = 0,
    EventDisconnect = 1,
    EventStateChange = 2,
}

#[repr(C)]
struct LivoxEthPacket {
    version: u8,
    slot: u8,
    id: u8,
    rsvd: u8,
    err_code: u32,
    timestamp_type: u8,
    data_type: u8,
    timestamp: [u8; 8],
    data: [u8; 1], // This is of varying size
}

#[repr(C)]
pub enum LidarMode {
    LidarModeNormal = 1,
    LidarModePowerSaving = 2,
    LidarModeStandby = 3,
}

type CommonCommandCallback = extern fn(u8, u8, u8, *mut u8);

#[link(name = "livox_sdk_static", kind = "static")]
extern {
    fn Init() -> bool;
    fn Start() -> bool;
    fn Uninit();

    fn SetBroadcastCallback(cb: extern fn(*const BroadcastDeviceInfo));
    fn SetDeviceStateUpdateCallback(cb: extern fn(*const DeviceInfo, DeviceEvent));
    fn AddLidarToConnect(broadcast_code: *const u8, handle: *mut u8) -> u8;
    fn GetConnectedDevices(devices: *mut DeviceInfo, size: *mut u8) -> u8;
    fn SetDataCallback(handle: u8, cb: extern fn(u8, *const LivoxEthPacket, u32));
    fn LidarStartSampling(handle: u8, cb: CommonCommandCallback, client_data: *mut u8) -> u8;
    fn LidarSetMode(handle: u8, mode: LidarMode, cb: CommonCommandCallback, client_data: *mut u8) -> u8;
}

static mut broadcast_pipe: Option<Mutex<Sender<BroadcastDeviceInfo>>> = None;

extern fn broadcast_cb(devinfo: *const BroadcastDeviceInfo) {
    //println!("Broadcast called with {:?}!", (*devinfo).broadcast_code);
    unsafe {
        println!("Broadcast called!");
        match &broadcast_pipe {
            Some(mutex) => {
                let sender = mutex.lock().unwrap();
                sender.send((*devinfo).clone()).unwrap();
            }
            None => {
                panic!("Broadcast pipe not setup but broadcast_cb called");
            }
        }
    }
}

static mut device_state_pipe: Option<Mutex<Sender<(DeviceInfo, DeviceEvent)>>> = None;

extern fn device_state_update_cb(devinfo: *const DeviceInfo, event: DeviceEvent) {
    unsafe {
        println!("Device state update: {:?} event: {:?}", *devinfo, event);

        match &device_state_pipe {
            Some(mutex) => {
                let sender = mutex.lock().unwrap();
                sender.send(((*devinfo).clone(), event)).unwrap();
            }
            None => {
                panic!("Device state pipe not setup but device_state_update_cb called");
            }
        }
    }
}

#[derive(Debug)]
pub struct CartesianPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub reflectivity: u8,
}

#[derive(Debug)]
pub struct SphericalPoint {
    pub depth: f32,
    pub theta: f32,
    pub phi: f32,
    pub reflectivity: u8
}

#[derive(Debug)]
pub enum DataPoint {
    Cartesian(CartesianPoint),
    Spherical(SphericalPoint),
}

pub struct DataPacket {
    handle: u8,
    error_code: u32,
    timestamp: u64,
    points: Vec<DataPoint>,
}

impl DataPacket {
    fn add_cartesian(&mut self, data: &[u8], npoints: usize) {
        assert!(data.len() == npoints * 13);
        let mut rdr = Cursor::new(data.to_vec());
        for _ in 0..npoints {
            let x = rdr.read_i32::<LittleEndian>().unwrap();
            let y = rdr.read_i32::<LittleEndian>().unwrap();
            let z = rdr.read_i32::<LittleEndian>().unwrap();
            let reflectivity = rdr.read_u8().unwrap();
            self.points.push(DataPoint::Cartesian(CartesianPoint{
                x: x as f32 / 1000.0,
                y: y as f32 / 1000.0,
                z: z as f32 / 1000.0,
                reflectivity: reflectivity,
            }));
        }
    }

    fn add_spherical(&mut self, data: &[u8], npoints: usize) {
        assert!(data.len() == npoints * 9);
        let mut rdr = Cursor::new(data.to_vec());
        for _ in 0..npoints {
            let depth = rdr.read_u32::<LittleEndian>().unwrap();
            let theta = rdr.read_u16::<LittleEndian>().unwrap();
            let phi = rdr.read_u16::<LittleEndian>().unwrap();
            let reflectivity = rdr.read_u8().unwrap();
            self.points.push(DataPoint::Spherical(SphericalPoint{
                depth: depth as f32 / 1000.0,
                theta: theta as f32 / 100.0 / 180.0 * 3.14159265,
                phi: phi as f32 / 100.0 / 180.0 * 3.14159265,
                reflectivity: reflectivity,
            }));
        }
    }
}

fn parse_timestamp(data: &[u8]) -> u64 {
    let mut val = 0;
    for i in 0..8 {
        val = val * 256 + data[i] as u64;
    }
    val
}

static mut data_pipe: Option<Mutex<Sender<DataPacket>>> = None;

extern fn data_cb(handle: u8, data: *const LivoxEthPacket, data_size: u32) {
    //println!("Device {} got {} points", handle, data_size);
    unsafe {
        match &data_pipe {
            Some(mutex) => {
                let mut sender = mutex.lock().unwrap();
                //let data = unsafe { *data };
                if (*data).version != 5 {
                    panic!("Unknown data version {} encountered", (*data).version);
                }
                let time = if (*data).timestamp_type == 0 {
                    // Nanoseconds, unsync'd
                    parse_timestamp(&(*data).timestamp)
                        /*(*data).timestamp[0] as usize + 256 *
                        ((*data).timestamp[1] as usize + 256 *
                         ((*data).timestamp[2] as usize + 256 *
                          ((*data).timestamp[3] as usize + 256 *
                           ((*data).timestamp[4] as usize + 256 *
                            ((*data).timestamp[5] as usize + 256 *
                             ((*data).timestamp[6] as usize + 256 *
                              ((*data).timestamp[7] as usize)))))))*/
                } else {
                    panic!("Unknown timestamp type {}", (*data).timestamp_type);
                };

                //let mut points = vec!();
                let mut dp = DataPacket{
                    handle: handle,
                    error_code: (*data).err_code,
                    timestamp: time,
                    points: vec!(),
                };
                if (*data).data_type == 0 {
                    // Cartesian
                    let raw_points = std::slice::from_raw_parts(&(*data).data[0], data_size as usize * 13);
                    dp.add_cartesian(raw_points, data_size as usize);
                    /*let mut rdr = Cursor::new(raw_points.to_vec());
                    for _ in 0..data_size as usize {
                        let x = rdr.read_i32::<LittleEndian>().unwrap();
                        let y = rdr.read_i32::<LittleEndian>().unwrap();
                        let z = rdr.read_i32::<LittleEndian>().unwrap();
                        let reflectivity = rdr.read_u8().unwrap();
                        points.push(DataPoint::Cartesian(CartesianPoint{
                            x: x as f32 / 1000.0,
                            y: y as f32 / 1000.0,
                            z: z as f32 / 1000.0,
                            reflectivity: reflectivity,
                        }));
                    }*/
                } else if (*data).data_type == 1 {
                    let raw_points = std::slice::from_raw_parts(&(*data).data[0], data_size as usize * 9);
                    dp.add_spherical(raw_points, data_size as usize);
                    /*let mut rdr = Cursor::new(raw_points.to_vec());
                    for _ in 0..data_size as usize {
                        let depth = rdr.read_u32::<LittleEndian>().unwrap();
                        let theta = rdr.read_u16::<LittleEndian>().unwrap();
                        let phi = rdr.read_u16::<LittleEndian>().unwrap();
                        let reflectivity = rdr.read_u8().unwrap();
                        points.push(DataPoint::Spherical(SphericalPoint{
                            depth: depth as f32 / 1000.0,
                            theta: theta as f32 / 100.0 / 180.0 * 3.14159265,
                            phi: phi as f32 / 100.0 / 180.0 * 3.14159265,
                            reflectivity: reflectivity,
                        }));
                    }*/
                } else {
                    panic!("Unknown data type {}", (*data).data_type);
                }
                sender.send(dp).unwrap();
                /*sender.send(DataPacket{
                    handle: handle,
                    error_code: (*data).err_code,
                    timestamp: time,
                    points: points,
                }).unwrap();*/
            }
            None => {
                panic!("Data pipe not setup but data_cb called!");
            }
        }
    }
}

extern fn common_command_cb(status: u8, handle: u8, response: u8, _client_data: *mut u8) {
    println!("Command callback says: status={}, handle={}, response={}", status, handle, response);
}

//use crate::LidarMode::*;
//use crate::LidarState::*;

#[derive(Debug)]
pub struct LidarDevice {
    pub code: String,
    pub state: LidarState,
}

#[derive(Debug)]
pub struct LidarData {
    pub code: String,
    pub timestamp: u64,
    pub points: Vec<DataPoint>,
}

#[derive(Debug)]
pub enum LidarUpdate {
    Broadcast(String),
    StateChange(LidarDevice),
    Data(LidarData),
}

pub struct Scanner {
    handle: Option<JoinHandle<()>>,
    kill: Sender<bool>,
    connected_devices: Arc<Mutex<HashMap<String, u8>>>,
}

impl Scanner {
    pub fn new() -> Result<(Scanner, Receiver<LidarUpdate>), ()> {
        unsafe {
            match broadcast_pipe {
                Some(_) => {
                    // The SDK is already initialized - close the existing SDK
                    return Err(());
                }
                None => {
                    // We're good
                }
            }
        }

        let result = unsafe { Init() };
        if !result {
            return Err(());
        }

        let (sender, data_receiver) = channel();
        unsafe { data_pipe = Some(Mutex::new(sender)); }

        let (sender, broadcast_receiver) = channel();
        unsafe { broadcast_pipe = Some(Mutex::new(sender)); }

        let (sender, device_state_receiver) = channel();
        unsafe { device_state_pipe = Some(Mutex::new(sender)); }

        unsafe {
            SetBroadcastCallback(broadcast_cb);
            SetDeviceStateUpdateCallback(device_state_update_cb);
        }

        // Spin up a thread to process broadcasts and state changes
        let (update_sender, update_receiver) = channel();

        let connected_devices: Arc<Mutex<HashMap<String, u8>>> = Arc::new(Mutex::new(HashMap::new()));
        let connected_devices_thread = Arc::clone(&connected_devices);
        let (kill_sender, kill_recv) = channel();
        let handle = thread::spawn(move || {
            loop {
                match data_receiver.try_recv() {
                    Ok(v) => {
                        if v.error_code != 0 {
                            //panic!("Got error code {}", v.error_code);
                            println!("Got error code {}", v.error_code);
                        }
                        let devices = connected_devices_thread.lock().unwrap();
                        for (code, handle) in devices.iter() {
                            if *handle == v.handle {
                                update_sender.send(LidarUpdate::Data(LidarData{
                                    code: code.clone(),
                                    timestamp: v.timestamp as u64,
                                    points: v.points,
                                })).unwrap();
                                break;
                            }
                        }
                    }
                    Err(_) => {
                    }
                }
                match broadcast_receiver.try_recv() {
                    Ok(v) => {
                        println!("Broadcast: {:?}", v);
                        //let mut devices = thread_devices.lock().unwrap();
                        let code = String::from_utf8(v.broadcast_code.to_vec()).unwrap();
                        let devices = connected_devices_thread.lock().unwrap();
                        if !devices.contains_key(&code) {
                            update_sender.send(LidarUpdate::Broadcast(code)).unwrap();
                        }
                    }
                    Err(_) => {
                        //
                    }
                }
                match device_state_receiver.try_recv() {
                    Ok(v) => {
                        println!("Dev state: {:?}", v);
                        let code = String::from_utf8(v.0.broadcast_code.to_vec()).unwrap();
                        update_sender.send(LidarUpdate::StateChange(LidarDevice{code: code, state: v.0.state})).unwrap();
                    }
                    Err(_) => {
                        //
                    }
                }
                match kill_recv.try_recv() {
                    Ok(_) => {
                        break;
                    }
                    Err(_) => {
                        //
                    }
                }
            }
        });

        unsafe {
            Start();
        }

        return Ok((Scanner{
            handle: Some(handle),
            kill: kill_sender,
            connected_devices: connected_devices,
        }, update_receiver));
    }

    pub fn connect(&mut self, code: String) {
        let mut handle: u8 = 0;
        let res = unsafe { AddLidarToConnect(&(&code).as_bytes()[0] as *const u8, &mut handle as *mut u8) };
        // @TODO: Check res
        println!("Handle: {}", handle);
        println!("Add lidar res = {}", res);

        let mut devices = self.connected_devices.lock().unwrap();
        devices.insert(code, handle);
    }

    pub fn set_mode(&mut self, code: String, mode: LidarMode) {
        let mut devices = self.connected_devices.lock().unwrap();
        match devices.get(&code) {
            Some(handle) => {
                let res = unsafe { LidarSetMode(*handle, mode, common_command_cb, 0 as *mut u8) };
                // @TODO: Check the result
            }
            None => {
                panic!("Called set_mode on code {} which doesn't exist", code);
            }
        }
    }

    pub fn start_sampling(&mut self, code: String) {
        let mut devices = self.connected_devices.lock().unwrap();
        match devices.get(&code) {
            Some(handle) => {
                unsafe {
                    SetDataCallback(*handle, data_cb);
                    LidarStartSampling(*handle, common_command_cb, 0 as *mut u8);
                }
            }
            None => {
                panic!("Called start_sampling on code {} which isn't connected", code);
            }
        }
    }

    pub fn list_connected_devices(&self) -> Vec<String> {
        let devices = self.connected_devices.lock().unwrap();
        let mut v = vec!();
        for (code, _) in devices.iter() {
            v.push(code.clone());
        }
        v
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        unsafe {
            Uninit();
        }

        // Kill the thread
        if let Some(handle) = self.handle.take() {
            self.kill.send(true).unwrap();
            handle.join().unwrap();
        }

        unsafe {
            broadcast_pipe = None;
            device_state_pipe = None;
        }
    }
}
