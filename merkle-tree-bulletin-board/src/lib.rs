//! # Merkle tree based bulletin board
//!
//! This is a library for a public bulletin board, allowing one to publish a series of
//! messages, and occasional root hashes. It can then provide a proof that each element
//! published before the root hash is referenced by the root hash. This is done via
//! Merkle Trees.
//!

pub mod hash;
pub mod hash_history;
pub mod merkle_storage;
pub mod build_merkle;
pub mod datasource;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
