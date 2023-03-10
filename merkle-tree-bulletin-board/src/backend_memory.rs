use std::collections::HashMap;
use crate::hash::HashValue;
use crate::hash_history::{HashInfo, HashSource, LeafHashHistory};
use crate::{BulletinBoardBackend, BulletinBoardError, DatabaseTransaction};

/// Store the contents of the "database" holding what has happened in memory. Useful for tests, but not for production.
#[derive(Default)]
pub struct BackendMemory {
    hash_lookup : HashMap<HashValue,HashInfo>,
    published : Vec<HashValue>,
}

impl BulletinBoardBackend for BackendMemory {
    fn get_all_published_roots(&self) -> Result<Vec<HashValue>,BulletinBoardError> {
        Ok(self.published.clone())
    }

    fn get_most_recent_published_root(&self) -> Result<Option<HashValue>,BulletinBoardError> {
        Ok(self.published.last().map(|h|*h))
    }

    fn get_all_leaves_and_branches_without_a_parent(&self) -> Result<Vec<HashValue>,BulletinBoardError> {
        fn ok(info:&HashInfo) -> bool {
            info.parent.is_none() && match info.source {
                HashSource::Leaf(_) => true,
                HashSource::Branch(_) => true,
                HashSource::Root(_) => false,
            }
        }
        Ok(self.hash_lookup.iter().filter(|(_,info)|ok(&info)).map(|(hash,_)|(*hash)).collect())
    }

    fn get_hash_info(&self, query: HashValue) -> Result<Option<HashInfo>,BulletinBoardError> {
        Ok(self.hash_lookup.get(&query).map(|r|r.clone()))
    }

    fn publish(&mut self, transaction: &DatabaseTransaction) -> Result<(),BulletinBoardError> {
        for (new_hash,source) in &transaction.pending {
            match source {
                HashSource::Leaf(history) => {
                    self.hash_lookup.insert(*new_hash,HashInfo{ source: HashSource::Leaf(history.clone()), parent: None });
                }
                HashSource::Branch(history) => {
                    self.hash_lookup.insert(*new_hash,HashInfo{ source: HashSource::Branch(history.clone()), parent: None });
                    self.add_parent(&history.left,*new_hash);
                    self.add_parent(&history.right,*new_hash);

                }
                HashSource::Root(history) => {
                    self.hash_lookup.insert(*new_hash,HashInfo{ source: HashSource::Root(history.clone()), parent: None });
                    self.published.push(*new_hash);
                }
            }
        }
        Ok(())
    }

    fn censor_leaf(&mut self, leaf_to_censor: HashValue) -> Result<(),BulletinBoardError> {
        match self.hash_lookup.get_mut(&leaf_to_censor) {
            None => Err(BulletinBoardError::NoSuchHash),
            Some(info) => {
                match &info.source {
                    HashSource::Leaf(LeafHashHistory{timestamp,..}) => {
                        info.source=HashSource::Leaf(LeafHashHistory{timestamp:*timestamp,data:None});
                        Ok(())
                    }
                    _ => Err(BulletinBoardError::CanOnlyCensorLeaves),
                }
            }
        }
    }
}

impl BackendMemory {
    fn add_parent(&mut self,child:&HashValue,parent:HashValue) {
        self.hash_lookup.get_mut(child).unwrap().parent=Some(parent);
    }

}
