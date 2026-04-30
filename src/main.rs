use std::{borrow::Borrow, net::Ipv4Addr};

use http_rs::{
    bio::{Reader, Writer},
    http11,
};

use crate::{plugin::Plugin, tls::listener::TlsListener};

mod plugin;
mod tls;

fn main() {
    let listener =
        TlsListener::bind((Ipv4Addr::LOCALHOST, 8443)).expect("Failed to set up TLS listener");

    println!("Set up listener");

    listener
        .set_key_pair(c"./cert.pem", c"./key.pem")
        .expect("Failed to set up key pair");

    println!("Set up key pair");

    let mut plugin =
        unsafe { Plugin::load_plugin(c"./test_plugin.so").expect("Failed to load test plugin") };

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

        let alpn = stream.get_selected_alpn().to_owned();
        println!("{}", String::from_utf8_lossy(&alpn));

        let (read, write) = match stream.split() {
            Ok(v) => v,
            Err(e) => {
                println!("Failed to split tls stream: {e}");
                continue;
            }
        };

        let (req, writer) = match String::from_utf8_lossy(&alpn).borrow() {
            "http/1.1" => {
                let reader = Reader::new(read);
                let writer = Writer::new(write);

                let req = http11::parse_request(reader).expect("Failed to parse http1.1 request");
                let writer = http11::new_response_writer(writer);

                (req, writer)
            }
            // "h2" => {
            //     let reader = reader_from_read(read);
            //     let writer = writer_from_write(write);
            //     let req = unsafe { http_parse_http2_request(reader, writer) };
            //     let writer = unsafe { http_http2_response_writer(3, writer) };
            //
            //     (req, writer)
            // }
            t => {
                println!("Unsupported protocol: {t}");
                continue;
            }
        };

        plugin.handle_request(req, writer);

        println!("served request");
    }
}
