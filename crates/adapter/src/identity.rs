//! Invariant 10: MACs are hashed at ingest and the twin is keyed by plant location. Only this store holds what it
//! takes to reach a modem, and nothing outside this crate reads it.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use hmac::{Hmac, Mac};
use schema::common::ElementId;
use schema::plant::{ModemBinding, Plant};
use sha2::Sha256;

/// A keyed hash of a MAC: stable for one operator's key, and useless without it.
pub fn pseudonym(key: &[u8], mac: &[u8; 6]) -> String {
    let mut hasher = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap_or_else(|_| {
        // HMAC accepts a key of any length; the empty slice is the only stand-in this needs.
        <Hmac<Sha256> as Mac>::new_from_slice(&[0u8; 32]).expect("a fixed key is valid")
    });
    hasher.update(mac);
    let digest = hasher.finalize().into_bytes();
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// What the adapter needs to reach a modem, by pseudonym.
#[derive(Debug, Clone, Default)]
pub struct AddressStore {
    entries: BTreeMap<String, (SocketAddr, [u8; 6])>,
}

impl AddressStore {
    /// Records one. The MAC stays here, and is what test triggers and files carry.
    pub fn insert(&mut self, pseudonym: String, addr: SocketAddr, mac: [u8; 6]) {
        self.entries.insert(pseudonym, (addr, mac));
    }

    /// Where a modem is.
    pub fn address(&self, pseudonym: &str) -> Option<SocketAddr> {
        self.entries.get(pseudonym).map(|e| e.0)
    }

    /// How many it holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it's empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The pseudonyms, in canonical order.
    pub fn pseudonyms(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }
}

/// Ties a plant's modems to pseudonyms, from an inventory of which MAC is where. The plant then has no address
/// in it at all.
pub fn bind_plant(plant: &mut Plant, key: &[u8], inventory: &[(ElementId, [u8; 6])]) {
    plant.modem_bindings = inventory
        .iter()
        .map(|(element_id, mac)| ModemBinding {
            element_id: element_id.clone(),
            pseudonym: pseudonym(key, mac),
        })
        .collect();
    plant
        .modem_bindings
        .sort_by(|a, b| a.element_id.cmp(&b.element_id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudonyms_are_keyed_and_stable() {
        let mac = [2, 0, 0, 0, 0, 1];
        assert_eq!(pseudonym(b"key", &mac), pseudonym(b"key", &mac));
        assert_ne!(pseudonym(b"key", &mac), pseudonym(b"other", &mac));
        assert_ne!(
            pseudonym(b"key", &mac),
            pseudonym(b"key", &[2, 0, 0, 0, 0, 2])
        );
        assert_eq!(pseudonym(b"key", &mac).len(), 32);
        assert!(!pseudonym(b"key", &mac).contains("020000"));
    }
}
