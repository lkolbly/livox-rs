use std::sync::mpsc::{Receiver, channel, TryRecvError};
use livox_sys;
use num_traits::ToPrimitive;
use crate::enums::*;
use crate::datapacket::DataPacket;
use crate::callbacks::*;

pub struct DataStream {
    handle: u8,
    receiver: Receiver<DataPacket>,
}

impl DataStream {
    fn new(handle: u8) -> Result<DataStream, ()> {
        let (sender, receiver) = channel();
        // @TODO: Check that there isn't already a handle there
        (*DATA_PIPES.lock().unwrap()).insert(
            handle.clone(),
            sender,
        );
        unsafe {
            livox_sys::SetDataCallback(handle, Some(data_cb), 0 as *mut std::ffi::c_void);
            livox_sys::LidarStartSampling(handle, Some(common_command_cb), 0 as *mut std::ffi::c_void);
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
            livox_sys::LidarStopSampling(self.handle, Some(common_command_cb), 0 as *mut std::ffi::c_void);
        }

        (*DATA_PIPES.lock().unwrap()).remove(&self.handle);
    }
}

pub struct Device {
    handle: u8,
}

/// Interface for a single Livox device.
impl Device {
    // @TODO: This really shouldn't be public
    pub fn new(handle: u8) -> Result<Device, ()> {
        (*DEVICE_STATES.lock().unwrap()).insert(
            handle,
            LidarState::LidarStateUnknown,
        );
        Ok(Device{
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
            // @TODO: Make this check a method on LidarStateMask
            if state_mask as u32 & (1 << (state) as u32) != 0 {
                break;
            }
        }
    }

    /// Sends a command to set the mode of the device. The device state may not
    /// instantaneously change, be sure to call wait_for_state after calling.
    pub fn set_mode(&mut self, mode: LidarMode) {
        let res = unsafe { livox_sys::LidarSetMode(self.handle, mode.to_u32().unwrap(), Some(common_command_cb), 0 as *mut std::ffi::c_void) };
        // @TODO: Check the result
    }

    /// Starts sampling. Returns a DataStream which can be used to retrieve data
    /// points.
    pub fn start_sampling(&mut self) -> Result<DataStream, ()> {
        let ds = DataStream::new(self.handle)?;
        Ok(ds)
    }

    pub fn set_coordinate_system(&mut self, system: CoordinateSystem) -> Result<(), ()> {
        let res = match system {
            CoordinateSystem::Cartesian => {
                unsafe { livox_sys::SetCartesianCoordinate(self.handle, Some(common_command_cb), 0 as *mut std::ffi::c_void) }
            }
            CoordinateSystem::Spherical => {
                unsafe { livox_sys::SetSphericalCoordinate(self.handle, Some(common_command_cb), 0 as *mut std::ffi::c_void) }
            }
        };
        // @TODO: Check the result
        Ok(())
    }
}
