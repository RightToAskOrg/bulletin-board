//! Rust verifier of the Merkle tree.
//! Really you should write your own rather than trusting me...
//! but consider this as documentation and testing.


use crate::hash::HashValue;
use crate::hash_history::{FullProof, HashSource, HashInfoWithHash};
use std::path::Path;
use crate::backend_flatfile::TransactionIterator;
use std::fs::File;

/// Check that proof is actually a proof that the provided data_to_be_proven is actually part of the published_root.
/// Really you should write your own verifier, preferably in some other language,
/// so that you don't have to trust me. It's not hard. You can use the code below
/// as an example to check the precise meanings of various hashes, although the example
/// web application goes into more explicit detail and effectively provides a Javascript
/// verifier.
///
/// Returns None if the proof is OK, otherwise returns a string describing the problem. Or at least the first problem found.
///
/// # Example
///
/// ```
/// use merkle_tree_bulletin_board::hash_history::{HashSource, BranchHashHistory};
/// use merkle_tree_bulletin_board::verifier::verify_proof;
///
/// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
/// let hash_a = board.submit_leaf("a").unwrap();
/// let hash_b = board.submit_leaf("b").unwrap(); // made a branch out of a and b
/// let branch = board.get_pending_hash_values().unwrap()[0];
/// let root = board.order_new_published_root().unwrap();
/// let proof = board.get_proof_chain(hash_a).unwrap(); // get the inclusion proof for "a".
/// let root2 = board.order_new_published_root().unwrap();
/// assert!(verify_proof("a",root,&proof).is_none());
/// assert!(verify_proof("a",root2,&proof).is_some()); // wrong root
/// assert!(verify_proof("b",root,&proof).is_some()); // wrong data
/// assert!(verify_proof("b",root2,&board.get_proof_chain(hash_b).unwrap()).is_none()); // proof for "b" in the new root.
pub fn verify_proof(data_to_be_proven:&str,published_root:HashValue,proof:&FullProof) -> Option<String> {
    // check that the data provided is in the first element of the proof chain, and that it has the correct hash.
    if proof.chain.is_empty()  { return Some("No hash chain in the proof".to_string()); }
    match &proof.chain[0].source {
        HashSource::Leaf(history) => {
            if &history.data!=data_to_be_proven  { return Some("The proof is not for the provided data".to_string()); }
            if proof.chain[0].hash!=history.compute_hash() { return Some("Leaf information in the proof chain does not hash to the correct value".to_string()); }
        }
        _ => { return Some("First element in the proof chain is not actually a leaf".to_string()); }
    }
    // check that each intermediate element in the proof chain is a branch and valid. Already checked element 0 above.
    for i in 1..proof.chain.len() {
        match &proof.chain[i].source {
            HashSource::Branch(history) => {
                let hash_to_be_verified=proof.chain[i-1].hash;
                if history.left!=hash_to_be_verified && history.right!=hash_to_be_verified { return Some(format!("Element {} in the chain is a branch but does not reference the hash from element {}",i,i-1)); }
                if proof.chain[i].hash!=history.compute_hash() { return Some("Leaf information in the proof chain does not hash to the correct value".to_string()); }
            }
            _ => { return Some("First element in the proof chain is not actually a leaf".to_string()); }
        }
    }
    // check that the root in the proof is the root we heard of
    if proof.published_root.is_none() { return Some("No root information provided in the proof".to_string()); }
    let published_root_info = proof.published_root.as_ref().unwrap();
    if published_root_info.hash!=published_root { return Some("Root information in the proof is not for the desired root".to_string()); }
    match &published_root_info.source {
        HashSource::Root(history) => {
            if published_root_info.hash!=history.compute_hash() { return Some("Root information in the proof does not hash to the correct value".to_string()); }
            if !history.elements.contains(&proof.chain.last().unwrap().hash)  { return Some("Root information in the proof does not contain the last hash in the chain".to_string()); }
        }
        _ => { return Some("Root information in the proof is not actually a root".to_string()); }
    }
    None // passed all tests!
}

