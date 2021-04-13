//! Contain information about where the data comes from


use crate::merkle_storage::BackendFlatfile;
use crate::build_merkle::HashesInPlay;
use crate::hash::HashValue;
use anyhow::anyhow;
use crate::hash_history::{HashInfo, FullProof, HashSource};

pub struct DataSource {
    backend : BackendFlatfile,
    pending : HashesInPlay,
}


impl DataSource {
    /// submit some data to be included in the bulletin board, and get back a HashValue that the
    /// board commits to having in the history.
    pub fn submit_leaf(&mut self,data:&String) -> anyhow::Result<HashValue> {
        if data.contains(',')||data.contains('\n')||data.contains('\r')||data.contains('\\')||data.contains('"') { Err(anyhow!("Submitted strings may not contain , \\ \" newline or carriage return"))}
        else { self.pending.submit_leaf(data.clone(),&mut self.backend) }
    }

    pub fn from_flatfiles() -> anyhow::Result<DataSource> {
        let mut backend = BackendFlatfile::new("csv_database")?;
        let pending = backend.reload_and_recreate_inplay()?;
        Ok(DataSource{ backend, pending })
    }

    /// Get the current published head that everyone knows. Everyone who is paying attention, that is. And who can remember 256 bits of gibberish.
    pub fn get_current_published_head(&self) -> anyhow::Result<Option<HashValue>> {
        Ok(self.pending.get_current_published_head())
    }

    pub fn get_all_published_heads(&self) -> anyhow::Result<Vec<HashValue>> {
        Ok(self.backend.get_all_published_heads())
    }

    /// Get the currently committed to, but not yet published, hash values
    pub fn get_pending_hash_values(&self) -> anyhow::Result<Vec<HashValue>> {
        Ok(self.pending.get_pending_hash_values())
    }

    pub fn request_new_published_head(&mut self) -> anyhow::Result<HashValue> {
        self.pending.publish_now(&mut self.backend)
    }

    /// Get information about a HashValue, assuming it exists
    pub fn lookup_hash(&self,query:HashValue) -> anyhow::Result<HashInfo> {
        self.backend.lookup_hash(query).ok_or_else(||anyhow!("No such result"))
    }


    /// Convenience method to get a whole proof chain at once. It could be done via multiple calls.
    pub fn get_proof_chain(&self,query:HashValue) -> anyhow::Result<FullProof> {
        let mut chain = vec![];
        let mut node = query;
        loop {
            if let Ok(node_info) = self.lookup_hash(node) {
                chain.push(node_info.add_hash(node));
                match node_info.parent {
                    Some(parent) => node=parent,
                    None => break,
                }
            } else {
                return Err(if query==node { anyhow!("The requested hash is not valid")} else {anyhow!("The server chain has become corrupt! The node {} does not exist",node)});
            } // There is a break in the logic!!!
        }
        let published_root = {
            if let Ok(Some(published_root_hash)) = self.get_current_published_head() {
                if let Ok(node_info) = self.lookup_hash(published_root_hash) {
                    if let HashSource::Root(history) = &node_info.source {
                        if history.elements.contains(&node) {
                            Some(node_info.add_hash(published_root_hash)) // It has been published!
                        } else { None } // It (barring bad stuff) has not been published yet
                    } else { return Err(anyhow!("The server chain has become corrupt! The published node {} has the wrong history",node)); } // There is a break in the logic!!!
                } else { return Err(anyhow!("The server chain has become corrupt! The published node {} does not exist",node)); } // There is a break in the logic!!!
            } else {None}
        };
        Ok(FullProof{ chain, published_root })
    }
}