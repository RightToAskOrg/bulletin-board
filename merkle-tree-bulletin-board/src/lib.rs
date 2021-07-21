//! # Merkle tree based bulletin board
//!
//! This is a library for a public bulletin board, allowing one to publish a series of
//! messages, and occasional root hashes. It can then provide a proof that each element
//! published before the root hash is referenced by the root hash. This is done via
//! Merkle Trees.
//!
//! It is a method of being open, specifically of allowing external people to verify
//! the bulletin board. After entries are added, the board publishes a public root (256 bit hash).
//! Different people can confirm that they are told the same hash to check that they
//! are looking at the same bulletin board and are not getting shown different data.
//! Anyone can find a proof that their data of interest is in the board; also anyone
//! can retrieve the entire contents of the board and do whatever is wanted with it.
//!
//! As an example application, imagine a public election (this is of no use for an
//! election with private votes, which is of course often very important). Everyone
//! submits their vote to a central "of course we are totally trustworthy" authority (CA). The CA then publishes a root
//! hash, which everyone can telephone their friends to check is the same. Also,
//! everyone can easily check that *their* vote is recorded correctly (in time
//! logarithmic in the number of entries, that is, quickly). Also, anyone can
//! check the total list of votes (in time proportional to the number of votes)
//! and see that the announced tally is correct. This means that even people who
//! do not trust the CA can still trust the *result*, because it is verifiable.
//!
//! The basic idea is that the bulletin board keeps track of a collection of items
//! that it commits to. These items are built up into a tree, where each node is labeled
//! by a SHA256 hash. The root is periodically published publicly. Anyone can then check
//! that the root hash proves any particular committed element is referenced to by
//! asking for the section of the tree containing the path from said element to the
//! root. Each committed element is a leaf node whose label is the hash of the element and
//! a timestamp. Each non-root, non-leaf node has two children; it is labeled by the
//! hash of its children. The root is a hash of its children and a timestamp. The path
//! is a proof of inclusion as it would require inverting SHA256 to make a fraudulent path,
//! and this is currently considered computationally infeasible.

pub mod hash;
pub mod hash_history;
pub mod growing_forest;
pub mod backend_memory;
pub mod backend_flatfile;
pub mod backend_journal;

use crate::growing_forest::{HashAndDepth, GrowingForest};
use crate::hash::HashValue;
use anyhow::anyhow;
use crate::hash_history::{HashInfo, FullProof, HashSource, BranchHashHistory, RootHashHistory, LeafHashHistory, timestamp_now};
use std::time::Duration;

