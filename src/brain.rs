use std::{collections::VecDeque, fs::read_to_string, path::Path, time::Duration};
use gcode::Mnemonic;
use log::{info, warn};
use crate::state::{AppMode, CncEvent, DebounceDiffTracker, DelayUpdates, DiffTracker, Point3, RemoteEvent, TrackCurrentPrevious};
use tokio::{sync::{broadcast, mpsc::Sender}, task::yield_now};

pub async fn event_brain_loop(mut xbee_events: broadcast::Receiver<RemoteEvent>, xbee_tx: Sender<String>, mut cnc_events: broadcast::Receiver<CncEvent>, cnc_tx: Sender<String>) {
    let mut mode = AppMode::Jog;
    let mut is_absolute = false;
    let mut cnc_has_communicted = true;
    let mut dial: DiffTracker<Point3<i64>> = DiffTracker::new(Default::default());
    let mut cnc_position: DebounceDiffTracker<Point3<f32>> = DebounceDiffTracker::new(Point3::<f32>::default(), Duration::from_millis(100));
    //let mut current_feed_rate = 0;
    let mut gcode_buffer = VecDeque::<String>::new();
    let mut gcode_processing = VecDeque::<String>::new();
    let cnc_buffer_size = 5;

    let celing_time = 0.1;
    let max_feed_rate = 300.0;
    let scale = (0.1, max_feed_rate*celing_time);
    info!("max move per jog: {}", scale.1);

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

            let mut jog = dial.current().to_f32().sub(dial.previous().to_f32());
            jog = jog.apply(|v| v * scale.0);
            jog = jog.apply(|v| v.clamp(-scale.1, scale.1));

            // todo: min step distance to jog.
            if is_absolute {
                jog = jog.add(*cnc_position.current());
            }

            info!("jog {}", jog);

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
