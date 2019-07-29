use std::sync::mpsc::{Sender, Receiver, channel, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::collections::HashMap;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

#[macro_use]
extern crate lazy_static;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct BroadcastDeviceInfo {
    broadcast_code: [u8; 16],
    dev_type: u8,
    rsvd: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LidarState {
    LidarStateInit = 0,
    LidarStateNormal = 1,
    LidarStatePowerSaving = 2,
    LidarStateStandBy = 3,
    LidarStateError = 4,
    LidarStateUnknown = 5,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LidarStateMask {
    Init = 1,
    Normal = 2,
    PowerSaving = 4,
    StandBy = 8,
    Error = 16,
    Unknown = 32,
    Any = 0x1F,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
enum LidarFeature {
    LidarFeatureNone = 0,
    LidarFeatureRainFog = 1,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug, Clone, Copy)]
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
    //fn GetConnectedDevices(devices: *mut DeviceInfo, size: *mut u8) -> u8;
    fn SetDataCallback(handle: u8, cb: extern fn(u8, *const LivoxEthPacket, u32));
    fn LidarStartSampling(handle: u8, cb: CommonCommandCallback, client_data: *mut u8) -> u8;
    fn LidarStopSampling(handle: u8, cb: CommonCommandCallback, client_data: *mut u8) -> u8;
    fn LidarSetMode(handle: u8, mode: LidarMode, cb: CommonCommandCallback, client_data: *mut u8) -> u8;
}

lazy_static! {
    static ref BROADCAST_PIPE: Mutex<Option<Sender<BroadcastDeviceInfo>>> = Mutex::new(None);
}

extern fn broadcast_cb(devinfo: *const BroadcastDeviceInfo) {
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
    static ref DEVICE_STATES: Mutex<HashMap<u8, LidarState>> = Mutex::new(HashMap::new());
}

extern fn device_state_update_cb(devinfo: *const DeviceInfo, event: DeviceEvent) {
    let devinfo = unsafe { (*devinfo).clone() };
    (*DEVICE_STATES.lock().unwrap()).insert(
        devinfo.handle,
        devinfo.state,
    );
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
    pub timestamp: u64,
    pub points: Vec<DataPoint>,
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

/*lazy_static! {
    static ref DATA_PIPE: Mutex<Option<Sender<DataPacket>>> = Mutex::new(None);
}*/

lazy_static! {
    static ref DATA_PIPES: Mutex<HashMap<u8, Sender<DataPacket>>> = Mutex::new(HashMap::new());
}

extern fn data_cb(handle: u8, data: *const LivoxEthPacket, data_size: u32) {
    //match &*DATA_PIPE.lock().unwrap() {
    match &(*DATA_PIPES.lock().unwrap()).get(&handle) {
        Some(sender) => {
            let version = unsafe { (*data).version };
            let timestamp_type = unsafe { (*data).timestamp_type };
            let timestamp = unsafe { (*data).timestamp };
            let err_code = unsafe { (*data).err_code };
            let data_type = unsafe { (*data).data_type };

            if version != 5 {
                panic!("Unknown data version {} encountered", version);
            }
            let time = if timestamp_type == 0 {
                // Nanoseconds, unsync'd
                parse_timestamp(&timestamp)
            } else {
                panic!("Unknown timestamp type {}", timestamp_type);
            };

            let mut dp = DataPacket{
                //handle: handle,
                //error_code: err_code,
                timestamp: time,
                points: vec!(),
            };
            if data_type == 0 {
                // Cartesian
                let raw_points = unsafe { std::slice::from_raw_parts(&(*data).data[0], data_size as usize * 13) };
                dp.add_cartesian(raw_points, data_size as usize);
            } else if data_type == 1 {
                let raw_points = unsafe { std::slice::from_raw_parts(&(*data).data[0], data_size as usize * 9) };
                dp.add_spherical(raw_points, data_size as usize);
            } else {
                panic!("Unknown data type {}", data_type);
            }
            sender.send(dp).unwrap();
        }
        None => {
            // This can happen after the data stream is closed
            //panic!("Data pipe not setup but data_cb called!");
        }
    }
}

extern fn common_command_cb(status: u8, handle: u8, response: u8, _client_data: *mut u8) {
    println!("Command callback says: status={}, handle={}, response={}", status, handle, response);
}

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

pub struct DataStream {
    handle: u8,
    receiver: Receiver<DataPacket>,
}

impl DataStream {
    fn new(handle: u8) -> Result<DataStream, ()> {
        let (sender, receiver) = channel();
        (*DATA_PIPES.lock().unwrap()).insert(
            handle.clone(),
            sender,
        );
        unsafe {
            SetDataCallback(handle, data_cb);
            LidarStartSampling(handle, common_command_cb, 0 as *mut u8);
        }
        Ok(DataStream{
            handle: handle,
            receiver: receiver,
        })
    }
}

impl Iterator for DataStream {
    type Item = DataPacket;

    fn next(&mut self) -> Option<DataPacket> {
        match self.receiver.try_recv() {
            Ok(packet) => {
                Some(packet)
            }
            Err(TryRecvError::Empty) => {
                None
            }
            Err(TryRecvError::Disconnected) => {
                panic!("Received disconnect error in DataStream iterator!");
            }
        }
    }
}

impl Drop for DataStream {
    fn drop(&mut self) {
        unsafe {
            LidarStopSampling(self.handle, common_command_cb, 0 as *mut u8);
        }

        (*DATA_PIPES.lock().unwrap()).remove(&self.handle);
    }
}

pub struct Device {
    code: String,
    handle: u8,
}

/// Interface for a single Livox device.
impl Device {
    fn new(code: String, handle: u8) -> Result<Device, ()> {
        (*DEVICE_STATES.lock().unwrap()).insert(
            handle,
            LidarState::LidarStateUnknown,
        );
        Ok(Device{
            code: code,
            handle: handle,
        })
    }

    /// Blocks until the device reaches a state that's permissible by the given
    /// mask. Note that it does not time out, so be sure to call set_mode before
    /// calling this method!
    pub fn wait_for_state(&mut self, state_mask: LidarStateMask) {
        loop {
            let state = match (*DEVICE_STATES.lock().unwrap()).get(&self.handle) {
                Some(state) => { state.clone() },
                None => { LidarState::LidarStateUnknown },
            };
            if state_mask as u32 & (1 << (state) as u32) != 0 {
                break;
            }
        }
    }

    /// Sends a command to set the mode of the device. The device state may not
    /// instantaneously change, be sure to call wait_for_state after calling.
    pub fn set_mode(&mut self, mode: LidarMode) {
        let res = unsafe { LidarSetMode(self.handle, mode, common_command_cb, 0 as *mut u8) };
        // @TODO: Check the result
    }

    /// Starts sampling. Returns a DataStream which can be used to retrieve data
    /// points.
    pub fn start_sampling(&mut self) -> Result<DataStream, ()> {
        let ds = DataStream::new(self.handle)?;
        Ok(ds)
    }
}

/// Rust-friendly wrapper around the Livox SDK.
pub struct Sdk {
    handle: Option<JoinHandle<()>>,
    kill: Sender<bool>,
    known_devices: Arc<Mutex<HashMap<String, bool>>>,
}

impl Sdk {
    /// Creates a new SDK. Starts a thread to handle the logistics of tracking
    /// devices and registering callbacks. Also calls Livox Start().
    ///
    /// # Errors
    ///
    /// This function will error if the Livox Init method returns an error,
    /// or if the SDK has already been opened. If the SDK has been opened you
    /// must drop the old Sdk object prior to creating a new one.
    pub fn new() -> Result<(Sdk, Receiver<LidarUpdate>), ()> {
        match *BROADCAST_PIPE.lock().unwrap() {
            Some(_) => {
                // The SDK is already initialized - close the existing SDK
                return Err(());
            }
            None => {}
        }

        let result = unsafe { Init() };
        if !result {
            return Err(());
        }

        let (sender, broadcast_receiver) = channel();
        *BROADCAST_PIPE.lock().unwrap() = Some(sender);

        unsafe {
            SetBroadcastCallback(broadcast_cb);
            SetDeviceStateUpdateCallback(device_state_update_cb);
        }

        // Spin up a thread to process broadcasts and state changes
        let (update_sender, update_receiver) = channel();

        let known_devices: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));
        let known_devices_thread = Arc::clone(&known_devices);

        let (kill_sender, kill_recv) = channel();
        let handle = thread::spawn(move || {
            loop {
                match broadcast_receiver.try_recv() {
                    Ok(v) => {
                        let code = String::from_utf8(v.broadcast_code.to_vec()).unwrap();
                        let mut devices = known_devices_thread.lock().unwrap();
                        devices.insert(code.clone(), false);
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

        return Ok((Sdk{
            handle: Some(handle),
            kill: kill_sender,
            known_devices: known_devices,
        }, update_receiver));
    }

    /// Connects to the given device, returning a Device object.
    pub fn connect(&mut self, code: String) -> Result<Device, ()> {
        // @TODO: Check whether the device is already in connected_devices

        let mut handle: u8 = 0;
        let res = unsafe { AddLidarToConnect(&(&code).as_bytes()[0] as *const u8, &mut handle as *mut u8) };
        // @TODO: Check res
        println!("Handle: {}", handle);
        println!("Add lidar res = {}", res);

        Ok(Device {
            code: code,
            handle: handle,
        })
    }

    pub fn list_known_devices(&self) -> Vec<String> {
        let devices = self.known_devices.lock().unwrap();
        let mut v = vec!();
        for (code, _) in devices.iter() {
            v.push(code.clone());
        }
        v
    }
}

impl Drop for Sdk {
    /// Un-inits the Livox SDK and kills all threads.
    fn drop(&mut self) {
        unsafe {
            Uninit();
        }

        // Kill the thread
        if let Some(handle) = self.handle.take() {
            self.kill.send(true).unwrap();
            handle.join().unwrap();
        }

        *BROADCAST_PIPE.lock().unwrap() = None;
    }
}
