use std::time::{Duration, Instant};
use las::{Writer, Point, Color, Builder};
use las::point::Format;
use std::io::BufWriter;
use std::fs::File;

use livox::{Sdk, LidarUpdate, LidarState, LidarMode, DataPoint, LidarStateMask};

fn save_points(points: Vec<DataPoint>, las_writer: &mut Writer<BufWriter<File>>) {
    for point in points.iter() {
    match point {
        DataPoint::Spherical(p) => {
            let p = Point{ x: 1.0, y: 1.0, z: 1.0, ..Default::default() };
            las_writer.write(p).unwrap();
        }
        DataPoint::Cartesian(p) => {
            let grad = palette::Gradient::new(vec![
                palette::Hsv::from(palette::LinSrgb::new(1.0, 0.1, 0.1)),
                palette::Hsv::from(palette::LinSrgb::new(0.1, 1.0, 1.0)),
            ]);
            let color = grad.get(p.reflectivity as f64 / 100.0);
            let rgb = palette::LinSrgb::from(color).into_format::<u16>();
            let p = Point{
                x: p.x as f64,
                y: p.y as f64,
                z: p.z as f64,
                intensity: p.reflectivity as u16,
                color: Some(Color{red: rgb.red, green: rgb.green, blue: rgb.blue}),
                ..Default::default()
            };
            las_writer.write(p).unwrap();
        }
    }
    }
}

fn main() {
    let mut builder = Builder::from((1, 4));
    builder.point_format = Format::new(2).unwrap();
    let header = builder.into_header().unwrap();
    let mut las_writer = Writer::from_path("points.las", header).unwrap();

    let (mut s, c) = Sdk::new().unwrap();

    // Spin until we find a device
    let mut dev = loop {
        let v = s.list_known_devices();
        if v.len() > 0 {
            break s.connect(v[0].clone()).unwrap();
        }
    };
    /*let mut dev = loop {
        match c.try_recv() {
            Ok(LidarUpdate::Broadcast(code)) => {
                println!("Found device: {}", code);
                break s.connect(code).unwrap();
            }
            Ok(_) => {
                // Don't care
            }
            Err(_) => {
                // @TODO: Nothing-to-receive should not be an error here!
            }
        }
    };*/

    dev.wait_for_state(LidarStateMask::Any);

    dev.set_mode(LidarMode::LidarModeNormal);
    dev.wait_for_state(LidarStateMask::Normal);

    // Now read data for 20s
    let now = Instant::now();
    {
        let mut ds = dev.start_sampling().unwrap();
        while now.elapsed() < Duration::from_millis(20_000) {
            match ds.next() {
                Some(data_packet) => {
                    save_points(data_packet.points, &mut las_writer);
                }
                None => {}
            }
        }
    }

    dev.set_mode(LidarMode::LidarModePowerSaving);
}
