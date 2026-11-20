//! SNMPv2c over BER: the subset a PNM collector uses. GET, GETNEXT, SET, and their response.

use thiserror::Error;

/// A malformed message.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum SnmpError {
    /// Ran out of bytes, or a length didn't add up.
    #[error("truncated or malformed BER")]
    Malformed,
    /// A tag this subset doesn't read.
    #[error("unexpected tag 0x{0:02x}")]
    UnexpectedTag(u8),
}

/// An object identifier, as its arcs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Oid(pub Vec<u32>);

impl Oid {
    /// From dotted text. Panics only on programmer error in a constant, so it's for fixed OIDs.
    pub fn parse(text: &str) -> Oid {
        Oid(text
            .trim_start_matches('.')
            .split('.')
            .filter_map(|arc| arc.parse().ok())
            .collect())
    }

    /// This OID with more arcs.
    pub fn child(&self, arcs: &[u32]) -> Oid {
        let mut out = self.0.clone();
        out.extend_from_slice(arcs);
        Oid(out)
    }

    /// Whether `other` is under this one.
    pub fn is_prefix_of(&self, other: &Oid) -> bool {
        other.0.len() >= self.0.len() && other.0[..self.0.len()] == self.0[..]
    }
}

impl std::fmt::Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let arcs: Vec<String> = self.0.iter().map(u32::to_string).collect();
        f.write_str(&arcs.join("."))
    }
}

/// A variable's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// INTEGER.
    Integer(i64),
    /// OCTET STRING.
    Octets(Vec<u8>),
    /// NULL, as in a request.
    Null,
    /// OBJECT IDENTIFIER.
    ObjectId(Oid),
    /// IpAddress.
    IpAddress([u8; 4]),
    /// Counter32.
    Counter32(u32),
    /// Gauge32.
    Gauge32(u32),
    /// Counter64.
    Counter64(u64),
    /// noSuchObject exception.
    NoSuchObject,
    /// noSuchInstance exception.
    NoSuchInstance,
    /// endOfMibView exception.
    EndOfMibView,
}

/// The PDU types used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PduKind {
    /// GetRequest.
    Get,
    /// GetNextRequest.
    GetNext,
    /// Response.
    Response,
    /// SetRequest.
    Set,
}

/// Error status: noError.
pub const NO_ERROR: i64 = 0;
/// Error status: wrongType.
pub const WRONG_TYPE: i64 = 7;
/// Error status: noCreation.
pub const NO_CREATION: i64 = 11;
/// Error status: inconsistentValue, what a modem says to a test while another's running [CM-OSSI §D.2.3].
pub const INCONSISTENT_VALUE: i64 = 12;
/// Error status: notWritable.
pub const NOT_WRITABLE: i64 = 17;

/// A whole message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The community string.
    pub community: Vec<u8>,
    /// Which PDU.
    pub kind: PduKind,
    /// Request id.
    pub request_id: i64,
    /// Error status.
    pub error_status: i64,
    /// Error index.
    pub error_index: i64,
    /// Variable bindings.
    pub bindings: Vec<(Oid, Value)>,
}

fn encode_len(len: usize, out: &mut Vec<u8>) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let skip = bytes.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | (bytes.len() - skip) as u8);
        out.extend_from_slice(&bytes[skip..]);
    }
}

fn tlv(tag: u8, body: &[u8], out: &mut Vec<u8>) {
    out.push(tag);
    encode_len(body.len(), out);
    out.extend_from_slice(body);
}

fn integer_body(value: i64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let mut start = 0;
    while start < 7 {
        let (a, b) = (bytes[start], bytes[start + 1]);
        if (a == 0x00 && b & 0x80 == 0) || (a == 0xff && b & 0x80 != 0) {
            start += 1;
        } else {
            break;
        }
    }
    bytes[start..].to_vec()
}

fn unsigned_body(value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let skip = bytes.iter().take_while(|b| **b == 0).count().min(7);
    let mut out = Vec::new();
    if bytes[skip] & 0x80 != 0 {
        out.push(0);
    }
    out.extend_from_slice(&bytes[skip..]);
    out
}

fn oid_body(oid: &Oid) -> Vec<u8> {
    let mut out = Vec::new();
    let arcs = &oid.0;
    if arcs.len() >= 2 {
        out.push((arcs[0] * 40 + arcs[1]) as u8);
    }
    for &arc in arcs.iter().skip(2) {
        let mut groups = vec![(arc & 0x7f) as u8];
        let mut rest = arc >> 7;
        while rest > 0 {
            groups.push(((rest & 0x7f) as u8) | 0x80);
            rest >>= 7;
        }
        groups.reverse();
        out.extend_from_slice(&groups);
    }
    out
}

