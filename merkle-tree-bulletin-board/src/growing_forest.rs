//! Build Merkle trees, incrementally.



use crate::hash::HashValue;
use crate::hash_history::BranchHashHistory;
use serde::{Serialize,Deserialize};
use anyhow::anyhow;
use crate::{BulletinBoardBackend, DatabaseTransaction};

#[derive(Debug,Clone,Serialize,Deserialize)]
/// A hash and its depth
/// This is used to represent the head of a perfectly balanced binary tree.
/// The depth is the distance from the root of the tree to a leaf.
/// So the tree contains 2^depth leafs.
pub struct HashAndDepth {
    /// the hash value of the root of the tree.
    pub hash : HashValue,
    /// depth of the tree. A leaf has depth 0, a branch has depth 1 or more.
    pub depth : usize,
}

/// A collection of balanced binary Merkle trees. Each tree is a different depth, and they are
/// ordered by depth, largest first.
///
/// When a new leaf is added, it is added as a tree of depth 0 at the end of this list.
/// If there was already a tree of depth 0 there, it is merged with the new leaf to make a tree of depth 1.
/// If there was already a tree of depth 1 there, it is merged with the new tree to make a tree of depth 2.
/// This continues until the invariant of having each tree be a different depth is reestablished.
///
/// The trees here have the following properties:
///  * Each tree is a perfect full balanced binary tree.
///  * Each element of the left branch of a branch predates all elements on the right branch
///
/// If there are n leafs in this forest, then there will be one tree in this forest for each
/// 1 bit in the binary representation of n, whose depth will correspond to the position of
/// said bit.
///
/// As it is used in this library, there is exactly one of these for the bulletin board, no hash
/// that is a root of a tree here has any parent, and whenever a publish event occurs, the publish
/// node references all trees listed here.
#[derive(Debug,Clone,Serialize,Deserialize,Default)]
pub struct GrowingForest {
    pub(crate) forest: Vec<HashAndDepth>,
}

fn merge_hashes<B:BulletinBoardBackend>(left:HashValue,right:HashValue,backend:&B,transaction:&mut DatabaseTransaction) -> anyhow::Result<HashValue> {
    let history = BranchHashHistory{ left, right };
    let new_hash = history.compute_hash();
    if let Some(hash_collision) = transaction.get_hash_info_completely(backend,new_hash)? {
        println!("Time to enter the lottery! You have just found a hash collision between {:?} and {:?}. More likely the program is buggy.",&hash_collision,&history);
        let history = BranchHashHistory{ right, left };
        let new_hash = history.compute_hash();
        if let Some(hash_collision) = transaction.get_hash_info_completely(backend,new_hash)? {
            println!("Time to enter the lottery! You have just found a hash collision between {:?} and {:?} as well. I am sure the program is buggy. Giving up!",&hash_collision,&history);
            Err(anyhow!("Multiple hash clashes indicates that the program is buggy or you are the unluckiest person in all the universes everywhere. I think it is the former."))
        } else { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the remaining time. Except the first collision was probably a bug, so probably won't help.
            transaction.add_branch_hash(new_hash,history);
            Ok(new_hash)
        }
    } else { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the time.
        transaction.add_branch_hash(new_hash,history);
        Ok(new_hash)
    }
}

impl GrowingForest {
    /// Merge the last two elements of this tree.
    fn merge_last_two(&mut self,backend:&impl BulletinBoardBackend,transaction:&mut DatabaseTransaction) -> anyhow::Result<()> {
        let right = self.forest.pop().unwrap();
        let left = self.forest.pop().unwrap();
        match merge_hashes(left.hash,right.hash,backend,transaction) {
            Ok(hash) => {
                self.forest.push(HashAndDepth {hash,depth:left.depth+1});
                Ok(())
            }
            Err(e) => { // unroll removal.
                self.forest.push(left);
                self.forest.push(right);
                Err(e)
            }
        }
    }
    /// Add the given hash value as a leaf to this tree collection.
    pub fn add_leaf(&mut self, hash:HashValue, backend:&impl BulletinBoardBackend, transaction:&mut DatabaseTransaction) -> anyhow::Result<()> {
        self.forest.push(HashAndDepth { hash, depth: 0 });
        while self.forest.len()>=2 && self.forest[self.forest.len()-1].depth==self.forest[self.forest.len()-2].depth {
            self.merge_last_two(backend,transaction)?;
        }
        Ok(())
    }
    /// Get a list of all the trees in this collection
    pub fn get_subtrees(&self) -> Vec<HashValue> {
        self.forest.iter().map(|e|e.hash).collect()
    }
}

