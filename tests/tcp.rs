use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};

use pima::{Config, Interpreter};

#[test]
fn pima_implements_http_over_native_tcp_primitives() {
    let probe = TcpListener::bind(("127.0.0.1", 0)).expect("ephemeral port should be available");
    let port = probe.local_addr().expect("listener has an address").port();
    drop(probe);

    let source = format!(
        r#"import "http_server_lib.pima" as :http

function :response (status reason headers body) {{
    new {{
        pub val :status status
        pub val :reason reason
        pub val :headers headers
        pub val :body body
    }}
}}

function :handle (request) {{
    response 200 "OK" (("X-Pima-Method" request.method)) request.target
}}

http.serve_once "127.0.0.1" {port} handle
"#
    );
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let server = thread::spawn(move || {
        let mut interpreter = Interpreter::new(Config {
            working_directory: Some(examples),
        });
        let outcome = interpreter.run_source("<tcp-http-test>", &source);
        (outcome.is_success(), format!("{:?}", outcome.diagnostics))
    });

    let mut stream = connect_with_retry(port);
    stream
        .write_all(b"GET /hello?name=pima HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("request should be written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("response should be readable");

    let (success, diagnostics) = server.join().expect("server thread should not panic");
    assert!(success, "{diagnostics}");
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("\r\nX-Pima-Method: GET\r\n"));
    assert!(response.contains("\r\nContent-Length: 16\r\n"));
    assert!(response.ends_with("/hello?name=pima"));
}

fn connect_with_retry(port: u16) -> TcpStream {
    for _ in 0..50 {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
            return stream;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("TCP listener did not start on port {port}");
}