/// This is the main API to the bulletin board library. This represents an entire bulletin board.
/// You provide a backend of type [BulletinBoardBackend] (typically an indexed database),
/// and it provides a suitable API. Actually, you are likely to wrap your provided backend
/// inside a [backend_journal::BackendJournal] to provide efficient bulk verification support.
///
/// There are two simple provided backends for testing, [backend_memory::BackendMemory] and [backend_flatfile::BackendFlatfile].
///
/// There is a demo website that exposes the below API at
/// <https://github.com/RightToAskOrg/bulletin-board-demo>
/// Each API call is exposed as a REST call with relative URL
/// the function name and the the hash query, if any, as a GET argument something like  `get_hash_info?hash=a425...56`.
/// All results are returned as JSON encodings of the actual results. The leaf is submitted as a POST with body encoded JSON, name `data`
///
/// # Example
///
/// In the following example, four elements are inserted, "A", "B", "C" and "D" into a previously empty bulletin board.
/// A publication occurs after "C" and another publication after "D".
///
/// When A is inserted, it is a leaf forming a single tree of depth 0.
/// When B is inserted after it, it is merged with A to make a tree of depth 1 with A on the left and B on the right.
///
/// When C is inserted, it forms a new single tree of depth 0. This does not merge with A or B as they are already taken,
/// and it does not merge with the combined tree AB as that has a different depth and that would lead to an unbalanced tree.
/// So there are now two pending trees, one containing AB and one containing C.
///
/// The first publication contains these two trees AB and C.
///
/// When D is inserted, it forms a tree with C, and the new tree CD merges with AB to make a new depth 2 tree ABCD.
/// This single tree is in the second publication.
/// ```
/// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
/// use merkle_tree_bulletin_board::BulletinBoard;
/// use merkle_tree_bulletin_board::hash::HashValue;
/// use merkle_tree_bulletin_board::hash_history::{HashSource, LeafHashHistory, HashInfo, BranchHashHistory, RootHashHistory};
///
/// let backend = BackendMemory::default();
/// let mut board = BulletinBoard::new(backend).unwrap();
/// fn assert_is_leaf(source:HashSource,expected_data:&str) { // check that a source is indeed a leaf
///     match source {
///         HashSource::Leaf(LeafHashHistory{data : d,timestamp:_}) => assert_eq!(d,expected_data),
///         _ => panic!("Not a leaf"),
///     }
/// }
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_most_recent_published_root().unwrap(),None);
/// assert_eq!(board.get_pending_hash_values().unwrap(),vec![]);
/// #[allow(non_snake_case)]
/// let hash_A : HashValue = board.submit_leaf("A").unwrap();
/// // we have inserted A, which is a single tree but nothing is published.
/// assert_eq!(board.get_hash_info(hash_A).unwrap().parent,None);
/// assert_is_leaf(board.get_hash_info(hash_A).unwrap().source,"A");
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_pending_hash_values().unwrap(),vec![hash_A]);
/// #[allow(non_snake_case)]
/// let hash_B : HashValue = board.submit_leaf("B").unwrap();
/// // we have now inserted B, which will be merged into a tree with A on the left and B on the right.
/// #[allow(non_snake_case)]
/// let branch_AB : HashValue = board.get_hash_info(hash_A).unwrap().parent.unwrap();
/// assert_eq!(board.get_hash_info(hash_B).unwrap().parent,Some(branch_AB));
/// assert_is_leaf(board.get_hash_info(hash_B).unwrap().source,"B");
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_pending_hash_values().unwrap(),vec![branch_AB]);
/// assert_eq!(board.get_hash_info(branch_AB).unwrap(),HashInfo{source: HashSource::Branch(BranchHashHistory{left:hash_A,right:hash_B}) ,parent: None});
/// #[allow(non_snake_case)]
/// let hash_C : HashValue = board.submit_leaf("C").unwrap();
/// // we have now inserted C, which will not be merged with branchAB as they are different depths and that would lead to an unbalanced tree.
/// assert_eq!(board.get_hash_info(hash_C).unwrap().parent,None);
/// assert_is_leaf(board.get_hash_info(hash_C).unwrap().source,"C");
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_pending_hash_values().unwrap(),vec![branch_AB,hash_C]);
/// // now publish! This will publish branch_AB and hash_C.
/// let published1 = board.order_new_published_root().unwrap();
/// match board.get_hash_info(published1).unwrap().source {
///     HashSource::Root(RootHashHistory{timestamp:_,elements:e}) => assert_eq!(e,vec![branch_AB,hash_C]),
///     _ => panic!("Should be a root"),
/// }
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![published1]);
/// assert_eq!(board.get_most_recent_published_root().unwrap(),Some(published1));
/// assert_eq!(board.get_pending_hash_values().unwrap(),vec![]); // branch_AB,hash_C are still "pending" and can be merged with, but are no longer unpublished.
/// // If another publication were done now, and the timestamp hadn't happened to change, board.order_new_published_root() would probably return an error as there is no new information or time stamp, and it is probably a bug we did two almost simultaneous publications.
/// // add another element D, which will merge with C, making branch_CD, which will then merge with AB making a single tree ABCD.
/// #[allow(non_snake_case)]
/// let hash_D : HashValue = board.submit_leaf("D").unwrap();
/// // we have inserted A, which is a single tree but nothing is published.
/// #[allow(non_snake_case)]
/// let branch_CD : HashValue = board.get_hash_info(hash_C).unwrap().parent.unwrap();
/// assert_eq!(board.get_hash_info(hash_D).unwrap().parent,Some(branch_CD));
/// assert_is_leaf(board.get_hash_info(hash_D).unwrap().source,"D");
/// #[allow(non_snake_case)]
/// let branch_ABCD : HashValue = board.get_hash_info(branch_AB).unwrap().parent.unwrap();
/// assert_eq!(board.get_hash_info(branch_CD).unwrap(),HashInfo{source: HashSource::Branch(BranchHashHistory{left:hash_C,right:hash_D}) ,parent: Some(branch_ABCD)});
/// assert_eq!(board.get_hash_info(branch_ABCD).unwrap(),HashInfo{source: HashSource::Branch(BranchHashHistory{left:branch_AB,right:branch_CD}) ,parent: None});
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![published1]);
/// assert_eq!(board.get_pending_hash_values().unwrap(),vec![branch_ABCD]);
/// // do another publication, which now only hash to contain branchABCD which includes everything, including things from before the last publication.
/// let published2 = board.order_new_published_root().unwrap();
/// match board.get_hash_info(published2).unwrap().source {
///     HashSource::Root(RootHashHistory{timestamp:_,elements:e}) => assert_eq!(e,vec![branch_ABCD]),
///     _ => panic!("Should be a root"),
/// }
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![published1,published2]);
/// assert_eq!(board.get_most_recent_published_root().unwrap(),Some(published2));
/// assert_eq!(board.get_pending_hash_values().unwrap(),vec![]); // branch_ABCD is still "pending" and can be merged with, but is no longer unpublished.
/// ```
///
pub struct BulletinBoard<B:BulletinBoardBackend> {
    backend : B,
    /// None if there is an error, otherwise the currently growing forest.
    current_forest: Option<GrowingForest>,
}

