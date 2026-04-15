use std::{
    borrow::Borrow,
    io::{Read, Write},
    net::Ipv4Addr,
};

use crate::{
    http::{
        http2, http11,
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
                    let req = match http2::parse_request(&stream) {
                        Ok(r) => r,
                        Err(e) => {
                            println!("Failed to parse request: {e}");
                            continue;
                        }
                    };

                    let writer = Box::new(http2::ResponseWriter::new(&stream));

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
            Ok(_) => {}
            Err(e) => {
                println!("Failed to write request: {e}");
            }
        }

        println!("served request");
    }
}
