use crate::hash::HashValue;
use std::time::{SystemTime, SystemTimeError};
use sha2::{Sha256, Digest};
use serde::{Serialize,Deserialize};

/// Unix timestamp, in, seconds since Epoch.
pub type Timestamp = u64;

/// get the present time stamp.
pub fn timestamp_now() -> Result<Timestamp,SystemTimeError> { Ok(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs()) }

/// Where a leaf comes from
/// Hash = sha256(0|timestamp(bigendian 64 bits)|data)
/// If the data is None it means it has been censored post incorporation into the tree,
/// and therefore it is no longer possible to compute the hash.
#[derive(Debug,Clone,Serialize,Deserialize,Eq,PartialEq)]
pub struct LeafHashHistory {
    /// when the leaf was received
    pub timestamp : Timestamp,
    /// the data that went into the leaf; If None then it has been censored.
    pub data : Option<String>,
}

impl LeafHashHistory {
    /// Hash = sha256(0|timestamp(bigendian 64 bits)|data)
    /// Returns None if the data has been censored.
    pub fn compute_hash(&self) -> Option<HashValue> {
        if let Some(data) = &self.data {
            let mut hasher = Sha256::default();
            hasher.update(&[0]);
            hasher.update(self.timestamp.to_be_bytes());
            hasher.update(data.as_bytes());
            Some(HashValue(<[u8; 32]>::from(hasher.finalize())))
        } else { None }
    }
}

/// Where a branch comes from. A branch has exactly two children, called left and right.
///
/// Every element in the right side of the tree will generally postdate every element on the left side of the tree,
/// The exception is in the absurdly unlikely case of a hash collision, in which case the two sides will be swapped.
///
/// The depth of the left side will always be the same as the depth of the right side; this is a balanced tree.
///
/// Hash = sha256(1|left|right)
#[derive(Debug,Copy,Clone,Serialize,Deserialize,Eq,PartialEq)]
pub struct BranchHashHistory {
    pub left : HashValue,
    pub right : HashValue,
}

impl BranchHashHistory {
    pub fn compute_hash(&self) -> HashValue {
        let mut hasher = Sha256::default();
        hasher.update(&[1]);
        hasher.update(&self.left.0);
        hasher.update(&self.right.0);
        HashValue(<[u8; 32]>::from(hasher.finalize()))
    }
}

/// Where a root comes from
/// Hash = sha256(2|timestamp|prior if exists otherwise byte 0|elements concatenated)
#[derive(Debug,Clone,Serialize,Deserialize,Eq,PartialEq)]
pub struct RootHashHistory {
    /// time that the root was published.
    pub timestamp : Timestamp,
    /// the prior published root, if any.
    pub prior : Option<HashValue>,
    /// elements in this root
    pub elements : Vec<HashValue>,
}

impl RootHashHistory {
    pub fn compute_hash(&self) -> HashValue {
        let mut hasher = Sha256::default();
        hasher.update(&[2]);
        hasher.update(self.timestamp.to_be_bytes());
        match self.prior {
            None => hasher.update(&[0]),
            Some(prior) => hasher.update(&prior.0),
        }
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
    Root(RootHashHistory),
}

/// Full information on a hash, where it has come from and its parent, if any.
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
pub struct HashInfo {
    /// Why it was was created.
    pub source : HashSource,
    /// The branch parent, if it has one. Published root nodes do not count as parents and do not have parents.
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

/// Full information on a hash. Like [HashInfo] but with the hash as well.
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct HashInfoWithHash {
    pub hash : HashValue,
    pub source : HashSource,
    pub parent : Option<HashValue>,
}

/// A proof structure that a given hash value is included.
/// It is a chain from the desired hash value back to the most recently published root, should it be present.
/// If the hash has been generated after the most recently published root, it will not of course be traceable back to it.
///
/// See [crate::verifier::verify_proof] for how to verify the proof.
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct FullProof {
    /// chain back to the start. Each element is the parent of the prior element.
    pub chain : Vec<HashInfoWithHash>,
    /// most recent published root, if it includes the last element of the chain. If None, then the last element of the chain has not been published yet.
    pub published_root : Option<HashInfoWithHash>
}