fn encode_value(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Integer(v) => tlv(0x02, &integer_body(*v), out),
        Value::Octets(v) => tlv(0x04, v, out),
        Value::Null => tlv(0x05, &[], out),
        Value::ObjectId(v) => tlv(0x06, &oid_body(v), out),
        Value::IpAddress(v) => tlv(0x40, v, out),
        Value::Counter32(v) => tlv(0x41, &unsigned_body(u64::from(*v)), out),
        Value::Gauge32(v) => tlv(0x42, &unsigned_body(u64::from(*v)), out),
        Value::Counter64(v) => tlv(0x46, &unsigned_body(*v), out),
        Value::NoSuchObject => tlv(0x80, &[], out),
        Value::NoSuchInstance => tlv(0x81, &[], out),
        Value::EndOfMibView => tlv(0x82, &[], out),
    }
}

impl Message {
    /// A request with null values.
    pub fn request(kind: PduKind, community: &[u8], request_id: i64, oids: &[Oid]) -> Message {
        Message {
            community: community.to_vec(),
            kind,
            request_id,
            error_status: NO_ERROR,
            error_index: 0,
            bindings: oids.iter().map(|o| (o.clone(), Value::Null)).collect(),
        }
    }

    /// To bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut list = Vec::new();
        for (oid, value) in &self.bindings {
            let mut bind = Vec::new();
            tlv(0x06, &oid_body(oid), &mut bind);
            encode_value(value, &mut bind);
            tlv(0x30, &bind, &mut list);
        }
        let mut pdu = Vec::new();
        tlv(0x02, &integer_body(self.request_id), &mut pdu);
        tlv(0x02, &integer_body(self.error_status), &mut pdu);
        tlv(0x02, &integer_body(self.error_index), &mut pdu);
        tlv(0x30, &list, &mut pdu);
        let tag = match self.kind {
            PduKind::Get => 0xa0,
            PduKind::GetNext => 0xa1,
            PduKind::Response => 0xa2,
            PduKind::Set => 0xa3,
        };
        let mut body = Vec::new();
        // Version 1 is SNMPv2c.
        tlv(0x02, &[1], &mut body);
        tlv(0x04, &self.community, &mut body);
        tlv(tag, &pdu, &mut body);
        let mut out = Vec::new();
        tlv(0x30, &body, &mut out);
        out
    }

    /// From bytes.
    pub fn decode(bytes: &[u8]) -> Result<Message, SnmpError> {
        let mut outer = Reader { bytes, at: 0 };
        let (tag, body) = outer.tlv()?;
        if tag != 0x30 {
            return Err(SnmpError::UnexpectedTag(tag));
        }
        let mut message = Reader { bytes: body, at: 0 };
        message.expect_integer()?;
        let (tag, community) = message.tlv()?;
        if tag != 0x04 {
            return Err(SnmpError::UnexpectedTag(tag));
        }
        let (tag, pdu_body) = message.tlv()?;
        let kind = match tag {
            0xa0 => PduKind::Get,
            0xa1 => PduKind::GetNext,
            0xa2 => PduKind::Response,
            0xa3 => PduKind::Set,
            other => return Err(SnmpError::UnexpectedTag(other)),
        };
        let mut pdu = Reader {
            bytes: pdu_body,
            at: 0,
        };
        let request_id = pdu.expect_integer()?;
        let error_status = pdu.expect_integer()?;
        let error_index = pdu.expect_integer()?;
        let (tag, list_body) = pdu.tlv()?;
        if tag != 0x30 {
            return Err(SnmpError::UnexpectedTag(tag));
        }
        let mut list = Reader {
            bytes: list_body,
            at: 0,
        };
        let mut bindings = Vec::new();
        while list.at < list.bytes.len() {
            let (tag, bind_body) = list.tlv()?;
            if tag != 0x30 {
                return Err(SnmpError::UnexpectedTag(tag));
            }
            let mut bind = Reader {
                bytes: bind_body,
                at: 0,
            };
            let (tag, oid_bytes) = bind.tlv()?;
            if tag != 0x06 {
                return Err(SnmpError::UnexpectedTag(tag));
            }
            let oid = decode_oid(oid_bytes)?;
            let (tag, value_bytes) = bind.tlv()?;
            bindings.push((oid, decode_value(tag, value_bytes)?));
        }
        Ok(Message {
            community: community.to_vec(),
            kind,
            request_id,
            error_status,
            error_index,
            bindings,
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn tlv(&mut self) -> Result<(u8, &'a [u8]), SnmpError> {
        let tag = *self.bytes.get(self.at).ok_or(SnmpError::Malformed)?;
        let first = *self.bytes.get(self.at + 1).ok_or(SnmpError::Malformed)?;
        let mut at = self.at + 2;
        let len = if first < 0x80 {
            first as usize
        } else {
            let count = (first & 0x7f) as usize;
            if count == 0 || count > 8 {
                return Err(SnmpError::Malformed);
            }
            let mut len = 0usize;
            for _ in 0..count {
                let byte = *self.bytes.get(at).ok_or(SnmpError::Malformed)?;
                len = len.checked_mul(256).ok_or(SnmpError::Malformed)? + byte as usize;
                at += 1;
            }
            len
        };
        let end = at.checked_add(len).ok_or(SnmpError::Malformed)?;
        let body = self.bytes.get(at..end).ok_or(SnmpError::Malformed)?;
        self.at = end;
        Ok((tag, body))
    }

    fn expect_integer(&mut self) -> Result<i64, SnmpError> {
        let (tag, body) = self.tlv()?;
        if tag != 0x02 {
            return Err(SnmpError::UnexpectedTag(tag));
        }
        decode_integer(body)
    }
}

fn decode_integer(body: &[u8]) -> Result<i64, SnmpError> {
    if body.is_empty() || body.len() > 8 {
        return Err(SnmpError::Malformed);
    }
    let mut value: i64 = if body[0] & 0x80 != 0 { -1 } else { 0 };
    for byte in body {
        value = (value << 8) | i64::from(*byte);
    }
    Ok(value)
}

fn decode_unsigned(body: &[u8]) -> Result<u64, SnmpError> {
    if body.is_empty() || body.len() > 9 {
        return Err(SnmpError::Malformed);
    }
    let mut value = 0u64;
    for byte in body {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

fn decode_oid(body: &[u8]) -> Result<Oid, SnmpError> {
    let first = *body.first().ok_or(SnmpError::Malformed)?;
    let mut arcs = vec![u32::from(first / 40), u32::from(first % 40)];
    let mut current = 0u32;
    for byte in &body[1..] {
        current = current
            .checked_mul(128)
            .ok_or(SnmpError::Malformed)?
            .checked_add(u32::from(byte & 0x7f))
            .ok_or(SnmpError::Malformed)?;
        if byte & 0x80 == 0 {
            arcs.push(current);
            current = 0;
        }
    }
    Ok(Oid(arcs))
}

fn decode_value(tag: u8, body: &[u8]) -> Result<Value, SnmpError> {
    Ok(match tag {
        0x02 => Value::Integer(decode_integer(body)?),
        0x04 => Value::Octets(body.to_vec()),
        0x05 => Value::Null,
        0x06 => Value::ObjectId(decode_oid(body)?),
        0x40 => Value::IpAddress(body.try_into().map_err(|_| SnmpError::Malformed)?),
        0x41 => Value::Counter32(decode_unsigned(body)? as u32),
        0x42 => Value::Gauge32(decode_unsigned(body)? as u32),
        0x46 => Value::Counter64(decode_unsigned(body)?),
        0x80 => Value::NoSuchObject,
        0x81 => Value::NoSuchInstance,
        0x82 => Value::EndOfMibView,
        other => return Err(SnmpError::UnexpectedTag(other)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_value_kind() {
        let message = Message {
            community: b"public".to_vec(),
            kind: PduKind::Response,
            request_id: -123456,
            error_status: 0,
            error_index: 0,
            bindings: vec![
                (
                    Oid::parse("1.3.6.1.4.1.4491.2.1.27.1.2.1.3.0"),
                    Value::Integer(-1),
                ),
                (Oid::parse("1.3.6.1.2.1.1.1.0"), Value::Octets(vec![0; 400])),
                (Oid::parse("1.3.6.1"), Value::Null),
                (
                    Oid::parse("1.3"),
                    Value::ObjectId(Oid::parse("1.3.6.1.300000")),
                ),
                (Oid::parse("1.3.1"), Value::IpAddress([127, 0, 0, 1])),
                (Oid::parse("1.3.2"), Value::Counter32(u32::MAX)),
                (Oid::parse("1.3.3"), Value::Gauge32(128)),
                (Oid::parse("1.3.4"), Value::Counter64(u64::MAX)),
                (Oid::parse("1.3.5"), Value::NoSuchInstance),
                (Oid::parse("1.3.7"), Value::EndOfMibView),
            ],
        };
        assert_eq!(Message::decode(&message.encode()).unwrap(), message);
    }

    #[test]
    fn known_bytes() {
        // A GET for sysDescr.0 with community "public", as any SNMP tool writes it.
        let message = Message::request(
            PduKind::Get,
            b"public",
            1,
            &[Oid::parse("1.3.6.1.2.1.1.1.0")],
        );
        let expected: Vec<u8> = vec![
            0x30, 0x26, 0x02, 0x01, 0x01, 0x04, 0x06, b'p', b'u', b'b', b'l', b'i', b'c', 0xa0,
            0x19, 0x02, 0x01, 0x01, 0x02, 0x01, 0x00, 0x02, 0x01, 0x00, 0x30, 0x0e, 0x30, 0x0c,
            0x06, 0x08, 0x2b, 0x06, 0x01, 0x02, 0x01, 0x01, 0x01, 0x00, 0x05, 0x00,
        ];
        assert_eq!(message.encode(), expected);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(Message::decode(&[]).is_err());
        assert!(Message::decode(&[0x30, 0x7f, 0x01]).is_err());
        assert!(Message::decode(&[0x02, 0x01, 0x01]).is_err());
        assert!(Message::decode(&[0x30, 0x84, 0xff, 0xff, 0xff, 0xff]).is_err());
    }

    #[test]
    fn integers_are_minimal() {
        assert_eq!(integer_body(0), vec![0]);
        assert_eq!(integer_body(127), vec![127]);
        assert_eq!(integer_body(128), vec![0, 128]);
        assert_eq!(integer_body(-1), vec![0xff]);
        assert_eq!(integer_body(-129), vec![0xff, 0x7f]);
    }
}
