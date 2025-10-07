use hex::{decode, encode};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

pub type Address = [u8; 20];
pub type Slot = [u8; 32];

// TODO: replace it with u256 and use safe math to avoid overflow
pub type FlatKey = u64;
pub type FlatValue = u64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub address: Address,
    pub slot: Slot,
}

impl Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Key", 2)?;
        s.serialize_field("address", &format!("0x{}", encode(self.address)))?;
        s.serialize_field("slot", &format!("0x{}", encode(self.slot)))?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct KeyHelper {
            address: String,
            slot: String,
        }

        let helper = KeyHelper::deserialize(deserializer)?;

        // strip "0x"
        let addr_bytes =
            decode(helper.address.trim_start_matches("0x")).map_err(D::Error::custom)?;
        let slot_bytes = decode(helper.slot.trim_start_matches("0x")).map_err(D::Error::custom)?;

        if addr_bytes.len() != 20 {
            return Err(D::Error::custom("address must be 20 bytes"));
        }
        if slot_bytes.len() != 32 {
            return Err(D::Error::custom("slot must be 32 bytes"));
        }

        let mut address = [0u8; 20];
        address.copy_from_slice(&addr_bytes);

        let mut slot = [0u8; 32];
        slot.copy_from_slice(&slot_bytes);

        Ok(Key { address, slot })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tx {
    pub id: u64,
    pub reads: Vec<Key>,
    pub writes: Vec<Key>,
    pub gas_hint: u64,
    pub metadata: Option<String>,
    pub program: Vec<MicroOp>,
}

#[derive(Debug)]
pub struct TxRWSet {
    pub id: u64,
    pub reads: BTreeSet<FlatKey>,
    pub writes: BTreeSet<FlatKey>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MicroOp {
    SLOAD { key: Key },
    SSTORE { key: Key },
    ADD { imm: FlatValue },
    NOOP,
}