/// Adding one element to a set to be committed may result in a variety of elements being produced.
/// A database may have the ability to do transactions, in which case this can be made safer by committing
/// all the modifications needed by a single API call so that the database doesn't have dangling elements.
#[derive(Default)]
pub struct DatabaseTransaction {
    pub pending : Vec<(HashValue,HashSource)>,
}

impl DatabaseTransaction {
    /// Add a new leaf hash to the database.
    pub fn add_leaf_hash(&mut self,new_hash:HashValue,history:LeafHashHistory) { self.pending.push((new_hash,HashSource::Leaf(history))) }
    /// Add a new branch hash to the database. (not leaf, not root).
    pub fn add_branch_hash(&mut self,new_hash:HashValue,history:BranchHashHistory) { self.pending.push((new_hash,HashSource::Branch(history))) }
    /// Add a new root hash to the database.
    pub fn add_root_hash(&mut self, new_hash:HashValue, history: RootHashHistory)  { self.pending.push((new_hash,HashSource::Root(history))) }

    fn get_hash_info(&self,query:HashValue) -> Option<HashSource> { self.pending.iter().find(|(hash,_)| *hash == query).map(|(_,source)|source.clone()) }
    /// check for a hash collision by looking up both this and the database backend.
    fn get_hash_info_completely(&self,backend:&impl BulletinBoardBackend,query:HashValue) -> anyhow::Result<Option<HashSource>> {
        if let Some(info) = backend.get_hash_info(query)? { Ok(Some(info.source)) }
        else { Ok(self.get_hash_info(query)) }
    }
}

/// The data from the bulletin board needs to be stored somewhere.
/// Typically this will be a database, but for generality anything implementing
/// this trait can be used.
pub trait BulletinBoardBackend {
    /// Get all published roots, for all time.
    fn get_all_published_roots(&self) -> anyhow::Result<Vec<HashValue>>;
    /// Get the most recently published root, should it exist.
    fn get_most_recent_published_root(&self) -> anyhow::Result<Option<HashValue>>;
    /// Get all leaves and branches without a parent branch. Published nodes do not count as a parent.
    /// This is used to recompute the starting state.
    fn get_all_leaves_and_branches_without_a_parent(&self) -> anyhow::Result<Vec<HashValue>>;
    /// given a hash, get information about what it represents, if anything.
    fn get_hash_info(&self, query:HashValue) -> anyhow::Result<Option<HashInfo>>;

