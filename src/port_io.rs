use std::{collections::VecDeque, path::Path, str::{from_utf8, FromStr}, time::{Duration, Instant}};
use config::Config;
use gcode::Mnemonic;
use log::{info, warn};
use tokio::{io::AsyncWriteExt, sync::{broadcast, mpsc::{Receiver, Sender}}, task::yield_now, time::sleep};
use tokio_serial::SerialPortBuilderExt;

use crate::state::{CncEvent, Point3};

#[derive(Debug, Clone)]
pub struct SerialPortInfo {
    pub path: String,
    pub baud: u32,
}

pub async fn uart_read_write<T>(mut port_rx: Receiver<SerialPortInfo>, mut write_channel: Receiver<String>, tx_remote_events: broadcast::Sender<T>)
    where 
        T: FromStr,
        T: Clone
{
    let mut buf: Vec<u8> = (0..255).collect();
    let mut read_buf: Vec<u8> = vec![];
    let mut port_info = None;
    loop {
        if port_info.is_none() {
            sleep(Duration::from_millis(200)).await;
            if let Some(next_port) = port_rx.recv().await {
                port_info = Some(next_port);
                continue;
            }
            else {
                continue;
            }
        }
        let pref = port_info.as_ref().unwrap();
        if !Path::new(&pref.path).exists() {
            warn!("Port does not exist: {}", pref.path);
            port_info = None;
            continue;
        }

        let mut port = tokio_serial::new(pref.path.clone(), pref.baud)
            .open_native_async().expect("Unable to open serial port.");
        loop {
            //info!("uart loop.");
            if let Ok(next_port) = port_rx.try_recv() {
                port_info = Some(next_port);
                let _ = port.shutdown().await;
                break;
            }

            let read_result = port.try_read(&mut buf[..]);
            if let Ok(read) = read_result {
                for i in 0..read {
                    read_buf.push(buf[i])
                }
                let nl = read_buf.iter().position(|&b| b == b'\n');
                if let Some(nl_index) = nl {
                    let line = from_utf8(&read_buf[0..nl_index]);
                    if let Ok(line) = line {
                        let event = line.parse::<T>();
                        if let Ok(event) = event {
                            let trysend = tx_remote_events.send(event);
                            if trysend.is_err() {
                                warn!("Failed to send parsed event.")
                            }
                        }
                    }
                    read_buf.drain(0..=nl_index);
                }
            }

            while let Ok(message) = write_channel.try_recv() {
                port.write(&message.as_bytes()).await.unwrap();
                if let Some(lchar) = message.chars().last() {
                    if lchar != '\n' {
                        port.write(b"\n").await.unwrap();
                    }
                }
                port.flush().await.unwrap();
            }
            yield_now().await;
        }
    }
}

pub async fn port_info_from_config(prefix: &str, config: &Config, ch: &Sender<SerialPortInfo>) {
    let path = format!("{prefix}_PORT");
    let baud = format!("{prefix}_BAUD");
    let port_config = SerialPortInfo {
        path: config.get_string(&path).unwrap(),
        baud: config.get_int(&baud).unwrap() as u32,
    };
    ch.send(port_config).await.expect("unable to send message to serial port config channel");
}


#[allow(unused_assignments)]
pub async fn fake_cnc_port(_port_rx: Receiver<SerialPortInfo>, mut write_channel: Receiver<String>, tx_remote_events: broadcast::Sender<CncEvent>) {
    let mut gcode_processing = VecDeque::<String>::new();
    gcode_processing.reserve(10);
    let mut is_absolute = true;
    let mut cnc_position: Point3<f32> = Default::default();
    let mut target_position: Point3<f32> = Default::default();
    let mut busy_until: Option<Instant> = None;
    let max_feed_rate = 300 /* mm/sec */ as f32;
    loop {
        tokio::time::sleep(Duration::from_millis(30)).await;
        yield_now().await;
        //info!("cnc loop.");
        if gcode_processing.len() < 5 {
            if let Ok(line) = write_channel.try_recv() {
                gcode_processing.push_back(line);
                if gcode_processing.len() < 5 {
                    let _ = tx_remote_events.send(CncEvent::Ok);
                }
                else {
                    // busy processing message?
                    //let _ = tx_remote_events.send(CncEvent::Ok);
                }
            }
        }
        if busy_until.is_some() && Instant::now() <= busy_until.unwrap() {
            if gcode_processing.len() == 5 {
                tokio::time::sleep_until(busy_until.unwrap().into()).await;
            }
            continue;
        } else if busy_until.is_some() {
            // finished moving machine.
            busy_until = None; 
            cnc_position = target_position.clone();
        }
        if let Some(code) = gcode_processing.pop_front() {
            if gcode_processing.len() == 5 {
                let _ = tx_remote_events.send(CncEvent::Ok);
            }
            info!("running command: {}", code);
            busy_until = Some(Instant::now());
            let code_parts:Vec<_> = gcode::parse(&code).collect();
            for command in code_parts {
                match (command.mnemonic(), command.major_number(), command.minor_number()) {
                    (Mnemonic::General, 91, _) => { is_absolute = true; },
                    (Mnemonic::General, 90, _) => { is_absolute = false; },
                    (Mnemonic::General, 0, _)|(Mnemonic::General, 1, _) => {
                        let p_default = if is_absolute {
                            Point3::<f32>::default()
                        } else {
                            target_position.clone()
                        };
                        let mut next_target = target_position.clone();

                        next_target.x = command.value_for('X').unwrap_or(p_default.x);
                        next_target.y = command.value_for('Y').unwrap_or(p_default.y);
                        next_target.z = command.value_for('Z').unwrap_or(p_default.z);
                        let d = next_target.sub(target_position);
                        let distance = d.mul(d).sum().sqrt();
                        let travel_time = Duration::from_secs_f32(distance / max_feed_rate);
                        busy_until = busy_until.unwrap().checked_add(travel_time);
                        target_position = next_target;
                    },
                    _ => { warn!("unknown gcode command: {}", command); },
                }
            }
        }
    }
}
