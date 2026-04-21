use std::{
    borrow::Borrow,
    io::{Read, Write},
    net::Ipv4Addr,
    time::Instant,
};

use crate::{
    http::{
        http2::{
            self,
            frame::{FrameHeader, go_away, ping},
            hpack::tables::HeaderTable,
        },
        http11,
        response::{ResponseWriter, StatusCode},
    },
    tls::listener::TlsListener,
};

mod ffi;
mod http;
mod tls;

fn main() {
    let listener =
        TlsListener::bind((Ipv4Addr::LOCALHOST, 8443)).expect("Failed to set up TLS listener");

    println!("Set up listener");

    listener
        .set_key_pair(c"./cert.pem", c"./key.pem")
        .expect("Failed to set up key pair");

    println!("Set up key pair");

    loop {
        let res = listener.accept();

        let (stream, _addr) = match res {
            Ok(d) => {
                println!("Got new connection on {:?}", d.1);
                d
            }
            Err(e) => {
                println!("Failed to get connection: {e}");
                continue;
            }
        };

        let alpn = stream.get_selected_alpn();
        println!("{}", String::from_utf8_lossy(alpn));

        let mut table = HeaderTable::new(65536);
        let (mut req, mut writer): (_, Box<dyn ResponseWriter>) =
            match String::from_utf8_lossy(alpn).borrow() {
                "http/1.1" => {
                    let req = match http11::parse_request(&stream) {
                        Ok(r) => r,
                        Err(e) => {
                            println!("Failed to parse request: {e}");
                            continue;
                        }
                    };

                    let writer = Box::new(http11::ResponseWriter::new(&stream));

                    (req, writer)
                }
                "h2" => {
                    let (id, req) = match http2::parse_request(&stream, &mut table) {
                        Ok(r) => r,
                        Err(e) => {
                            println!("Failed to parse request: {e}");
                            continue;
                        }
                    };

                    let writer = Box::new(http2::ResponseWriter::new(&stream, 3));

                    (req, writer)
                }
                t => {
                    println!("Unsupported protocol: {t}");
                    continue;
                }
            };

        let mut v = Vec::new();
        _ = req.body.read_to_end(&mut v);

        println!("{}", String::from_utf8_lossy(&v));

        _ = writer.write_status(StatusCode::OK);

        v.extend(b"Goodbye\n");

        match writer.write_all(&v) {
            Ok(()) => {}
            Err(e) => {
                println!("Failed to write request: {e}");
            }
        }
        println!("Reading next frame");
        let start = Instant::now();
        _ = ping::write_frame(0xfa041bf858403cd7, false, &mut &stream);
        println!("Sent ping: {}", 0xfa041bf858403cd7u64);
        let h = FrameHeader::read_header(&mut &stream).expect("Should get another frame");
        let finish = start.elapsed();
        println!("Read {:?} in {finish:?}", h);
        println!("{:?}", ping::read_frame(h, &mut &stream));
        println!("{:?}", go_away::write_frame(3, 0x0, &[], &mut &stream));

        println!("served request");
    }
}
