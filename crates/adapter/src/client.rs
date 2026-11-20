//! An SNMPv2c manager, with the retries UDP needs.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use pnm_wire::snmp::{Message, Oid, PduKind, SnmpError, Value};
use thiserror::Error;

/// What can go wrong on the wire.
#[derive(Error, Debug)]
pub enum WireError {
    /// The network.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A reply that didn't parse.
    #[error(transparent)]
    Snmp(#[from] SnmpError),
    /// A device said no.
    #[error("SNMP error status {0}")]
    Status(i64),
    /// No reply in time.
    #[error("timed out")]
    Timeout,
    /// A PNM file that didn't parse.
    #[error(transparent)]
    Pnm(#[from] pnm_wire::pnm::PnmError),
    /// A test that didn't produce a file.
    #[error("test failed: {0}")]
    Test(String),
    /// The device told us something that doesn't fit the schema.
    #[error("unexpected from device: {0}")]
    Unexpected(String),
}

/// A manager for one device.
pub struct SnmpClient {
    socket: UdpSocket,
    target: SocketAddr,
    community: Vec<u8>,
    next_id: i64,
}

const ATTEMPTS: usize = 3;

impl SnmpClient {
    /// A client for `target`.
    pub fn new(target: SocketAddr, community: &[u8]) -> io::Result<SnmpClient> {
        let socket = UdpSocket::bind(("127.0.0.1", 0))?;
        socket.set_read_timeout(Some(Duration::from_millis(500)))?;
        Ok(SnmpClient {
            socket,
            target,
            community: community.to_vec(),
            next_id: 1,
        })
    }

    fn exchange(&mut self, request: Message) -> Result<Message, WireError> {
        let bytes = request.encode();
        let mut buffer = vec![0u8; 65535];
        for _ in 0..ATTEMPTS {
            self.socket.send_to(&bytes, self.target)?;
            if let Ok((n, _)) = self.socket.recv_from(&mut buffer) {
                let reply = Message::decode(&buffer[..n])?;
                if reply.request_id == request.request_id {
                    if reply.error_status != 0 {
                        return Err(WireError::Status(reply.error_status));
                    }
                    return Ok(reply);
                }
            }
        }
        Err(WireError::Timeout)
    }

    fn id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    /// One GET. `None` where the device has no such instance.
    pub fn get(&mut self, oid: &Oid) -> Result<Option<Value>, WireError> {
        let id = self.id();
        let request = Message::request(
            PduKind::Get,
            &self.community.clone(),
            id,
            std::slice::from_ref(oid),
        );
        let reply = self.exchange(request)?;
        Ok(reply.bindings.into_iter().next().map(|b| b.1).filter(|v| {
            !matches!(
                v,
                Value::NoSuchInstance | Value::NoSuchObject | Value::EndOfMibView
            )
        }))
    }

    /// One SET.
    pub fn set(&mut self, oid: &Oid, value: Value) -> Result<(), WireError> {
        let id = self.id();
        let mut request = Message::request(
            PduKind::Set,
            &self.community.clone(),
            id,
            std::slice::from_ref(oid),
        );
        request.bindings[0].1 = value;
        self.exchange(request)?;
        Ok(())
    }

    /// Every instance under `root`, in order, by GETNEXT.
    pub fn walk(&mut self, root: &Oid) -> Result<Vec<(Oid, Value)>, WireError> {
        let mut out = Vec::new();
        let mut current = root.clone();
        loop {
            let id = self.id();
            let request = Message::request(
                PduKind::GetNext,
                &self.community.clone(),
                id,
                std::slice::from_ref(&current),
            );
            let reply = self.exchange(request)?;
            let Some((oid, value)) = reply.bindings.into_iter().next() else {
                return Ok(out);
            };
            if matches!(value, Value::EndOfMibView) || !root.is_prefix_of(&oid) {
                return Ok(out);
            }
            current = oid.clone();
            out.push((oid, value));
        }
    }
}