/// Verify that all the transactions between two published roots R and S
///  - Are all validly hashed
///  - make the difference between the given hash and its prior hash.
/// Note that R may be non-existant, if S is the first root.
///
/// filename is the name of a file containing a bulk list of the transactions between
/// roots R and S, formatted as in [backend_flatfile::write_transaction_to_csv].
/// [backend_journal::BackendJournal] will produce this in a file called <hash of S>.csv
///
/// Returns None if OK, otherwise a description of something that was wrong.
///
/// Example
///
/// ```
/// use merkle_tree_bulletin_board::backend_journal::{BackendJournal, StartupVerification};
/// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
/// use merkle_tree_bulletin_board::BulletinBoard;
/// use merkle_tree_bulletin_board::verifier::bulk_verify_between_two_published_elements;
/// use merkle_tree_bulletin_board::hash_history::HashInfoWithHash;
/// let dir = tempdir::TempDir::new("journal").unwrap();
/// let journal = BackendJournal::new(BackendMemory::default(),dir.path(),StartupVerification::SanityCheckAndRepairPending).unwrap();
/// let mut board = BulletinBoard::new(journal).unwrap();
/// board.submit_leaf("a").unwrap();
/// let hash1 = board.order_new_published_root().unwrap();
/// board.submit_leaf("b").unwrap();
/// board.submit_leaf("c").unwrap();
/// board.submit_leaf("d").unwrap();
/// board.submit_leaf("e").unwrap();
/// board.submit_leaf("f").unwrap();
/// board.submit_leaf("g").unwrap();
/// let hash2 = board.order_new_published_root().unwrap();
/// let filename1 = dir.path().join(&(hash1.to_string()+".csv"));
/// let filename2 = dir.path().join(&(hash2.to_string()+".csv"));
/// let root1 : HashInfoWithHash = board.get_hash_info(hash1).unwrap().add_hash(hash1);
/// let root2 : HashInfoWithHash = board.get_hash_info(hash2).unwrap().add_hash(hash2);
/// assert!(bulk_verify_between_two_published_elements(filename1.as_path(),None,&root1).is_none());
/// assert!(bulk_verify_between_two_published_elements(filename1.as_path(),None,&root2).is_some());
/// assert!(bulk_verify_between_two_published_elements(filename2.as_path(),Some(&root1),&root2).is_none());
/// ```
pub fn bulk_verify_between_two_published_elements(filename:&Path,old_root:Option<&HashInfoWithHash>,new_root:&HashInfoWithHash) -> Option<String> {
    // first check the old root, and extract the elements it has signed, if any.
    let mut work_elements : Vec<HashValue> = match old_root {
        None => Vec::default(),
        Some(HashInfoWithHash{ hash, source : HashSource::Root(history), parent : Option::None }) => {
            if *hash!=history.compute_hash() { return Some("Old root does not have the correct hash value".to_string()); }
            history.elements.clone()
        }
        _ => { return Some("Old root was not a root".to_string()); }
    };
    // now check the elements between.
    let mut has_found_root = false;
    for transaction in TransactionIterator::new(File::open(filename).unwrap()) {
        for (hash,source) in transaction.unwrap().pending {
            if has_found_root  { return Some(format!("Entry with hash {} comes after a root",hash)); }
            match &source {
                HashSource::Leaf(history) => {
                    if hash!=history.compute_hash() { return Some(format!("Leaf with ostensible hash {} actually has hash {}",hash,history.compute_hash())); }
                    work_elements.push(hash);
                }
                HashSource::Branch(history) => {
                    if hash!=history.compute_hash() { return Some(format!("Branch with ostensible hash {} actually has hash {}",hash,history.compute_hash())); }
                    if work_elements.len()<2 { return Some(format!("Branch with hash {} when there are not two elements to join",hash)); }
                    let expected_right = work_elements.pop().unwrap();
                    let expected_left = work_elements.pop().unwrap();
                    if expected_left==history.right && expected_right==history.left {
                        println!("Wow! The values were reversed due to a hash collision. This is better than being hit by a meteorite while winning a lottery and being struck by lightning and living. Or something weird (but probably harmless) has occurred.")
                    } else {
                        if expected_left!=history.left { return Some(format!("Branch with hash {} has a left hash {} when {} was expected",hash,history.left,expected_left)); }
                        if expected_right!=history.right { return Some(format!("Branch with hash {} has a right hash {} when {} was expected",hash,history.right,expected_right)); }
                    }
                    work_elements.push(hash);
                }
                HashSource::Root(history) => {
                    if hash!=history.compute_hash() { return Some(format!("Entry with ostensible hash {} actually has hash {}",hash,history.compute_hash())); }
                    if hash!=new_root.hash { return Some("Found a root in the data file that is not the expected root".to_string()); }
                    if new_root.source!=source { return Some("The root in the datafile has a different source to the provided source".to_string()); }
                    if history.elements!=work_elements { return Some(format!("The new root should contain elements {:#?} but actually contains {:#?}",history.elements,work_elements)); }
                    has_found_root = true;
                }
            }
        }
    }
    if !has_found_root  { return Some("No root present in file".to_string()); }
    None // passed all tests!
}