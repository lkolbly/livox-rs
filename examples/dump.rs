use std::time::{Duration, Instant};
use las::{Writer, Point, Color, Builder, GpsTimeType};
use las::point::Format;
use palette::{Gradient, Hsv, LinSrgb};
use chrono::TimeZone;
use std::f64;

use livox::{Sdk, LidarMode, DataPoint, DataPacket, LidarStateMask, CoordinateSystem};

fn build_point(x: f32, y: f32, z: f32, reflectivity: u8, time: Option<f64>) -> Point {
    let grad = Gradient::new(vec![
        Hsv::from(LinSrgb::new(1.0, 0.1, 0.1)),
        Hsv::from(LinSrgb::new(0.1, 1.0, 1.0)),
    ]);
    let color = grad.get(reflectivity as f64 / 100.0);
    let rgb = LinSrgb::from(color).into_format::<u16>();
    Point{
        x: x as f64,
        y: y as f64,
        z: z as f64,
        intensity: reflectivity as u16,
        gps_time: time,
        color: Some(Color{red: rgb.red, green: rgb.green, blue: rgb.blue}),
        ..Default::default()
    }
}

fn save_points(packet: DataPacket, las_writer: &mut las::Write) {
    let points = packet.points;
    for (i, point) in points.iter().enumerate() {
        let tm = Some((packet.timestamp - 1_000_000_000) as f64 + i as f64 * 0.000_01);
        match point {
            DataPoint::Spherical(p) => {
                let pi = f64::consts::PI;
                let x = p.depth * p.phi.cos() * p.theta.sin();
                let y = p.depth * p.phi.sin() * p.theta.sin();
                let z = p.depth * p.theta.cos();
                let p = build_point(x, y, z, p.reflectivity, tm);
                las_writer.write(p).unwrap();
            }
            DataPoint::Cartesian(p) => {
                let p = build_point(p.x, p.y, p.z, p.reflectivity, tm);
                las_writer.write(p).unwrap();
            }
        }
    }
}

fn main() {
    let mut builder = Builder::from((1, 4));
    builder.point_format = Format::new(3).unwrap();
    builder.gps_time_type = GpsTimeType::Standard;
    let header = builder.into_header().unwrap();
    let mut las_writer = Writer::from_path("points.laz", header).unwrap();

    let mut s = Sdk::new().unwrap();

    // Spin until we find a device
    let mut dev = loop {
        let v = s.list_known_devices();
        if v.len() > 0 {
            break s.connect(&v[0]).unwrap();
        }
    };

    dev.wait_for_state(LidarStateMask::Any);

    dev.set_mode(LidarMode::LidarModeNormal);
    dev.wait_for_state(LidarStateMask::Normal);

    dev.set_coordinate_system(CoordinateSystem::Spherical).unwrap();

    // Now read data for a bit
    {
        let mut ds = dev.start_sampling().unwrap();
        let now = Instant::now();
        while now.elapsed() < Duration::from_millis(5_000) {
            match ds.next() {
                Some(data_packet) => {
                    save_points(data_packet, &mut las_writer);
                }
                None => {}
            }
        }
    }

    dev.set_mode(LidarMode::LidarModePowerSaving);
}
