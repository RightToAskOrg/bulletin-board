//! Contain information about where the data comes from


use crate::merkle_storage::BackendFlatfile;
use crate::build_merkle::HashesInPlay;
use crate::hash::HashValue;
use anyhow::anyhow;
use crate::hash_history::HashInfo;

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

    /// Get the current head that everyone wants.
    pub fn get_current_published_head(&self) -> Option<HashValue> {
        self.pending.get_current_published_head()
    }

    /// Get the currently committed to, but not yet published, hash values
    pub fn get_pending_hash_values(&self) -> Vec<HashValue> {
        self.pending.get_pending_hash_values()
    }

    pub fn request_new_published_head(&mut self) -> anyhow::Result<HashValue> {
        self.pending.publish_now(&mut self.backend)
    }

    pub fn lookup_hash(&self,query:HashValue) -> Option<HashInfo> {
        self.backend.lookup_hash(query)
    }

}