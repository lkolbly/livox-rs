use std::time::{Duration, Instant};
use las::{Writer, Point, Color, Builder};
use las::point::Format;

use livox::{Sdk, LidarUpdate, LidarState, LidarMode, DataPoint};

fn main() {
    let mut builder = Builder::from((1, 4));
    builder.point_format = Format::new(2).unwrap();
    let header = builder.into_header().unwrap();
    let mut las_writer = Writer::from_path("points.las", header).unwrap();

    let (mut s, c) = Sdk::new().unwrap();
    let now = Instant::now();
    while now.elapsed() < Duration::from_millis(20_000) {
        match c.try_recv() {
            Ok(update) => {
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
