//! TFTP (RFC 1350) with the block size option (RFC 2348). A modem is the client: it opens on the
//! server's port in `octet` mode and writes [CM-OSSI §D.5.1].

use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const WRQ: u16 = 2;
const DATA: u16 = 3;
const ACK: u16 = 4;
const ERROR: u16 = 5;
const OACK: u16 = 6;

/// [CM-OSSI §D.5.1]: block size on IPv4.
pub const BLOCK_SIZE_IPV4: usize = 1448;
/// Retransmissions before giving up. The spec doesn't require automatic retries; a lost upload is the
/// application's to rerun [PNM-HFC §8.4.3].
const RETRIES: usize = 5;
const TIMEOUT: Duration = Duration::from_millis(300);
/// A file bigger than this is refused: a modem's store holds at least 64 KB per file, and the largest
/// real files are a few hundred KB.
const MAX_FILE: usize = 16 * 1024 * 1024;

fn be16(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.first()?, *bytes.get(1)?]))
}

/// Uploads `data` to a TFTP server, as a modem does.
pub fn upload(
    server: SocketAddr,
    filename: &str,
    data: &[u8],
    block_size: usize,
) -> io::Result<()> {
    let socket = UdpSocket::bind(("127.0.0.1", 0))?;
    socket.set_read_timeout(Some(TIMEOUT))?;
    let mut request = Vec::new();
    request.extend_from_slice(&WRQ.to_be_bytes());
    request.extend_from_slice(filename.as_bytes());
    request.push(0);
    request.extend_from_slice(b"octet\0blksize\0");
    request.extend_from_slice(block_size.to_string().as_bytes());
    request.push(0);

    let mut buffer = [0u8; 2048];
    let mut negotiated = 512;
    let mut peer = None;
    for attempt in 0..RETRIES {
        socket.send_to(&request, server)?;
        if let Ok((n, from)) = socket.recv_from(&mut buffer) {
            match be16(&buffer[..n]) {
                Some(OACK) => {
                    let text: Vec<&[u8]> = buffer[2..n].split(|b| *b == 0).collect();
                    for pair in text.chunks(2) {
                        if pair.len() == 2 && pair[0].eq_ignore_ascii_case(b"blksize") {
                            negotiated = std::str::from_utf8(pair[1])
                                .ok()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(512);
                        }
                    }
                    peer = Some(from);
                    break;
                }
                Some(ACK) => {
                    peer = Some(from);
                    break;
                }
                Some(ERROR) => return Err(io::Error::other("server refused the write")),
                _ => {}
            }
        }
        if attempt + 1 == RETRIES {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "no reply to WRQ"));
        }
    }
    let peer = peer.ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "no reply to WRQ"))?;

    let mut block = 1u16;
    let mut at = 0usize;
    loop {
        let end = (at + negotiated).min(data.len());
        let mut packet = Vec::with_capacity(4 + end - at);
        packet.extend_from_slice(&DATA.to_be_bytes());
        packet.extend_from_slice(&block.to_be_bytes());
        packet.extend_from_slice(&data[at..end]);
        let mut acked = false;
        for _ in 0..RETRIES {
            socket.send_to(&packet, peer)?;
            if let Ok((n, _)) = socket.recv_from(&mut buffer)
                && be16(&buffer[..n]) == Some(ACK)
                && be16(&buffer[2..n]) == Some(block)
            {
                acked = true;
                break;
            }
        }
        if !acked {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "no ACK"));
        }
        let last = end - at < negotiated;
        at = end;
        if last {
            return Ok(());
        }
        block = block.wrapping_add(1);
    }
}

/// Received files, by name.
pub type Files = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

/// A TFTP server taking uploads, on a thread until dropped.
pub struct Server {
    /// Where it listens.
    pub addr: SocketAddr,
    /// What it has received.
    pub files: Files,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

/// A name a modem may write: no directories.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && !name.contains("..") && name.len() < 256
}

