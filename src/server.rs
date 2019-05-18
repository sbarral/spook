use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::{Event, EVENT_PATH};

// Listen for SSE requests.
pub fn manage_connections(event_tx_list: Arc<Mutex<Vec<Sender<()>>>>, event: Event) {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], event.port))).expect(
        concat!(env!("CARGO_PKG_NAME"), ": error starting the server"),
    );

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let (event_tx, event_rx) = channel();
            event_tx_list.lock().unwrap().push(event_tx);
            let event_name = event.name.clone();
            thread::spawn(move || serve_events(event_rx, stream, event_name));
        }
    }
}

// Serve events via the specified subscriber stream.
pub fn serve_events(rx: Receiver<()>, mut stream: TcpStream, event_name: String) {
    // Read the request.
    let mut read_buffer = [0u8; 512];
    let mut buffer = Vec::new();
    let (method, path) = loop {
        // Read the request, or part thereof.
        match stream.read(&mut read_buffer) {
            Ok(0) | Err(_) => {
                // Connection closed or error.
                return;
            }
            Ok(n) => {
                // Succesfull read.
                buffer.extend_from_slice(&read_buffer[..n]);
            }
        }

        // Try to parse the request.
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&buffer) {
            Ok(_) => {
                // We are happy even with a partial parse as long as the method
                // and path are available.
                if let (Some(method), Some(path)) = (req.method, req.path) {
                    break (method, path);
                }
            }
            Err(_) => return,
        }
    };

    // The only supported request method for SSE is GET.
    if method != "GET" {
        let _ = stream.write(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n");
        return;
    }

    // Check the path.
    if path != EVENT_PATH {
        let _ = stream.write(b"HTTP/1.1 404 Not Found\r\n\r\n");
        return;
    }

    // Declare SSE capability and allow cross-origin access.
    let response = b"\
        HTTP/1.1 200 OK\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\
        Content-Type: text/event-stream\r\n\
        \r\n";
    if stream.write(response).is_err() {
        return;
    }

    // Make the stream non-blocking to be able to detect whether the
    // connection was closed by the client.
    stream.set_nonblocking(true).expect(concat!(
        env!("CARGO_PKG_NAME"),
        ": error setting up non-blocking TCP stream"
    ));

    // Serve events until the connection is closed.
    // Keep in mind that the client will often close
    // the request after the first event if the event
    // is used to trigger a page refresh, so try to eagerly
    // detect closed connections.
    loop {
        // Wait for the next update.
        rx.recv().unwrap();

        // Detect whether the connection was closed.
        match stream.read(&mut read_buffer) {
            Ok(0) => {
                // Connection closed.
                return;
            }
            Ok(_) => {}
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    // Something bad happened.
                    return;
                }
            }
        }

        // Send event.
        let event = format!("event: {}\r\ndata\r\n\r\n", event_name);
        let _ = stream.write(event.as_bytes()); // errors will be caught at next read() anyway
    }
}
