use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

pub fn spawn_incomplete_success_body_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
    let addr = listener
        .local_addr()
        .expect("test server address should be readable");
    thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("test server should accept one request");
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nX-Request-Id: typed-response-test\r\nContent-Length: 999999\r\nConnection: close\r\n\r\n";
        stream
            .write_all(response.as_bytes())
            .expect("test response headers should be written");
    });
    format!("http://{addr}")
}
