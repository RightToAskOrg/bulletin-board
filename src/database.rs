//! This file contains the main functions that the program runs.
//! The intention is that this should be stored in a database; for the moment it is just stored in RAM.

use std::sync::{Mutex, MutexGuard};
use crate::merkle::{OurMerkleTree, MerkleProof};
use crate::hash::HashValue;

struct GlobalState {
    pending_put_into_merkle_tree : Vec<String>,
    merkle_trees : Vec<OurMerkleTree>
}

fn init_global_state() -> Mutex<GlobalState> {
    Mutex::new(GlobalState{ pending_put_into_merkle_tree: vec![], merkle_trees: vec![] })
}

lazy_static! {
    static ref GLOBAL_STATE : Mutex<GlobalState> = init_global_state();
}

fn state() -> MutexGuard<'static,GlobalState> {
    GLOBAL_STATE.lock().unwrap()
}

pub fn add_item_to_merkle(item:&str) {
    state().pending_put_into_merkle_tree.push(item.to_string())
}

pub fn get_pending() -> Vec<String> {
    state().pending_put_into_merkle_tree.clone()
}

pub fn initiate_merkle() -> anyhow::Result<[u8; 32]> {
    let mut state = state();
    let tree = crate::merkle::make_merkle_tree(&state.pending_put_into_merkle_tree)?;
    state.merkle_trees.push(tree);
    state.pending_put_into_merkle_tree.clear();
    Ok(state.merkle_trees.last().unwrap().tree.root())
}

#[derive(serde::Serialize)]
pub struct MerkleSummary {
    root : HashValue,
    leafs : Vec<String>,
}

impl MerkleSummary {
    fn new(tree : &OurMerkleTree) -> Self {
        MerkleSummary{
            root : HashValue(tree.tree.root()),
            leafs : tree.leaf_elements.clone(),
        }
    }
}

pub fn get_merkle_tree_summaries() -> Vec<MerkleSummary> {
    let state = state();
    state.merkle_trees.iter().map(MerkleSummary::new).collect()
}



pub fn get_proof(index:usize) -> anyhow::Result<MerkleProof> {
    let state = state();
    let tree = state.merkle_trees.last().ok_or_else(||anyhow::Error::msg("No tree"))?;
    tree.get_proof(index)
}