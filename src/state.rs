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

fn align_len(len: usize) -> usize {
    let n = 4096;
    n * (1 + (std::cmp::max(len, 1)-1) / n)
}

fn map(file: &File, len: usize) -> std::io::Result<MmapMut> {
    Ok(unsafe { MmapOptions::new().len(len).map_mut(file)? })
}

/// State consists of connection information and list items.
pub struct State {
    pub connections: HashMap<SocketAddr, UnboundedSender<Message>>,
    pub password: Option<String>,
    file: File,
    mmap: Option<MmapMut>,
    len: usize,
    size: usize // len padded to page boundary
}

/// Initial state items.
static JSON_INIT: &'static str = r#"[{"html":"Apples","checked":false},{"html":"Bacon","checked":false},{"html":"Coke","checked":false}]"#;

impl State {
    /// Parameter json_path specifies memory-mapped file for storing the state.
    /// Clients must supply the password as a subprotocol to access the server.
    pub fn new(json_path: &str, password: Option<String>) -> std::io::Result<State> {
        let exists = Path::new(json_path).exists();

        let file = OpenOptions::new().read(true).write(true).create(true).open(json_path)?;
        file.try_lock_exclusive()?;

        let (mmap, len, size) = if exists {
            let file_len = fs::metadata(json_path)?.len() as usize;
            let size = align_len(file_len);
            if file_len != size { file.set_len(size as u64)?; }
            let mut mmap = map(&file, size)?;
            let len = mmap.iter().position(|&x| x == 0).unwrap_or(size);
            (mmap, len, size)
        } else {
            let len = JSON_INIT.len();
            let size = align_len(len);
            file.set_len(size as u64)?;
            let mut mmap = map(&file, size)?;
            mmap[..len].copy_from_slice(JSON_INIT.as_ref());
            (mmap, len, size)
        };

        Ok(State {
            connections: HashMap::new(),
            password: password,
            file: file,
            mmap: Some(mmap),
            len: len,
            size: size
        })
    }

    /// Set JSON state.
    pub fn set_json(&mut self, json: &str) -> std::io::Result<()> {
        let len = json.len();
        let size = align_len(len);

        if size != self.size {
            drop(self.mmap.take());
            self.file.set_len(size as u64)?;
            let mmap = map(&self.file, size)?;
            std::mem::replace(&mut self.mmap, Some(mmap));
            self.size = size;
        }

        let buf = self.mmap.as_mut().unwrap();
        buf[..len].copy_from_slice(json.as_ref());
        if len < size { buf[len] = 0; }
        self.len = len;

        Ok(())
    }

    /// Get JSON state.
    pub fn get_json(&self) -> String {
        let buf = self.mmap.as_ref().unwrap();
        String::from_utf8_lossy(&buf[..self.len]).to_string()
    }
}
