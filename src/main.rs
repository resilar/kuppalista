extern crate bytes;
extern crate clap;
extern crate fs2;
extern crate futures;
extern crate httparse;
extern crate hyper;
extern crate memmap;
extern crate native_tls;
extern crate rpassword;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_tls;
extern crate tokio_tungstenite;
extern crate tungstenite;

mod args;
mod rewind_stream;
mod state;

use args::Args;
use rewind_stream::RewindStream;
use state::State;

use futures::stream::Stream;
use futures::{Async, Future, Sink};
use hyper::header;
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Method, Request, Response, StatusCode};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::accept_hdr_async;
use tungstenite::protocol::Message;

use std::cell::RefCell;
use std::convert::From;
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::process;
use std::rc::Rc;

/// The server is a single-threaded, so Rc and RefCell are sufficient.
type StateRef = Rc<RefCell<State>>;

static INDEX: &'static [u8] = include_bytes!("index.html");
static NOTFOUND: &'static [u8] = b"404";

#[derive(Debug)]
pub enum AppError {
    ArgsError(args::ArgsError),
    IoError(io::Error),
    Other(String),
}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        AppError::IoError(error)
    }
}

impl From<args::ArgsError> for AppError {
    fn from(error: args::ArgsError) -> Self {
        use args::ArgsError::*;
        match error {
            IoError(err) => AppError::IoError(err),
            _ => AppError::ArgsError(error),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AppError::*;
        match *self {
            IoError(ref err) => write!(f, "IO error: {}", err),
            ArgsError(ref err) => write!(f, "error: {}", err),
            Other(ref wtf) => f.write_str(&wtf),
        }
    }
}

fn handle_http<I>(io: I) -> impl Future<Item = RewindStream<I>, Error = ()>
where
    I: AsyncRead + AsyncWrite + 'static,
{
    let io = RewindStream::new(io);
    let service = service_fn(|req: Request<Body>| {
        match (req.method(), req.uri().path()) {
            (&Method::GET, "/") => {
                if let Some(value) = req.headers().get("upgrade") {
                    if value == "websocket" {
                        // Break out of the service by returning an error.
                        // tungstenite will handle WebSocket communication.
                        return Err("WebSocket");
                    }
                }
                let mut response = Response::builder();
                response
                    .header(header::CONTENT_TYPE, "text/html")
                    .status(StatusCode::OK);
                response.body(Body::from(INDEX)).or(Err("error"))
            }
            _ => {
                let mut response = Response::builder();
                response
                    .header(header::CONTENT_TYPE, "text/text")
                    .status(StatusCode::NOT_FOUND);
                response.body(Body::from(NOTFOUND)).or(Err("error"))
            }
        }
    });

    let mut conn = Some(Http::new().http1_only(true).serve_connection(io, service));
    futures::future::poll_fn(move || {
        match conn.as_mut().unwrap().poll_without_shutdown() {
            Ok(Async::Ready(_)) => Err(()), // connection closed
            Err(ref e) if e.is_user() && e.to_string().contains("WebSocket") => {
                let mut io = conn.take().unwrap().into_parts().io;
                io.rewind(); // Replay the upgrade request for tokio-tungstenite
                io.pass_through();
                Ok(Async::Ready(io))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => {
                eprintln!("HTTP error: {}", e);
                Err(())
            }
        }
    })
}

fn handle_websocket<I>(
    io: I,
    addr: SocketAddr,
    state: StateRef,
) -> impl Future<Item = (), Error = ()>
where
    I: AsyncRead + AsyncWrite + 'static,
{
    use tungstenite::handshake::server::{Callback, ErrorResponse, Request};

    /// Accept connection if WebSocket subprotocol matches self.0 (password).
    struct ProtocolChecker(Option<String>);

    impl Callback for ProtocolChecker {
        fn on_request(self, req: &Request) -> Result<Option<Vec<(String, String)>>, ErrorResponse> {
            let mut protocols = req
                .headers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("Sec-Websocket-Protocol"))
                .filter_map(|(_, v)| std::str::from_utf8(v).ok())
                .flat_map(|v| v.split(',').map(|v| v.trim()));
            if let Some(protocol) = self.0 {
                if let Some(p) = protocols.find(move |&v| v == protocol) {
                    Ok(Some(vec![(
                        "Sec-Websocket-Protocol".to_string(),
                        p.to_string(),
                    )]))
                } else {
                    Err(ErrorResponse {
                        error_code: StatusCode::FORBIDDEN,
                        headers: None,
                        body: Some("Bad WebSocket subprotocol".to_string()),
                    })
                }
            } else if let Some(p) = protocols.next() {
                // Protocol not required but the client provided one.
                // Let's just accept it.
                Ok(Some(vec![(
                    "Sec-Websocket-Protocol".to_string(),
                    p.to_string(),
                )]))
            } else {
                Ok(None)
            }
        }
    }

    let password_checker = ProtocolChecker(state.borrow().password.clone());
    accept_hdr_async(io, password_checker)
        .and_then(move |ws_stream| {
            println!("New WebSocket connection: {}", addr);
            let (tx, rx) = futures::sync::mpsc::unbounded();
            state.borrow_mut().connections.insert(addr, tx.clone());
            tx.unbounded_send(Message::Text(state.borrow().get_json()))
                .unwrap();

            let inner_state = state.clone();
            let (sink, stream) = ws_stream.split();

            let ws_reader = stream
                .for_each(move |message: Message| {
                    if cfg!(feature = "verbose") {
                        println!("Received a message from {}: {}", addr, message);
                    }

                    // Send the state update to all clients except the sender itself
                    if message.is_text() && message.to_string().chars().next() == Some('[') {
                        let mut state = state.borrow_mut();
                        state.set_json(&message.to_string()).unwrap();
                        state
                            .connections
                            .iter_mut()
                            .filter_map(|(k, tx)| if k != &addr { Some(tx) } else { None })
                            .for_each(move |tx| tx.unbounded_send(message.clone()).unwrap());
                    }

                    Ok(())
                })
                .map_err(|_| ());

            let ws_writer = rx.forward(sink.sink_map_err(|_| ())).map(|_| ());

            let connection = ws_reader.select(ws_writer);
            connection.then(move |_| {
                inner_state.borrow_mut().connections.remove(&addr);
                println!("Connection {} closed.", addr);
                Ok(())
            })
        })
        .or_else(|e| Err(eprintln!("{}", e)))
}

fn try_main() -> Result<(), AppError> {
    let args = Args::parse()?;
    let state = Rc::new(RefCell::new(State::new(
        "state.json.bin",
        args.password.clone(),
    )?));

    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let listener = TcpListener::bind(&args.addr, &handle).map_err(AppError::IoError)?;
    println!(
        "Listening {}/WebSocket on: {}",
        if args.pkcs12.is_some() {
            "HTTPS"
        } else {
            "HTTP"
        },
        args.addr
    );

    if let Some(pkcs12) = args.pkcs12 {
        let acceptor = native_tls::TlsAcceptor::new(pkcs12).unwrap();
        let acceptor = tokio_tls::TlsAcceptor::from(acceptor);
        let tls_handler = |(tcp_stream, addr)| {
            let state = state.clone();
            let handle = handle.clone();
            acceptor
                .accept(tcp_stream)
                .and_then(move |tls_stream| {
                    handle.spawn(
                        handle_http(tls_stream)
                            .and_then(move |io| handle_websocket(io, addr, state)),
                    );
                    Ok(())
                })
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
        };
        core.run(listener.incoming().for_each(tls_handler))
            .map_err(AppError::IoError)
    } else {
        let tcp_handler = |(tcp_stream, addr)| {
            let state = state.clone();
            handle.spawn(
                handle_http(tcp_stream).and_then(move |io| handle_websocket(io, addr, state)),
            );
            Ok(())
        };
        core.run(listener.incoming().for_each(tcp_handler))
            .map_err(AppError::IoError)
    }
}

fn main() {
    match try_main() {
        Ok(_) => process::exit(0),
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1)
        }
    }
}