fn receive_one(
    socket: &UdpSocket,
    client: SocketAddr,
    name: String,
    block_size: usize,
    files: &Files,
) -> io::Result<()> {
    // Each transfer gets its own port (RFC 1350), which is what a firewall has to allow [PNM-HFC §8.4.3].
    let transfer = UdpSocket::bind(("127.0.0.1", 0))?;
    transfer.set_read_timeout(Some(TIMEOUT))?;
    let mut oack = Vec::new();
    oack.extend_from_slice(&OACK.to_be_bytes());
    oack.extend_from_slice(b"blksize\0");
    oack.extend_from_slice(block_size.to_string().as_bytes());
    oack.push(0);
    transfer.send_to(&oack, client)?;
    let _ = socket;

    let mut data = Vec::new();
    let mut expected = 1u16;
    let mut buffer = vec![0u8; block_size + 16];
    let mut misses = 0;
    loop {
        match transfer.recv_from(&mut buffer) {
            Ok((n, from)) if from == client => {
                misses = 0;
                if be16(&buffer[..n]) != Some(DATA) {
                    continue;
                }
                let Some(block) = be16(&buffer[2..n]) else {
                    continue;
                };
                let mut ack = Vec::new();
                ack.extend_from_slice(&ACK.to_be_bytes());
                ack.extend_from_slice(&block.to_be_bytes());
                if block == expected {
                    data.extend_from_slice(&buffer[4..n]);
                    if data.len() > MAX_FILE {
                        return Err(io::Error::other("file too large"));
                    }
                    transfer.send_to(&ack, client)?;
                    expected = expected.wrapping_add(1);
                    if n - 4 < block_size {
                        if let Ok(mut store) = files.lock() {
                            store.insert(name, data);
                        }
                        return Ok(());
                    }
                } else {
                    // A retransmission: its ACK was lost.
                    transfer.send_to(&ack, client)?;
                }
            }
            Ok(_) => {}
            Err(_) => {
                misses += 1;
                if misses > RETRIES {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "client went away"));
                }
            }
        }
    }
}

impl Server {
    /// Starts one on `addr`. Port 0 picks a free one; a real deployment listens on 69.
    pub fn bind(addr: &str) -> io::Result<Server> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_read_timeout(Some(Duration::from_millis(50)))?;
        let local = socket.local_addr()?;
        let files: Files = Arc::new(Mutex::new(BTreeMap::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (stop_thread, files_thread) = (stop.clone(), files.clone());
        let handle = thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            while !stop_thread.load(Ordering::Relaxed) {
                let Ok((n, client)) = socket.recv_from(&mut buffer) else {
                    continue;
                };
                if be16(&buffer[..n]) != Some(WRQ) {
                    continue;
                }
                let parts: Vec<&[u8]> = buffer[2..n].split(|b| *b == 0).collect();
                let Some(name) = parts.first().and_then(|p| std::str::from_utf8(p).ok()) else {
                    continue;
                };
                let mode_ok = parts
                    .get(1)
                    .is_some_and(|m| m.eq_ignore_ascii_case(b"octet"));
                if !safe_name(name) || !mode_ok {
                    let mut error = Vec::new();
                    error.extend_from_slice(&ERROR.to_be_bytes());
                    error.extend_from_slice(&2u16.to_be_bytes());
                    error.extend_from_slice(b"refused\0");
                    let _ = socket.send_to(&error, client);
                    continue;
                }
                let mut block_size = 512;
                for pair in parts[2..].chunks(2) {
                    if pair.len() == 2 && pair[0].eq_ignore_ascii_case(b"blksize") {
                        block_size = std::str::from_utf8(pair[1])
                            .ok()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(512usize)
                            .clamp(8, BLOCK_SIZE_IPV4);
                    }
                }
                let name = name.to_string();
                let files = files_thread.clone();
                // Uploads from different modems overlap, so one slow one can't hold up the rest.
                if let Ok(clone) = socket.try_clone() {
                    thread::spawn(move || {
                        let _ = receive_one(&clone, client, name, block_size, &files);
                    });
                }
            }
        });
        Ok(Server {
            addr: local,
            files,
            stop,
            handle: Some(handle),
        })
    }

    /// Takes a file out, if it has arrived.
    pub fn take(&self, name: &str) -> Option<Vec<u8>> {
        self.files.lock().ok()?.remove(name)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for(server: &Server, name: &str) -> Vec<u8> {
        for _ in 0..200 {
            if let Some(data) = server.take(name) {
                return data;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("no file");
    }

    #[test]
    fn transfers_of_every_shape() {
        let server = Server::bind("127.0.0.1:0").unwrap();
        for (name, len) in [
            ("small", 10usize),
            ("exact", BLOCK_SIZE_IPV4),
            ("large", BLOCK_SIZE_IPV4 * 40 + 7),
            ("empty", 0),
        ] {
            let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            upload(server.addr, name, &data, BLOCK_SIZE_IPV4).unwrap();
            assert_eq!(wait_for(&server, name), data, "{name}");
        }
    }

    #[test]
    fn refuses_a_path_out_of_its_directory() {
        let server = Server::bind("127.0.0.1:0").unwrap();
        assert!(upload(server.addr, "../../etc/passwd", b"x", 512).is_err());
    }
}
