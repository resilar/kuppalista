use httparse;

use bytes::BytesMut;
use futures::Async;
use tokio_io::{AsyncRead, AsyncWrite};

use std;
use std::io::{Read, Write};

/// `RewindStream` buffers the last received HTTP request and allows rewinding
/// to it. This is used to implement WebSocket HTTP upgrades between hyper and
/// tokio-tungstenite. The basic idea is to repeat the upgrade request after
/// hyper receives it, so that tokio-tungstenite can also receive the upgrade
/// request and start handling WebSocket handshake.
pub struct RewindStream<S> {
    io: S,
    buf_in: BytesMut,
    buf_out: BytesMut,
    last_req: BytesMut,
    pass_through: bool
}

impl<S> RewindStream<S> {
    /// Wrap a stream.
    pub fn new(stream: S) -> RewindStream<S> {
        RewindStream {
            io: stream,
            buf_in: BytesMut::new(),
            buf_out: BytesMut::new(),
            last_req: BytesMut::new(),
            pass_through: false
        }
    }

    /// Rewind to the beginning of the last request.
    pub fn rewind(&mut self) {
        assert!(!self.pass_through, "cannot rewind in pass_through mode");
        self.buf_out.extend_from_slice(&self.last_req);
    }

    /// Pass through data from now on (disables rewinding and HTTP parsing).
    pub fn pass_through(&mut self) {
        assert!(!self.pass_through, "RewindStream::pass_through called twice");
        self.pass_through = true;
    }
}

impl<S: Read> Read for RewindStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.buf_out.is_empty() {
            if self.pass_through {
                return self.io.read(buf);
            }

            loop {
                if let Some(size) = {
                    let mut headers = [httparse::EMPTY_HEADER; 128];
                    let mut req = httparse::Request::new(&mut headers);
                    if let Ok(httparse::Status::Complete(size)) = req.parse(&self.buf_in) {
                        Some(size)
                    } else {
                        None
                    }
                } {
                    self.last_req = self.buf_in.split_to(size);
                    self.buf_out.extend_from_slice(&self.last_req);
                    break;
                }

                let mut tmp = [0u8; 4096];
                let len = self.io.read(&mut tmp)?;
                if len > 0 {
                    self.buf_in.extend_from_slice(&tmp[..len]);
                } else {
                    return Ok(0);
                }
            }
        }

        let len = std::cmp::min(buf.len(), self.buf_out.len());
        buf[..len].copy_from_slice(&self.buf_out.split_to(len));
        Ok(len)
    }
}

impl<S: Write> Write for RewindStream<S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.io.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.io.flush()
    }
}

impl<S: AsyncRead> AsyncRead for RewindStream<S> { }

impl<S: AsyncWrite> AsyncWrite for RewindStream<S> {
    fn shutdown(&mut self) -> Result<Async<()>, std::io::Error> {
        self.io.shutdown()
    }
}
