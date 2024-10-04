use std::{env, io::{self, Write}, str::from_utf8, time::{Duration, Instant}};
//use config::Config;
use serial::SerialPort;
use tokio::{sync::mpsc::{self, Receiver, Sender}, task::{self, yield_now}};

#[derive(Debug)]
struct XYZ_F32 {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug)]
enum RemoteStateType {
    DialXYZEvent(XYZ_F32),
}

async fn write_loop(ch: Sender<String>) {
    loop {
        println!("sending a message");
        ch.send("This is a message\n".to_string()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        yield_now().await;
    }
}

async fn uart_read<T: SerialPort>(mut port: T, mut write_channel: Receiver<String>) {
    let mut buf: Vec<u8> = (0..255).collect();
    let mut read_buf: Vec<u8> = vec![];
    loop {
        let read = port.read(&mut buf[..]).unwrap();
        for i in 0..read {
            read_buf.push(buf[i])
        }
        let nl = read_buf.iter().position(|&b| b == b'\n');
        if let Some(nl_index) = nl {
             println!("{}", from_utf8(&read_buf[0..nl_index]).unwrap());
             read_buf.drain(0..=nl_index);
        }

        //if !write_channel.is_empty() {
            //while let Some(message) = write_channel.recv().await {
                //port.write(message.as_bytes()).unwrap();
            //}
            //port.flush().expect("flush");
        //}
        yield_now().await;
    }
    //Ok(())
}

#[tokio::main(flavor="current_thread")]
async fn main() {
    env::set_var("RUST_LOG", "info");
    colog::init();
    //let config = Config::builder()
        //.add_source(
            //config::Environment::with_prefix("CNC").try_parsing(true),
        //)
        //.build()
        //.unwrap();

    println!("setting up xbee");
    let mut port = serial::open("/dev/ttyAMA0").unwrap();
    let _ = port.reconfigure(&|settings| {
        settings.set_baud_rate(serial::Baud9600).expect("set baud rate");
        settings.set_char_size(serial::Bits8);
        settings.set_parity(serial::ParityNone);
        settings.set_stop_bits(serial::Stop1);
        settings.set_flow_control(serial::FlowNone);
        Ok(())
    }).expect("success");
    port.set_timeout(Duration::from_millis(60_000)).expect("set port timeout");
    port.flush().unwrap();

    let (tx, rx) = mpsc::channel(32);
    let local = task::LocalSet::new();
    local.run_until(async move {
        let write = task::spawn_local(write_loop(tx));
        let reads = task::spawn_local(uart_read(port, rx));
        write.await.unwrap();
        reads.await.unwrap();
    }).await;

    println!("fin xbee.");


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
