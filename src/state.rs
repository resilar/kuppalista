use fs2::FileExt;
use futures::sync::mpsc::UnboundedSender;
use memmap::{MmapMut, MmapOptions};
use tungstenite::protocol::Message;

use std;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;

struct StateBuffer {
    mmap: MmapMut,
    size: usize // Padded size
}

impl StateBuffer {
    pub fn map(file: &File, len: usize) -> std::io::Result<StateBuffer> {
        let size = 4096 * (1 + (std::cmp::max(len, 1)-1) / 4096); // Align to page boundary
        let mmap = unsafe { MmapOptions::new().len(size as usize).map_mut(file)? };
        Ok(StateBuffer { mmap, size })
    }
}

/// State consists of connection information and list items.
pub struct State {
    pub connections: HashMap<SocketAddr, UnboundedSender<Message>>,
    pub password: Option<String>,
    file: File,
    buffer: StateBuffer,
    json_len: usize
}

/// Initial state items.
static JSON_INIT: &'static str = r#"[{"html":"Apples","checked":false},{"html":"Bacon","checked":false},{"html":"Coke","checked":false}]"#;

impl State {
    /// Parameter json_path specifies memory-mapped file for storing the state.
    /// Clients must supply the password as a subprotocol to access the server.
    pub fn new(json_path: &str, password: Option<String>) -> std::io::Result<State> {
        let exists = Path::new(json_path).exists();
        let len = if exists { fs::metadata(json_path)?.len() as usize } else { JSON_INIT.len() };

        let file = OpenOptions::new().read(true).write(true).create(true).open(json_path)?;
        file.try_lock_exclusive()?;

        if !exists { file.set_len(len as u64)?; }
        let mut buffer = StateBuffer::map(&file, len)?;
        if !exists { buffer.mmap[..len as usize].copy_from_slice(JSON_INIT.as_ref()); }

        Ok(State {
            connections: HashMap::new(),
            file: file,
            buffer: buffer,
            json_len: len,
            password: password
        })
    }

    /// Set JSON state.
    pub fn set_json(&mut self, json: &str) -> std::io::Result<()> {
        let len = json.len();
        self.file.set_len(len as u64)?;
        if len > self.buffer.size {
            std::mem::replace(&mut self.buffer, StateBuffer::map(&self.file, len)?);
        }
        self.buffer.mmap[..len as usize].copy_from_slice(json.as_ref());
        self.json_len = json.len();
        Ok(())
    }

    /// Get JSON state.
    pub fn get_json(&self) -> String {
        String::from_utf8_lossy(&self.buffer.mmap[..self.json_len]).to_string()
    }
}
