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

/// State consists of connection information and list items.
pub struct State {
    pub connections: HashMap<SocketAddr, UnboundedSender<Message>>,
    pub password: Option<String>,
    file: File,
    mmap: Option<MmapMut>
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
        let mut mmap = State::map(&file, len)?;
        if !exists { mmap.copy_from_slice(JSON_INIT.as_ref()); }

        Ok(State {
            connections: HashMap::new(),
            file: file,
            mmap: Some(mmap),
            password: password
        })
    }

    /// Set JSON state.
    pub fn set_json(&mut self, json: &str) -> std::io::Result<()> {
        let len = json.len();
        drop(self.mmap.take());
        self.file.set_len(len as u64)?;
        let mut mmap = State::map(&self.file, len)?;
        mmap.copy_from_slice(json.as_ref());
        std::mem::replace(&mut self.mmap, Some(mmap));
        Ok(())
    }

    /// Get JSON state.
    pub fn get_json(&self) -> String {
        String::from_utf8_lossy(self.mmap.as_ref().unwrap()).to_string()
    }

    fn map(file: &File, len: usize) -> std::io::Result<MmapMut> {
        Ok(unsafe { MmapOptions::new().len(len).map_mut(file)? })
    }
}
