use std::time::{Duration, Instant};
use std::fs::File;
use std::io::Write;
use las::{Writer, Point, Color, Builder};
use las::point::Format;

use livox::*;

fn main() {
    let mut builder = Builder::from((1, 4));
    builder.point_format = Format::new(2).unwrap();
    let header = builder.into_header().unwrap();
    let mut las_writer = Writer::from_path("points.las", header).unwrap();

    let (mut s, c) = Scanner::new().unwrap();
    let now = Instant::now();
    while now.elapsed() < Duration::from_millis(20_000) {
        match c.try_recv() {
            Ok(update) => {
                //println!("{:?}", update);
                match update {
                    LidarUpdate::Broadcast(code) => {
                        println!("Found device: {}", code);
                        s.connect(code);
                    }
                    LidarUpdate::StateChange(device) => {
                        if device.state == LidarState::LidarStateNormal {
                            // Start sampling
                            println!("Device entered Normal state");
                            s.start_sampling(device.code);
                        } else if device.state == LidarState::LidarStateInit || device.state == LidarState::LidarStatePowerSaving {
                            // Power up
                            println!("Device entered init or power saving state");
                            s.set_mode(device.code, LidarMode::LidarModeNormal);
                        }
                    }
                    LidarUpdate::Data(data) => {
                        for point in data.points {
                            match point {
                                DataPoint::Spherical(p) => {
                                    //file.write_fmt(format_args!("{},{},{},{}\n", p.depth, p.theta, p.phi, p.reflectivity));
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
                                    let p = Point{ x: p.x as f64, y: p.y as f64, z: p.z as f64, intensity: p.reflectivity as u16, color: Some(Color{red: rgb.red, green: rgb.green, blue: rgb.blue}), ..Default::default() };
                                    las_writer.write(p).unwrap();
                                    //file.write_fmt(format_args!("{},{},{},{}\n", p.x, p.y, p.z, p.reflectivity));
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
            }
        }
    }

    for device in s.list_connected_devices() {
        s.set_mode(device, LidarMode::LidarModePowerSaving);
    }
}
