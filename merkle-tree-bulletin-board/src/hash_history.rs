use crate::hash::HashValue;
use std::time::{SystemTime, SystemTimeError};
use sha2::{Sha256, Digest};
use serde::{Serialize,Deserialize};

/// Unix timestamp, in, seconds since Epoch.
pub type Timestamp = u64;

/// get the present time stamp.
pub fn timestamp_now() -> Result<Timestamp,SystemTimeError> { Ok(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs()) }

/// Where a leaf comes from
/// Hash = sha256(0|timestamp(bigendian)|data)
#[derive(Debug,Clone,Serialize,Deserialize,Eq,PartialEq)]
pub struct LeafHashHistory {
    /// when the leaf was received
    pub timestamp : Timestamp,
    /// the data that went into the leaf;
    pub data : String,
}

impl LeafHashHistory {
    pub fn compute_hash(&self) -> HashValue {
        let mut hasher = Sha256::default();
        hasher.update(&[0]);
        hasher.update(self.timestamp.to_be_bytes());
        hasher.update(self.data.as_bytes());
        HashValue(<[u8; 32]>::from(hasher.finalize()))
    }
}

/// Where a branch comes from
/// Hash = sha256(1|timestamp(bigendian)|left|right)
#[derive(Debug,Copy,Clone,Serialize,Deserialize,Eq,PartialEq)]
pub struct BranchHashHistory {
    pub timestamp : Timestamp,
    pub left : HashValue,
    pub right : HashValue,
}

impl BranchHashHistory {
    pub fn compute_hash(&self) -> HashValue {
        let mut hasher = Sha256::default();
        hasher.update(&[1]);
        hasher.update(self.timestamp.to_be_bytes());
        hasher.update(&self.left.0);
        hasher.update(&self.right.0);
        HashValue(<[u8; 32]>::from(hasher.finalize()))
    }
}

/// Where a root comes from
/// Hash = sha256(2|timestamp|elements concatenated)
#[derive(Debug,Clone,Serialize,Deserialize,Eq,PartialEq)]
pub struct PublishedRootHistory {
    /// time that the root was published.
    pub timestamp : Timestamp,
    /// elements in this root
    pub elements : Vec<HashValue>,
}

impl PublishedRootHistory {
    pub fn compute_hash(&self) -> HashValue {
        let mut hasher = Sha256::default();
        hasher.update(&[2]);
        hasher.update(self.timestamp.to_be_bytes());
        for elem in &self.elements {
            hasher.update(&elem.0);
        }
        HashValue(<[u8; 32]>::from(hasher.finalize()))
    }
}



/// where a Hash came from
#[derive(Debug,Clone,Serialize,Deserialize,Eq,PartialEq)]
pub enum HashSource {
    Leaf(LeafHashHistory),
    Branch(BranchHashHistory),
    Root(PublishedRootHistory),
}

impl HashSource {
    pub fn timestamp(&self) -> Timestamp {
        match self {
            HashSource::Leaf(history) => history.timestamp,
            HashSource::Branch(history) => history.timestamp,
            HashSource::Root(history) => history.timestamp,
        }
    }
}

/// Full information on a hash
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct HashInfo {
    pub source : HashSource,
    pub parent : Option<HashValue>,
}

impl HashInfo {
    /// produce a more detailed structure including the hash value.
    pub fn add_hash(&self,hash:HashValue) -> HashInfoWithHash {
        HashInfoWithHash {
            hash,
            source: self.source.clone(),
            parent: self.parent,
        }
    }
}
/// Full information on a hash
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct HashInfoWithHash {
    pub hash : HashValue,
    pub source : HashSource,
    pub parent : Option<HashValue>,
}

/// A proof structure that a given hash value is included.
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct FullProof {
    /// chain back to the start. Each element is the parent of the prior element.
    pub chain : Vec<HashInfoWithHash>,
    /// most recent published root, if it includes the last element of the chain. If None, then the last element of the chain has not been published yet.
    pub published_root : Option<HashInfoWithHash>
}


