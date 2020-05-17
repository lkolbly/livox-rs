use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::collections::HashMap;
use std::ffi::CString;
use livox_sys;
use crate::callbacks::*;
use crate::device::*;

/// Rust-friendly wrapper around the Livox SDK.
pub struct Sdk {
    handle: Option<JoinHandle<()>>,
    kill: Sender<bool>,
    known_devices: Arc<Mutex<HashMap<String, bool>>>,
}

/// Creates a new SDK. Starts a thread to handle the logistics of tracking
/// devices and registering callbacks. Also calls Livox Start().
///
/// # Errors
///
/// This function will error if the Livox Init method returns an error,
/// or if the SDK has already been opened. If the SDK has been opened you
/// must drop the old Sdk object prior to creating a new one.
///
/// # Examples
///
/// ```
/// # use livox::Sdk;
/// {
///    let sdk = Sdk::new().unwrap(); // Inits the SDK
/// } // And uninits on close
/// ```
impl Sdk {
    pub fn new() -> Result<Sdk, ()> {
        match *BROADCAST_PIPE.lock().unwrap() {
            Some(_) => {
                // The SDK is already initialized - user should close the existing SDK
                return Err(());
            }
            None => {}
        }

        let result = unsafe { livox_sys::Init() };
        if !result {
            return Err(());
        }

        let (sender, broadcast_receiver) = channel();
        *BROADCAST_PIPE.lock().unwrap() = Some(sender);

        unsafe {
            livox_sys::SetBroadcastCallback(Some(broadcast_cb));
            livox_sys::SetDeviceStateUpdateCallback(Some(device_state_update_cb));
        }

        let known_devices: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));
        let known_devices_thread = Arc::clone(&known_devices);

        let (kill_sender, kill_recv) = channel();
        let handle = thread::spawn(move || {
            loop {
                match broadcast_receiver.try_recv() {
                    Ok(v) => {
                        let mut v2 = vec!();
                        for c in v.broadcast_code.iter() {
                            v2.push(*c as u8);
                        }
                        let code = String::from_utf8(v2).unwrap();
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
            livox_sys::Start();
        }

        return Ok(Sdk{
            handle: Some(handle),
            kill: kill_sender,
            known_devices: known_devices,
        });
    }

    /// Connects to the given device, returning a Device object.
    pub fn connect(&mut self, code: &str) -> Result<Device, ()> {
        // @TODO: Check whether the device is already in connected_devices

        let mut handle: u8 = 0;
        let bytes = CString::new(code).expect("CString::new failed");
        let res = unsafe { livox_sys::AddLidarToConnect(bytes.as_ptr(), &mut handle as *mut u8) };
        // @TODO: Check res
        println!("Handle: {}", handle);
        println!("Add lidar res = {}", res);

        Device::new(handle)
    }

    /// Returns a list of known devices, as a vector of strings representing the
    /// devices codes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use livox::Sdk;
    /// let mut sdk = Sdk::new().unwrap();
    /// println!("{:?}", sdk.list_known_devices());
    /// ```
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
            livox_sys::Uninit();
        }

        // Kill the thread
        if let Some(handle) = self.handle.take() {
            self.kill.send(true).unwrap();
            handle.join().unwrap();
        }

        *BROADCAST_PIPE.lock().unwrap() = None;
    }
}
