//! This deals with the Merkle tree implementation.
//! At the moment it is just a simple wrapper around the merkletree library, but this may change.

use merkletree::merkle::{MerkleTree, next_pow2};
use crate::hash::{MerkleHash, HashValue};
use merkletree::store::VecStore;
use anyhow::Error;
use anyhow::anyhow;

pub struct OurMerkleTree {
    pub tree: MerkleTree<[u8; 32], MerkleHash, VecStore<[u8; 32]>>,
    pub leaf_elements : Vec<String>,
}

impl OurMerkleTree {
    pub fn get_proof(&self,index:usize) -> anyhow::Result<MerkleProof> {
        if index>= self.leaf_elements.len() { return Err(anyhow!("Index {} should be < size {}",index,self.leaf_elements.len())) }
        let proof = self.tree.gen_proof(index)?;
        let leaf = self.leaf_elements[index].clone();
        Ok(MerkleProof{ leaf, index, proof: proof.lemma().iter().map(|h|HashValue(*h)).collect() })
    }
}

/// A proof that a particular element is inside the Merkle Tree.
#[derive(serde::Serialize, Debug)]
pub struct MerkleProof {
    /// the value used for the leaf
    pub leaf : String,
    /// the index of the leaf. Used to determine the path back from the proof.
    pub index : usize,
    /// An array of hash values, starting from the hashed leaf, and extending to the root of the tree.
    pub proof : Vec<HashValue>,
}

pub fn make_merkle_tree(elements : &Vec<String>) -> Result<OurMerkleTree,Error> {
    let leaf_elements = elements.clone();
    let mut elements_used = elements.clone();
    elements_used.resize(next_pow2(elements_used.len()).max(2),"".to_string());
    let tree : MerkleTree<[u8; 32], MerkleHash, VecStore<_>> = MerkleTree::from_data(elements_used).unwrap();
    println!("Made Merkle Tree {:?}",&tree);
    Ok(OurMerkleTree { tree, leaf_elements })
}