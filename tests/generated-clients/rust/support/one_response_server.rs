use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

pub fn spawn_one_response_server(status: &str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
    let addr = listener
        .local_addr()
        .expect("test server address should be readable");
    let status = status.to_string();
    thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("test server should accept one request");
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 120\r\nX-Request-Id: typed-response-test\r\nX-Trace-Id: typed-error-test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("test response should be written");
    });
    format!("http://{addr}")
}
