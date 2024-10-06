mod state;
mod state_parser;
mod port_io;

use std::{collections::VecDeque, env, fs::read_to_string, path::Path, time::{Duration, Instant}};
use config::Config;
use gcode::Mnemonic;
use log::{info, warn};
use port_io::{fake_cnc_port, port_info_from_config, uart_read_write, SerialPortInfo};
use state::{AppMode, CncEvent, DebounceDiffTracker, DelayUpdates, DiffTracker, Point3, RemoteEvent, TrackCurrentPrevious};
use tokio::{sync::{broadcast, mpsc::{self, Sender}}, task::{self, yield_now}};

async fn event_brain_loop(mut xbee_events: broadcast::Receiver<RemoteEvent>, xbee_tx: Sender<String>, mut cnc_events: broadcast::Receiver<CncEvent>, cnc_tx: Sender<String>) {
    let mut mode = AppMode::Jog;
    let mut is_absolute = true;
    let mut cnc_has_communicted = true;
    let mut dial: DiffTracker<Point3<i64>> = DiffTracker::new(Default::default());
    let mut prev_dial_time = Instant::now();
    let mut cnc_position: DebounceDiffTracker<Point3<f32>> = DebounceDiffTracker::new(Point3::<f32>::default(), Duration::from_millis(100));
    //let mut current_feed_rate = 0;
    let mut gcode_buffer = VecDeque::<String>::new();
    let mut gcode_processing = VecDeque::<String>::new();
    let cnc_buffer_size = 5;
    loop {
        //info!("brain loop.");
        yield_now().await;
        if let Ok(x_event) = xbee_events.try_recv() {
            match x_event {
                RemoteEvent::DialXYZEvent(p) => { *dial.current_mut() = p; },
                RemoteEvent::SDList((path, skip)) => {
                    let path = Path::new(&path);
                    if path.is_dir() {
                        let paths = std::fs::read_dir(path).unwrap().enumerate().skip(skip).take(5);
                        for (i, path) in paths {
                            if path.is_err() {
                                continue;
                            }
                            xbee_tx.send(format!("L:{} {}", i, path.unwrap().file_name().to_str().unwrap())).await.unwrap();
                        }
                    }
                },
                RemoteEvent::SDLoadFile(file_path) => {
                    let p = Path::new(&file_path);
                    if p.is_file() {
                        //todo configurable pre-job
                        gcode_buffer.push_back("G90".to_owned());
                        for line in read_to_string(p).unwrap().lines() {
                            gcode_buffer.push_back(line.to_string());
                        }
                        //todo configurable post-job
                        gcode_buffer.push_back("G90".to_owned());
                        gcode_buffer.push_back("G21".to_owned());
                        //gcode_buffer.push_back("M02".to_owned());
                    }
                },
                RemoteEvent::RunGCode(gcode) => { gcode_buffer.push_back(gcode); },
            };
        }
        if let Ok(cnc_event) = cnc_events.try_recv() {
            if !cnc_has_communicted && mode == AppMode::Uninitialized {
                cnc_has_communicted = true;
                mode = AppMode::Jog;
            }
            match cnc_event {
                CncEvent::Unknown => {},
                CncEvent::Ok => { gcode_processing.pop_front(); },
                CncEvent::PositionReport(_) => todo!(),
                CncEvent::EndStopStates(_) => todo!(),
            }
        }

        if mode == AppMode::Jog && gcode_processing.len() < cnc_buffer_size && dial.needs_update() {
            let dial_3 = dial.current();
            let delta_3 = dial_3.to_f32().sub(dial.previous().to_f32());
            //let delta = delta_3.mul(delta_3).sum().sqrt();
            //let scale_factor = (delta * (now - prev_dial_time).as_millis() as f32).sqrt() / 100.00;
            let scale_factor = 1.0;
            let mut jog = Point3::new(delta_3.x as f32 * scale_factor, delta_3.y as f32 * scale_factor, delta_3.z as f32 * scale_factor);
            // todo: min step distance to jog.
            if is_absolute {
                jog = jog.add(*cnc_position.current());
            }

            gcode_buffer.push_back(format!("G0 {jog}"));
            dial.update();
        }

        if let Some(code) = gcode_buffer.pop_front() {
            gcode_processing.push_back(code.clone());
            // TODO, parse the gcode before send. Update current feed and current
            // position.
            cnc_tx.send(code.clone()).await.expect("unable to send gcode.");
            let code_parts:Vec<_> = gcode::parse(&code).collect();
            for command in code_parts {
                match (command.mnemonic(), command.major_number(), command.minor_number()) {
                    (Mnemonic::General, 91, _) => { is_absolute = true; },
                    (Mnemonic::General, 90, _) => { is_absolute = false; },
                    (Mnemonic::General, 0, _)|(Mnemonic::General, 1, _) => {
                        let cnc_position = cnc_position.current_mut();
                        let p_default = if is_absolute {
                            Point3::<f32>::default()
                        } else {
                            cnc_position.clone()
                        };
                        cnc_position.x = command.value_for('X').unwrap_or(p_default.x);
                        cnc_position.y = command.value_for('Y').unwrap_or(p_default.y);
                        cnc_position.z = command.value_for('Z').unwrap_or(p_default.z);
                    },
                    _ => { warn!("unknown gcode command: {}", command); },
                }
            }
        }
        if cnc_position.update_check() {
            let p = cnc_position.current().clone();
            xbee_tx.send(format!("P: {p}\n")).await.unwrap();
        }

        yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}


#[tokio::main(flavor="current_thread")]
async fn main() {
    env::set_var("RUST_LOG", "info");
    colog::init();
    let config: Config = Config::builder()
        .set_default("XBEE_PORT", "/dev/ttyAMA0").unwrap()
        .set_default("XBEE_BAUD", "9600").unwrap()
        .set_default("CNC_PORT", "/dev/ttyUSB0").unwrap()
        .set_default("CNC_BAUD", "9600").unwrap()
        .add_source(
            config::Environment::with_prefix("CNC").try_parsing(true),
        )
        .build()
        .unwrap();
    //let port_baud: u32 = config.get_int("XBEE_BAUD").expect("Unable to find baud rate for xbee") as u32;
    let (xbee_config_tx, xbee_config_rx) = mpsc::channel::<SerialPortInfo>(2);
    let (cnc_config_tx, cnc_config_rx) = mpsc::channel::<SerialPortInfo>(2);
    let (xbee_data_tx, xbee_data_rx) = mpsc::channel::<String>(32);
    let (xbee_events_tx, xbee_events_rx) = broadcast::channel::<RemoteEvent>(32);
    let (cnc_data_tx, cnc_data_rx) = mpsc::channel::<String>(32);
    let (cnc_events_tx, cnc_events_rx) = broadcast::channel::<CncEvent>(32);

    port_info_from_config("XBEE", &config, &xbee_config_tx).await;
    port_info_from_config("CNC", &config, &cnc_config_tx).await;

    let local = task::LocalSet::new();
    local.run_until(async move {
        let xbee_io = task::spawn_local(uart_read_write(xbee_config_rx, xbee_data_rx, xbee_events_tx));
        //let cnc_io = task::spawn_local(uart_read_write(cnc_config_rx, cnc_data_rx, cnc_events_tx));
        let cnc_io = task::spawn_local(fake_cnc_port(cnc_config_rx, cnc_data_rx, cnc_events_tx));
        let brain_loop = task::spawn_local(event_brain_loop(xbee_events_rx, xbee_data_tx, cnc_events_rx, cnc_data_tx));
        brain_loop.await.unwrap();
        xbee_io.await.unwrap();
        cnc_io.await.unwrap();
    }).await;

    //for dev in nusb::list_devices().unwrap() {
        //if let Some(product) = dev.product_string() {
            //let lower = product.to_lowercase();
            //let mut cont = false;
            //for black_list_item in ["keyboard", "mouse", "camera", "webcam", "gaming", "host controller", "mystic light", "usb2.0 hub", "otg controller"] {
                //if lower.contains(black_list_item) {
                    //cont = true;
                    //break;
                //}
            //}
            //if cont {
                //continue;
            //}
        //}
        //if let Some(man) = dev.manufacturer_string() {
            //let lower = man.to_lowercase();
            //for white_list_item in ["arduino"] {
                //if lower.contains(white_list_item) {
                    //println!("{:#?}", dev);
                //}
            //}
        //}
    //}
    //println!("done");
}
