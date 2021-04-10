use sha2::{Sha256, Digest};
use std::hash::Hasher;
use merkletree::hash::Algorithm;
use merkletree::merkle::{MerkleTree, next_pow2};
use merkletree::store::VecStore;
use anyhow::Error;

pub struct MerkleHash(Sha256);

impl MerkleHash {
    pub fn new() -> MerkleHash {
        MerkleHash(Sha256::new())
    }
}

impl Default for MerkleHash {
    fn default() -> MerkleHash {
        MerkleHash::new()
    }
}

impl Hasher for MerkleHash {
    #[inline]
    fn finish(&self) -> u64 {
        unimplemented!() // not needed for Merkle tree.
    }

    #[inline]
    fn write(&mut self, msg: &[u8]) {
        self.0.update(msg)
    }
}

impl Algorithm<[u8; 32]> for MerkleHash {
    #[inline]
    fn hash(&mut self) -> [u8; 32] {
        let output = self.0.finalize_reset();
        <[u8; 32]>::from(output)
//        let mut h = [0u8; 32];
//        self.0.result(&mut h);
//        h
    }

    #[inline]
    fn reset(&mut self) {
        self.0.reset();
    }
}

pub struct OurMerkleTree {
    pub tree: MerkleTree<[u8; 32], MerkleHash, VecStore<[u8; 32]>>,
    pub leaf_elements : Vec<String>,
}

pub fn make_merkle_tree(elements : &Vec<String>) -> Result<OurMerkleTree,Error> {
    let leaf_elements = elements.clone();
    let mut elements_used = elements.clone();
    elements_used.resize(next_pow2(elements_used.len()).max(2),"".to_string());
    let tree : MerkleTree<[u8; 32], MerkleHash, VecStore<_>> = MerkleTree::from_data(elements_used).unwrap();
    println!("Made Merkle Tree {:?}",&tree);
    Ok(OurMerkleTree { tree, leaf_elements })
}