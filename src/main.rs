mod state;
mod state_parser;
mod port_io;
mod brain;

use std::{env, time::Duration};
use config::Config;
use brain::*;
use log::warn;
use port_io::*;
use state::*;
use tokio::{io::AsyncBufReadExt, sync::{broadcast, mpsc::{self}}, task::{self}, time::sleep};

#[tokio::main(flavor="current_thread")]
async fn main() {
    env::set_var("RUST_LOG", "info");
    colog::init();
    let config: Config = Config::builder()
        .set_default("XBEE_PORT", "/dev/ttyAMA0").unwrap()
        .set_default("XBEE_BAUD", "9600").unwrap()
        .set_default("CNC_PORT", "/dev/ttyUSB0").unwrap()
        .set_default("CNC_BAUD", "115200").unwrap()
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

    for dev in nusb::list_devices().unwrap() {
        if let Some(product) = dev.product_string() {
            let lower = product.to_lowercase();
            let mut cont = false;
            for black_list_item in ["keyboard", "mouse", "camera", "webcam", "gaming", "host controller", "mystic light", "usb2.0 hub", "otg controller"] {
                if lower.contains(black_list_item) {
                    cont = true;
                    break;
                }
            }
            if cont {
                continue;
            }
        }
        //println!("{:#?}", dev);
        //if let Some(man) = dev.manufacturer_string() {
            //let lower = man.to_lowercase();
            //for white_list_item in ["arduino"] {
                //if lower.contains(white_list_item) {
                    //println!("{:#?}", dev);
                //}
            //}
        //}
    }

    let local = task::LocalSet::new();
    local.run_until(async move {
        let xbee_io = task::spawn_local(uart_read_write(xbee_config_rx, xbee_data_rx, xbee_events_tx));
        let cnc_io = task::spawn_local(uart_read_write(cnc_config_rx, cnc_data_rx, cnc_events_tx));
        //let cnc_io = task::spawn_local(fake_cnc_port(cnc_config_rx, cnc_data_rx, cnc_events_tx));
        //let cnc_data_tx_startup = cnc_data_tx.clone();
        let readline_input = cnc_data_tx.clone();
        let brain_loop = task::spawn_local(event_brain_loop(xbee_events_rx, xbee_data_tx, cnc_events_rx, cnc_data_tx));

        //task::spawn_local(async move {
            //sleep(Duration::from_secs_f32(0.5)).await;
            //cnc_data_tx_startup.send("G92 X0 Y0 Z0".to_owned()).await.unwrap();
        //});
        task::spawn_local(async move {
            sleep(Duration::from_secs_f32(0.5)).await;
            let stdin = tokio::io::stdin();
            let reader = tokio::io::BufReader::new(stdin);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                readline_input.send(line).await.unwrap();
            }
            warn!("fin console input");
        });

        brain_loop.await.unwrap();
        xbee_io.await.unwrap();
        cnc_io.await.unwrap();
    }).await;
}