    /// Store a transaction in the database.
    fn publish(&mut self,transaction:&DatabaseTransaction) -> anyhow::Result<()>;


    /// Get the depth of a subtree rooted at a given leaf or branch node) by following elements down the left side of each branch.
    /// A leaf node has depth 0.
    /// A branch node has depth 1 or more.
    ///
    /// The default implementation is to repeatedly call get_hash_info depth times; this is usually adequate as this is only used during startup.
    fn left_depth(&self,hash:&HashValue) -> anyhow::Result<usize> {
        let mut res = 0;
        let mut hash = *hash;
        loop {
            match self.get_hash_info(hash)? {
                Some(HashInfo{source:HashSource::Branch(history),..}) => {
                    res+=1;
                    hash = history.left;
                }
                _ => break
            }
        }
        Ok(res)
    }


    /// Deduce the current forest structure.
    /// * First find leaf or branch elements that do not have a parent. These are the trees that are in the forest
    /// * Find the depth of each of these elements.
    /// * Sort, highest first.
    ///
    /// The default implementation is usually adequate as it is only used during startup.
    fn compute_current_forest(&self) -> anyhow::Result<GrowingForest> {
        let mut pending : Vec<HashAndDepth> = Vec::default();
        for hash in self.get_all_leaves_and_branches_without_a_parent()? {
            pending.push(HashAndDepth{hash,depth:self.left_depth(&hash)?});
        }
        pending.sort_unstable_by_key(|e|e.depth);
        pending.reverse();
        Ok(GrowingForest { forest: pending })
    }

}

impl <B:BulletinBoardBackend> BulletinBoard<B> {

    /// called when the current_forest field is corrupt. Make it valid, if possible.
    fn reload_current_forest(&mut self) -> anyhow::Result<()> {
        match self.backend.compute_current_forest() {
            Ok(f) => {
                self.current_forest = Some(f);
                Ok(())
            }
            Err(e) => {
                self.current_forest = None;
                Err(e)
            }
        }
    }


    /// Helper used in submit_leaf to wrap errors so that it is easy to reload the current forest if a recoverable error (e.g. resubmitted data) occurs during this step.
    fn submit_leaf_work(&mut self,data:String) -> anyhow::Result<HashValue> {
        let history = LeafHashHistory{ timestamp: timestamp_now()?, data };
        let new_hash = history.compute_hash();
        match self.backend.get_hash_info(new_hash)? {
            Some(HashInfo{source:HashSource::Leaf(other_history), .. }) if other_history==history => {
                Err(anyhow!("You submitted the same data as already present data"))
            }
            Some(hash_collision) => {
                println!("Time to enter the lottery! Actually you have probably won without entering. You have just done a submission and found a hash collision between {:?} and {:?}",&hash_collision,&history);
                std::thread::sleep(Duration::from_secs(1)); // work around - wait a second and retry, with a new timestamp.
                self.submit_leaf_work(history.data)
            }
            _ =>  { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the time.
                let mut transaction = DatabaseTransaction::default();
                transaction.add_leaf_hash(new_hash,history);
                self.current_forest.as_mut().ok_or_else(||anyhow!("Could not initialize from database"))?.add_leaf(new_hash, &self.backend, &mut transaction)?;
                self.backend.publish(&transaction)?;
                Ok(new_hash)
            }
        }
    }


    /// Submit some data to be included in the bulletin board, and get back a HashValue that the
    /// board commits to having in the history.
    /// Note that if the same data is submitted twice in the same second it will return an error (as this probably is)
    pub fn submit_leaf(&mut self,data:&str) -> anyhow::Result<HashValue> {
        let res = self.submit_leaf_work(data.to_string());
        if res.is_err() { self.reload_current_forest()? }
        res
    }

