//! Build Merkle trees, incrementally.



use crate::hash::HashValue;
use crate::hash_history::{timestamp_now, BranchHashHistory, PublishedRootHistory, LeafHashHistory, HashSource};
use crate::merkle_storage::BackendFlatfile;
use std::time::Duration;
use serde::{Serialize,Deserialize};
use anyhow::anyhow;

#[derive(Debug,Clone,Serialize,Deserialize)]
struct UnpublishedElement {
    hash : HashValue,
    depth : usize,
}

/// This is sort of a partially built Merkle tree.
/// If it contained exactly 2^n elements for some n, it would be a single Merkle tree.
/// More generally, it is a collection of Merkle trees, with the following properties:
///  * Each tree is a full binary tree (relative to some leaf node - it is possibly that the leaf nodes could actually be trees).
///  * Each tree has a depth (depth d means the tree contains 2^d leaves)
///  * No two trees have the same depth
///  * The trees are ordered by depth, largest one first.
///
/// This means that one can add a new leaf by adding it as a depth 0 element at the end of the list,
/// and then recursively merging the last two elements into a tree of one higher depth if they are
/// the same.
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct BuildingIntoTree {
    pending : Vec<UnpublishedElement>,
}

fn merge_hashes(left:HashValue,right:HashValue,backend:&mut BackendFlatfile) -> anyhow::Result<HashValue> {
    let history = BranchHashHistory{
        timestamp: timestamp_now()?,
        left,
        right
    };
    let new_hash = history.compute_hash();
    if let Some(hash_collision) = backend.lookup_hash(new_hash) {
        println!("Time to enter the lottery! You have just found a hash collision between {:?} and {:?}",&hash_collision,&history);
        std::thread::sleep(Duration::from_secs(1)); // work around - wait a second and retry, with a new timestamp.
        merge_hashes(left,right,backend)
    } else { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the time.
        backend.add_branch_hash(new_hash,history)?;
        Ok(new_hash)
    }
}

impl BuildingIntoTree {
    /// Merge the last two elements of this tree.
    fn merge_last_two(&mut self,backend:&mut BackendFlatfile) -> anyhow::Result<()> {
        let right = self.pending.pop().unwrap();
        let left = self.pending.pop().unwrap();
        match merge_hashes(left.hash,right.hash,backend) {
            Ok(hash) => {
                self.pending.push(UnpublishedElement{hash,depth:left.depth+1});
                Ok(())
            }
            Err(e) => { // unroll removal.
                self.pending.push(left);
                self.pending.push(right);
                Err(e)
            }
        }
    }
    /// merge all the elements in this tree collection into a single tree, and
    /// return it, clearing this structure. Return None if there are no elements in this tree.
    pub fn merge_down_to_one(&mut self,backend:&mut BackendFlatfile) -> anyhow::Result<Option<HashValue>> {
        while self.pending.len()>1 { self.merge_last_two(backend)?; }
        Ok(self.pending.pop().map(|p|p.hash))
    }
    /// Add the given hash value as a leaf to this tree collection.
    pub fn add_hash(&mut self,hash:HashValue,backend:&mut BackendFlatfile)  -> anyhow::Result<()> {
        self.pending.push(UnpublishedElement{ hash, depth: 0 });
        while self.pending.len()>=2 && self.pending[self.pending.len()-1].depth==self.pending[self.pending.len()-2].depth {
            self.merge_last_two(backend)?;
        }
        Ok(())
    }
    /// Get a list of all the trees in this collection
    pub fn get_subtrees(&self) -> Vec<HashValue> {
        self.pending.iter().map(|e|e.hash).collect()
    }
}

/// Hashes that are "current".
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct HashesInPlay {
    pending : BuildingIntoTree,
    already_published : BuildingIntoTree,
    most_recently_published : Option<HashValue>,
}


impl HashesInPlay {

    /// Order the system to do a publish event now.
    pub fn publish_now(&mut self,backend:&mut BackendFlatfile) -> anyhow::Result<HashValue> {
        if let Some(pending_tree) = self.pending.merge_down_to_one(backend)? {
            self.already_published.add_hash(pending_tree,backend)?;
        }
        let history = PublishedRootHistory{ timestamp: timestamp_now()?, elements: self.already_published.get_subtrees() };
        let new_hash = history.compute_hash();
        if let Some(hash_collision) = backend.lookup_hash(new_hash) {
            println!("Time to enter the lottery! You have just found a hash collision between {:?} and {:?}",&hash_collision,&history);
            std::thread::sleep(Duration::from_secs(1)); // work around - wait a second and retry, with a new timestamp.
            self.publish_now(backend)
        } else { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the time.
            backend.add_published_hash(new_hash,history)?;
            self.most_recently_published = Some(new_hash);
            backend.save_inplay(self)?;
            Ok(new_hash)
        }
    }

    /// submit some data to be included in the bulletin board, and get back a HashValue that the
    /// board commits to having in the history.
    pub fn submit_leaf(&mut self,data:String,backend:&mut BackendFlatfile) -> anyhow::Result<HashValue> {
        let history = LeafHashHistory{ timestamp: timestamp_now()?, data };
        let new_hash = history.compute_hash();
        if let Some(hash_collision) = backend.lookup_hash(new_hash) {
            if hash_collision.source==HashSource::Leaf(history.clone()) { Err(anyhow!("You submitted the same data as already present data")) } else {
                println!("Time to enter the lottery! You have just done a submission and found a hash collision between {:?} and {:?}",&hash_collision,&history);
                std::thread::sleep(Duration::from_secs(1)); // work around - wait a second and retry, with a new timestamp.
                self.submit_leaf(history.data,backend)
            }
        } else { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the time.
            backend.add_leaf_hash(new_hash,history)?;
            self.pending.add_hash(new_hash,backend)?;
            backend.save_inplay(self)?;
            Ok(new_hash)
        }

    }

    /// Get the current head that everyone wants.
    pub fn get_current_published_head(&self) -> Option<HashValue> {
        self.most_recently_published
    }

    /// Get the currently committed to, but not yet published, hash values
    pub fn get_pending_hash_values(&self) -> Vec<HashValue> {
        self.pending.get_subtrees()
    }

    /// Reconstitute self based on deduced data
    /// Hashvalues that have no parent (and are not published roots) are the ones that go in this structure
    /// The ones which have been published in the most recent published root go in the "already published" section, the others in "pending".
    pub fn build_from(no_parent_unpublished:Vec<(HashValue,usize)>,no_parent_published:Vec<(HashValue,usize)>,most_recently_published:Option<HashValue>) -> Self {
        HashesInPlay{
            pending: BuildingIntoTree { pending: no_parent_unpublished.iter().map(|(hash,depth)|UnpublishedElement{ hash: *hash, depth: *depth }).collect() },
            already_published: BuildingIntoTree { pending: no_parent_published.iter().map(|(hash,depth)|UnpublishedElement{ hash: *hash, depth: *depth }).collect() },
            most_recently_published
        }

    }
}


