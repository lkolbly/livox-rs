use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

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

impl From<(*mut livox_sys::LivoxEthPacket, u32)> for DataPacket {
    fn from((data, data_size): (*mut livox_sys::LivoxEthPacket, u32)) -> Self {
        let version = unsafe { (*data).version };
        let timestamp_type = unsafe { (*data).timestamp_type };
        let timestamp = unsafe { (*data).timestamp };
        let err_code = unsafe { (*data).err_code };
        let data_type = unsafe { (*data).data_type };

        // Bit 9 is the PPS status - 0 is no signal, 1 is signal OK.
        if err_code&!(1 << 9) != 0 {
            panic!("Error code in data packet: {}", err_code);
        }

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
        dp
    }
}

fn parse_timestamp(data: &[u8]) -> u64 {
    let mut val = 0;
    for i in 0..8 {
        val = val * 256 + data[i] as u64;
    }
    val
}