    /// Create a new bulletin board from a backend.
    pub fn new(backend:B) -> anyhow::Result<Self> {
        let mut res = BulletinBoard { backend, current_forest : None };
        res.reload_current_forest()?;
        Ok(res)
    }

    /// Get a valid forest reference, or an error.
    fn forest_or_err(&self) -> anyhow::Result<&GrowingForest> {
        self.current_forest.as_ref().ok_or_else(||anyhow!("Could not initialize from database"))
    }

    /// Get the current published head that everyone knows. Everyone who is paying attention, that is. And who can remember 256 bits of gibberish.
    pub fn get_most_recent_published_root(&self) -> anyhow::Result<Option<HashValue>> {
        self.backend.get_most_recent_published_root()
    }

    /// Get a list of all published roots, ordered oldest to newest.
    pub fn get_all_published_roots(&self) -> anyhow::Result<Vec<HashValue>> {
        self.backend.get_all_published_roots()
    }

    /// Get the currently committed to, but not yet published, hash values.
    /// Equivalently, get all branches and leaves that do not have parents, and which are not included in the last published root.
    pub fn get_pending_hash_values(&self) -> anyhow::Result<Vec<HashValue>> {
        let mut currently_used : Vec<HashValue> = self.forest_or_err()?.get_subtrees();
        if let Some(published_root) = self.backend.get_most_recent_published_root()? {
            if let Some(HashInfo{source:HashSource::Root(history),..}) = self.backend.get_hash_info(published_root)? {
                currently_used.retain(|h|!history.elements.contains(h)) // remove already published elements.
            } else { return Err(anyhow!("The published root has has no info")) }
        }
        Ok(currently_used)
    }

    /// Request a new published root. This will contain a reference to each tree in
    /// the current forest. That is, each leaf or branch node that doesn't have a parent.
    /// This will return an error if called twice in rapid succession (same timestamp) with nothing added in the meantime, as it would otherwise produce the same hash, and is almost certainly not what was intended anyway.
    pub fn order_new_published_root(&mut self) -> anyhow::Result<HashValue> {
        let history = RootHashHistory { timestamp: timestamp_now()?, elements: self.forest_or_err()?.get_subtrees() };
        let new_hash = history.compute_hash();
        match self.backend.get_hash_info(new_hash)? {
            Some(HashInfo{source:HashSource::Root(other_history), .. }) if other_history==history => {
                Err(anyhow!("You tried to publish twice rapidly with no new data. Shame on you, you spammer."))
            }
            Some(hash_collision) => {
                println!("Time to enter the lottery! Actually you have probably won without entering. You have just done a submission and found a hash collision between {:?} and {:?}",&hash_collision,&history);
                std::thread::sleep(Duration::from_secs(1)); // work around - wait a second and retry, with a new timestamp.
                self.order_new_published_root()
            }
            _ =>  { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the time.
                let mut transaction = DatabaseTransaction::default();
                transaction.add_root_hash(new_hash,history);
                self.backend.publish(&transaction)?;
                Ok(new_hash)
            }
        }
    }

    /// Get information about a HashValue, assuming it exists.
    /// This includes its parent branch, if any, and how it is created.
    pub fn get_hash_info(&self, query:HashValue) -> anyhow::Result<HashInfo> {
        self.backend.get_hash_info(query)?.ok_or_else(||anyhow!("No such result"))
    }


    /// Convenience method to get a whole proof chain at once, that is, the chain
    /// from the provided hashvalue back to the most recent published root.
    ///
    /// This could easily be done via multiple calls
    /// to the other APIs, and indeed that is how this is implemented.
    pub fn get_proof_chain(&self,query:HashValue) -> anyhow::Result<FullProof> {
        let mut chain = vec![];
        let mut node = query;
        loop {
            if let Ok(node_info) = self.get_hash_info(node) {
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
            if let Ok(Some(published_root_hash)) = self.get_most_recent_published_root() {
                if let Ok(node_info) = self.get_hash_info(published_root_hash) {
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
