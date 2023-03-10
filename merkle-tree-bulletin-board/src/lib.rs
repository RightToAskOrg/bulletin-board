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
//!
//! The system allows censorship of individual leaves. This is obviously generally undesirable and
//! to some extent undermines some of the point of a committed bulletin board. However,
//! most if not all countries have censorship laws that require operators of bulletin boards
//! to do censorship, regardless of whether or not you agree with such laws. The system is
//! designed to have the following properties in the presence of censorship:
//!  * Censorship does not invalidate any published root.
//!  * Censorship, even post a published root, does not invalidate or even affect any proof chain
//!    other than the particular leaf being censored.
//!  * Even the leaf being censored can still be verified should you happen to know
//!    what had been present before the censorship.
//!  * It is impossible to hide the fact that a censorship post published root has occurred.
//!  * If you do not use the censorship feature, then the overhead of having it present is negligible.
//!
//! Censorship is accomplished by simply not providing the censored text; the leaf and its
//! associated hash value (and timestamp) are still present. The hash value cannot be modified
//! post published root, as that would invalidate the parent branch. The timestamp cannot
//! however be verified unless you happen to know the uncensored text.
//!

pub mod hash;
pub mod hash_history;
pub mod growing_forest;
pub mod backend_memory;
pub mod backend_flatfile;
pub mod backend_journal;
pub mod deduce_journal;
pub mod verifier;

use crate::growing_forest::GrowingForest;
use crate::hash::{FromHashValueError, HashValue};
use crate::hash_history::{HashInfo, FullProof, HashSource, BranchHashHistory, RootHashHistory, LeafHashHistory, timestamp_now, HashInfoWithHash, Timestamp};
use std::time::Duration;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::num::ParseIntError;
use serde::{Serialize,Deserialize};

/// This is the main API to the bulletin board library. This represents an entire bulletin board.
/// You provide a backend of type [BulletinBoardBackend] (typically an indexed database),
/// and it provides a suitable API.
///
/// Actually, you are likely to wrap your provided backend
/// inside a [backend_journal::BackendJournal] to provide efficient bulk verification support,
/// unless you want efficient censorship support. Alternatively
/// you may use the computationally expensive [deduce_journal::deduce_journal] to compute
/// the journal needed for bulk verification when needed. In production, you would probably
/// want to make a separate backend with a separate database connection so [deduce_journal::deduce_journal]
/// can run in parallel.
///
/// There are two simple provided backends for testing and prototyping,
/// [backend_memory::BackendMemory] and [backend_flatfile::BackendFlatfile].
/// In production you will probably want to use some database; this is somewhat database
/// dependent, and so a sample mysql database backend is given
/// in <https://github.com/RightToAskOrg/bulletin-board>.
///
/// There is a demo website project (bulletin-board-demo) that exposes the below API
/// in the git repository at
/// <https://github.com/RightToAskOrg/bulletin-board>
/// Each API call is exposed as a REST call with relative URL
/// the function name and the the hash query, if any, as a GET argument something like  `get_hash_info?hash=a425...56`.
/// All results are returned as JSON encodings of the actual results. The leaf is submitted as a POST with body encoded JSON object containing a single field name `data`,
/// and censoring is similarly a POST with body encoded JSON object with a single field name `leaf_to_censor`.
///
///
/// The Merkle trees are grown as described in [GrowingForest]. Each published root consists of a hash of a small O(log leafs) number
/// of leaf or branch nodes, and the prior published root. Each branch node in it is a perfectly balanced binary tree.
/// Verification steps are described in [backend_journal::BackendJournal]
///
/// # Example
///
/// In the following example, four elements are inserted, "a", "b", "c" and "d" into a previously empty bulletin board.
/// A publication occurs after "c" and another publication after "d".
///
/// When "a" is inserted, it is a leaf forming a single tree of depth 0.
/// When "b" is inserted after it, it is merged with "a" to make a tree of depth 1 with "a" on the left and "b" on the right.
///
/// When c is inserted, it forms a new single tree of depth 0. This does not merge with "a" or "b" as they are already taken,
/// and it does not merge with the combined tree ab as that has a different depth and that would lead to an unbalanced tree.
/// So there are now two pending trees, one containing ab and one containing c.
///
/// The first publication contains these two trees ab and c.
///
/// When d is inserted, it forms a tree with c, and the new tree cd merges with ab to make a new depth 2 tree abcd.
/// This single tree is in the second publication.
/// ```
/// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
/// use merkle_tree_bulletin_board::BulletinBoard;
/// use merkle_tree_bulletin_board::hash::HashValue;
/// use merkle_tree_bulletin_board::hash_history::{HashSource, LeafHashHistory, HashInfo, BranchHashHistory, RootHashHistory};
///
/// let backend = BackendMemory::default();
/// let mut board = BulletinBoard::new(backend).unwrap();
/// // utility function to check that something is indeed a leaf with the expected data.
/// fn assert_is_leaf(source:HashSource,expected_data:&str) {
///   match source {
///     HashSource::Leaf(LeafHashHistory{data:Some(d),timestamp:_}) => assert_eq!(d,expected_data),
///     _ => panic!("Not a leaf"),
///   }
/// }
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_most_recent_published_root().unwrap(),None);
/// assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![]);
///
/// let hash_a : HashValue = board.submit_leaf("a").unwrap();
/// // we have inserted A, which is a single tree but nothing is published.
/// assert_eq!(board.get_hash_info(hash_a).unwrap().parent,None);
/// assert_is_leaf(board.get_hash_info(hash_a).unwrap().source,"a");
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![hash_a]);
///
/// let hash_b : HashValue = board.submit_leaf("b").unwrap();
/// // we have inserted 'b', which will be merged into a tree with 'a' on the left and 'b' right.
/// let branch_ab : HashValue = board.get_hash_info(hash_a).unwrap().parent.unwrap();
/// assert_eq!(board.get_hash_info(hash_b).unwrap().parent,Some(branch_ab));
/// assert_is_leaf(board.get_hash_info(hash_b).unwrap().source,"b");
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![branch_ab]);
/// assert_eq!(board.get_hash_info(branch_ab).unwrap(), HashInfo{
///    source: HashSource::Branch(BranchHashHistory{left:hash_a,right:hash_b}) ,parent: None});
///
/// let hash_c : HashValue = board.submit_leaf("c").unwrap();
/// // we have now inserted 'c', which will not be merged with branch_ab
/// // as they are different depths and that would lead to an unbalanced tree.
/// assert_eq!(board.get_hash_info(hash_c).unwrap().parent,None);
/// assert_is_leaf(board.get_hash_info(hash_c).unwrap().source,"c");
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
/// assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![branch_ab,hash_c]);
///
/// // now publish! This will publish branch_ab and hash_c.
/// let published1 : HashValue = board.order_new_published_root().unwrap();
/// match board.get_hash_info(published1).unwrap().source {
///     HashSource::Root(RootHashHistory{timestamp:_,elements:e,prior:None}) =>
///        assert_eq!(e,vec![branch_ab,hash_c]),
///     _ => panic!("Should be a root"),
/// }
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![published1]);
/// assert_eq!(board.get_most_recent_published_root().unwrap(),Some(published1));
/// assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![]);
/// // branch_ab,hash_c are still parentless and can be merged with, but are no longer unpublished.
///
/// // add another element 'd', which will merge with 'c', making branch_cd,
/// // which will then merge with ab making a single tree abcd.
/// let hash_d : HashValue = board.submit_leaf("d").unwrap();
/// let branch_cd : HashValue = board.get_hash_info(hash_c).unwrap().parent.unwrap();
/// assert_eq!(board.get_hash_info(hash_d).unwrap().parent,Some(branch_cd));
/// assert_is_leaf(board.get_hash_info(hash_d).unwrap().source,"d");
/// let branch_abcd : HashValue = board.get_hash_info(branch_ab).unwrap().parent.unwrap();
/// assert_eq!(board.get_hash_info(branch_cd).unwrap(),HashInfo{
///     source: HashSource::Branch(BranchHashHistory{left:hash_c,right:hash_d}) ,
///     parent: Some(branch_abcd)});
/// assert_eq!(board.get_hash_info(branch_abcd).unwrap(),HashInfo{
///     source: HashSource::Branch(BranchHashHistory{left:branch_ab,right:branch_cd}) ,
///     parent: None});
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![published1]);
/// assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![branch_abcd]);
///
/// // do another publication, which now only has to contain abcd which includes everything,
/// // including things from before the last publication.
/// let published2 = board.order_new_published_root().unwrap();
/// match board.get_hash_info(published2).unwrap().source {
///     HashSource::Root(RootHashHistory{timestamp:_,elements:e,prior:Some(prior)}) => {
///         assert_eq!(e,vec![branch_abcd]);
///         assert_eq!(prior,published1);
///     }
///     _ => panic!("Should be a root"),
/// }
/// assert_eq!(board.get_all_published_roots().unwrap(),vec![published1,published2]);
/// assert_eq!(board.get_most_recent_published_root().unwrap(),Some(published2));
/// assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![]);
/// // branch_abcd is still parentless and can be merged with, but is no longer unpublished.
/// ```
///
pub struct BulletinBoard<B:BulletinBoardBackend> {
    pub backend : B,
    /// None if there is an error, otherwise the currently growing forest.
    current_forest: Option<GrowingForest>,
}

/// Possible things that could go wrong during a Bulletin Board operation.
#[derive(Debug,Clone,Serialize,Deserialize,Eq,PartialEq,thiserror::Error)]
pub enum BulletinBoardError {
    #[error("Identical data has already been submitted very recently to the bulletin board")]
    IdenticalDataAlreadySubmitted,
    #[error("Could not initialize bulletin board from the database")]
    CouldNotInitializeFromDatabase,
    #[error("The published root has no information in the backend")]
    PublishedRootHasNoInfo,
    #[error("A new published root was ordered immediately after already publishing one, with no new data")]
    PublishingNewRootInstantlyAfterLastRoot,  // if ordering a new root to be published with same timestamp and no new data.
    #[error("The provided hash value does not correspond to a node")]
    NoSuchHash,
    #[error("The proof chain is missing node {0}")]
    ProofChainCorruptMissingPublishedNode(HashValue),
    #[error("The published root {0} is not actually a root node")]
    PublishedRootIsNotARoot(HashValue),
    #[error("Multiple hash clashes indicates that the program is buggy or you are the unluckiest person in all the universes everywhere. I think it is the former")]
    MultipleHashClashes,
    #[error("Can only censor leaves")]
    CanOnlyCensorLeaves,
    #[error("The bulletin board backend is inconsistent or corrupt : {0}")]
    BackendInconsistentError(String),
    #[error("The bulletin board backend has an IO Error : {0}")]
    BackendIOError(String),
    #[error("The bulletin board backend had an error parsing a value : {0}")]
    BackendParsingError(String),
    #[error("system time clock is not available")]
    ClockError,
}


impl From<std::io::Error> for BulletinBoardError {
    fn from(value: std::io::Error) -> Self {
        BulletinBoardError::BackendIOError(value.to_string())
    }
}
impl From<FromHashValueError> for BulletinBoardError {
    fn from(value: FromHashValueError) -> Self {
        BulletinBoardError::BackendParsingError(value.to_string())
    }
}

impl From<ParseIntError> for BulletinBoardError {
    fn from(value: ParseIntError) -> Self {
        BulletinBoardError::BackendParsingError(value.to_string())
    }
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
    fn get_hash_info_completely(&self,backend:&impl BulletinBoardBackend,query:HashValue) -> Result<Option<HashSource>,BulletinBoardError> {
        if let Some(info) = backend.get_hash_info(query)? { Ok(Some(info.source)) }
        else { Ok(self.get_hash_info(query)) }
    }

    /// make a transaction containing a single entry.
    pub fn singleton(hash:HashValue,source:HashSource) -> DatabaseTransaction {
        DatabaseTransaction{ pending:vec![(hash,source)]}
    }
}

/// The data from the bulletin board needs to be stored somewhere.
/// Typically this will be a database, but for generality anything implementing
/// this trait can be used.
pub trait BulletinBoardBackend {
    /// Get all published roots, for all time.
    fn get_all_published_roots(&self) -> Result<Vec<HashValue>,BulletinBoardError>;
    /// Get the most recently published root, should it exist.
    fn get_most_recent_published_root(&self) -> Result<Option<HashValue>,BulletinBoardError>;
    /// Get all leaves and branches without a parent branch. Published nodes do not count as a parent.
    /// This is used to recompute the starting state.
    fn get_all_leaves_and_branches_without_a_parent(&self) -> Result<Vec<HashValue>,BulletinBoardError>;
    /// given a hash, get information about what it represents, if anything.
    fn get_hash_info(&self, query:HashValue) -> Result<Option<HashInfo>,BulletinBoardError>;

    /// Store a transaction in the database.
    fn publish(&mut self,transaction:&DatabaseTransaction) -> Result<(),BulletinBoardError>;

    /// Remove the text associated with a leaf.
    fn censor_leaf(&mut self,leaf_to_censor:HashValue) -> Result<(),BulletinBoardError>;

    /// Get the depth of a subtree rooted at a given leaf or branch node) by following elements down the left side of each branch.
    /// A leaf node has depth 0.
    /// A branch node has depth 1 or more.
    ///
    /// The default implementation is to repeatedly call get_hash_info depth times; this is usually adequate as this is only used during startup.
    fn left_depth(&self,hash:HashValue) -> Result<usize,BulletinBoardError> {
        let mut res = 0;
        let mut hash = hash;
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
    fn compute_current_forest(&self) -> Result<GrowingForest,BulletinBoardError> {
        GrowingForest::new(&self.get_all_leaves_and_branches_without_a_parent()?,|h|self.left_depth(h))
    }

}

fn bb_timestamp_now() -> Result<Timestamp, BulletinBoardError> {
    timestamp_now().map_err(|_|BulletinBoardError::ClockError)
}

impl <B:BulletinBoardBackend> BulletinBoard<B> {

    /// called when the current_forest field is corrupt. Make it valid, if possible.
    fn reload_current_forest(&mut self) -> Result<(),BulletinBoardError> {
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
    fn submit_leaf_work(&mut self,data:String) -> Result<HashValue,BulletinBoardError> {
        let history = LeafHashHistory{ timestamp: bb_timestamp_now()?, data: Some(data) };
        let new_hash = history.compute_hash().unwrap();
        match self.backend.get_hash_info(new_hash)? {
            Some(HashInfo{source:HashSource::Leaf(other_history), .. }) if other_history==history => {
                Err(BulletinBoardError::IdenticalDataAlreadySubmitted)
            }
            Some(hash_collision) => { // The below case is absurdly unlikely to happen.
                eprintln!("Time to enter the lottery! Actually you have probably won without entering. You have just done a submission and found a hash collision between {:?} and {:?}",&hash_collision,&history);
                std::thread::sleep(Duration::from_secs(1)); // work around - wait a second and retry, with a new timestamp.
                self.submit_leaf_work(history.data.unwrap())
            }
            _ =>  { // no hash collision, all is good. Should go here 99.99999999999999999999999999999..% of the time.
                let mut transaction = DatabaseTransaction::default();
                transaction.add_leaf_hash(new_hash,history);
                self.current_forest.as_mut().ok_or_else(||BulletinBoardError::CouldNotInitializeFromDatabase)?.add_leaf(new_hash, &self.backend, &mut transaction)?;
                self.backend.publish(&transaction)?;
                Ok(new_hash)
            }
        }
    }


    /// Submit some data to be included in the bulletin board, and get back a HashValue that the
    /// board commits to having in the history.
    /// Note that if the same data is submitted twice in the same second it will return an error (as this probably is)
    ///
    /// # Example
    ///
    /// ```
    /// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(
    ///     merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
    /// board.submit_leaf("A").unwrap();
    /// // the board now has one leaf!
    ///```
    pub fn submit_leaf(&mut self,data:&str) -> Result<HashValue,BulletinBoardError> {
        let res = self.submit_leaf_work(data.to_string());
        if res.is_err() { self.reload_current_forest()? }
        res
    }

    /// Create a new bulletin board from a backend.
    pub fn new(backend:B) -> Result<Self,BulletinBoardError> {
        let mut res = BulletinBoard { backend, current_forest : None };
        res.reload_current_forest()?;
        Ok(res)
    }

    /// Get a valid forest reference, or an error.
    fn forest_or_err(&self) -> Result<&GrowingForest,BulletinBoardError> {
        self.current_forest.as_ref().ok_or_else(||BulletinBoardError::CouldNotInitializeFromDatabase)
    }

    /// Get the current published head that everyone knows. Everyone who is paying attention, that is. And who can remember 256 bits of gibberish.
    ///
    /// # Example
    ///
    /// ```
    /// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(
    ///     merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
    /// assert_eq!(None,board.get_most_recent_published_root().unwrap());
    /// board.submit_leaf("A").unwrap();
    /// let hash1 = board.order_new_published_root().unwrap();
    /// assert_eq!(Some(hash1),board.get_most_recent_published_root().unwrap());
    /// board.submit_leaf("B").unwrap();
    /// assert_eq!(Some(hash1),board.get_most_recent_published_root().unwrap());
    /// let hash2 = board.order_new_published_root().unwrap();
    /// assert_eq!(Some(hash2),board.get_most_recent_published_root().unwrap());
    ///```
    pub fn get_most_recent_published_root(&self) -> Result<Option<HashValue>,BulletinBoardError> {
        self.backend.get_most_recent_published_root()
    }

    /// Get a list of all published roots, ordered oldest to newest.
    ///
    /// # Example
    ///
    /// ```
    /// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(
    ///     merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
    /// assert!(board.get_all_published_roots().unwrap().is_empty());
    /// board.submit_leaf("A").unwrap();
    /// let hash1 = board.order_new_published_root().unwrap();
    /// assert_eq!(vec![hash1],board.get_all_published_roots().unwrap());
    /// board.submit_leaf("B").unwrap();
    /// assert_eq!(vec![hash1],board.get_all_published_roots().unwrap());
    /// let hash2 = board.order_new_published_root().unwrap();
    /// assert_eq!(vec![hash1,hash2],board.get_all_published_roots().unwrap());
    ///```
    pub fn get_all_published_roots(&self) -> Result<Vec<HashValue>,BulletinBoardError> {
        self.backend.get_all_published_roots()
    }

    /// Get the currently committed to, but not yet published, hash values.
    /// Equivalently, get all branches and leaves that do not have parents, and which are not included in the last published root.
    ///
    /// # Example
    ///
    /// ```
    /// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(
    ///     merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
    /// assert!(board.get_parentless_unpublished_hash_values().unwrap().is_empty());
    /// let hash = board.submit_leaf("A").unwrap();
    /// assert_eq!(vec![hash],board.get_parentless_unpublished_hash_values().unwrap());
    /// board.order_new_published_root().unwrap();
    /// assert!(board.get_parentless_unpublished_hash_values().unwrap().is_empty());
    /// board.submit_leaf("B").unwrap();
    /// assert_eq!(1,board.get_parentless_unpublished_hash_values().unwrap().len());
    ///   // will be the tree formed from "A" and "B", not "B" itself.
    ///```
    pub fn get_parentless_unpublished_hash_values(&self) -> Result<Vec<HashValue>,BulletinBoardError> {
        let mut currently_used : Vec<HashValue> = self.forest_or_err()?.get_subtrees();
        if let Some(published_root) = self.backend.get_most_recent_published_root()? {
            if let Some(HashInfo{source:HashSource::Root(history),..}) = self.backend.get_hash_info(published_root)? {
                currently_used.retain(|h|!history.elements.contains(h)) // remove already published elements.
            } else { return Err(BulletinBoardError::PublishedRootHasNoInfo) }
        }
        Ok(currently_used)
    }

    /// Request a new published root. This will contain a reference to each tree in
    /// the current forest. That is, each leaf or branch node that doesn't have a parent.
    /// This will return an error if called twice in rapid succession (same timestamp) with nothing added in the meantime, as it would otherwise produce the same hash, and is almost certainly not what was intended anyway.
    pub fn order_new_published_root(&mut self) -> Result<HashValue,BulletinBoardError> {
        let history = RootHashHistory { timestamp: bb_timestamp_now()?, elements: self.forest_or_err()?.get_subtrees(), prior : self.get_most_recent_published_root()? };
        let new_hash = history.compute_hash();
        match self.backend.get_hash_info(new_hash)? {
            Some(HashInfo{source:HashSource::Root(other_history), .. }) if other_history==history => {
                Err(BulletinBoardError::PublishingNewRootInstantlyAfterLastRoot)
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
    ///
    /// # Example
    ///
    /// ```
    /// use merkle_tree_bulletin_board::hash_history::{HashSource, LeafHashHistory};
    ///
    /// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(
    ///     merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
    /// let hash = board.submit_leaf("A").unwrap();
    /// let info = board.get_hash_info(hash).unwrap();
    /// assert_eq!(info.parent,None);
    /// match info.source {
    ///         HashSource::Leaf(LeafHashHistory{data:Some(d),timestamp:_}) => assert_eq!(d,"A"),
    ///         _ => panic!("Not a leaf"),
    /// }
    /// ```
    pub fn get_hash_info(&self, query:HashValue) -> Result<HashInfo,BulletinBoardError> {
        self.backend.get_hash_info(query)?.ok_or_else(||BulletinBoardError::NoSuchHash)
    }


    /// Convenience method to get a whole proof chain at once, that is, the chain
    /// from the provided hashvalue back to the most recent published root.
    ///
    /// This could easily be done via multiple calls
    /// to the other APIs, and indeed that is how this is implemented.
    ///
    /// See [verifier::verify_proof] for how to verify the proof.
    ///
    /// # Example
    ///
    /// ```
    /// use merkle_tree_bulletin_board::hash_history::{HashSource, BranchHashHistory};
    /// use merkle_tree_bulletin_board::verifier::verify_proof;
    ///
    /// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(
    ///     merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
    /// let hash_a = board.submit_leaf("a").unwrap();
    /// let hash_b = board.submit_leaf("b").unwrap(); // made a branch out of a and b
    /// let branch = board.get_parentless_unpublished_hash_values().unwrap()[0];
    /// let root = board.order_new_published_root().unwrap();
    /// let proof = board.get_proof_chain(hash_a).unwrap(); // get the inclusion proof for "a".
    /// assert_eq!(proof.published_root.clone().unwrap().hash,root); // the proof is for this root
    /// match &proof.published_root.as_ref().unwrap().source {
    ///     HashSource::Root(history) => assert_eq!(history.elements,vec![branch]),
    ///     _ => panic!("Not root")
    /// }
    /// assert_eq!(proof.chain.len(),2);
    /// assert_eq!(proof.chain[0].hash,hash_a);  // the leaf we asked for
    /// assert_eq!(proof.chain[1].hash,branch);  // the parent of the leaf we asked for
    ///   // the chain does not continue up as chain[1].hash is in the published root.
    /// assert_eq!(proof.chain[1].source,
    ///     HashSource::Branch(BranchHashHistory{left: hash_a,right: hash_b}));
    /// assert_eq!(verify_proof("a",root,&proof),None); // A thorough check.
    /// ```
   pub fn get_proof_chain(&self,query:HashValue) -> Result<FullProof,BulletinBoardError> {
        let mut chain = vec![];
        let mut node = query;
        let mut published_root : Option<HashInfoWithHash> =  {
            if let Ok(Some(published_root_hash)) = self.get_most_recent_published_root() {
                if let Ok(node_info) = self.get_hash_info(published_root_hash) {
                    Some(node_info.add_hash(published_root_hash))
                } else { return Err(BulletinBoardError::ProofChainCorruptMissingPublishedNode(published_root_hash)); } // There is a break in the logic!!!
            } else {None }
        };
        let published: HashSet<HashValue> = {
            if let Some(info) = &published_root {
                if let HashSource::Root(history) = &info.source {
                    HashSet::from_iter(history.elements.iter().cloned())
                } else { return Err(BulletinBoardError::PublishedRootIsNotARoot(node)); } // There is a break in the logic!!!
            } else { HashSet::default() }
        };
        loop {
            if let Ok(node_info) = self.get_hash_info(node) {
                chain.push(node_info.add_hash(node));
                if published.contains(&node) { break; }
                match node_info.parent {
                    Some(parent) => node=parent,
                    None => {
                        published_root=None; // got to the end of the line without finding something in the published root.
                        break
                    },
                }
            } else {
                return Err(if query==node { BulletinBoardError::NoSuchHash } else { BulletinBoardError::ProofChainCorruptMissingPublishedNode(node)});
            } // There is a break in the logic!!!
        }
        Ok(FullProof{ chain, published_root })
    }

    /// Censor a leaf!
    ///
    /// The system allows censorship of individual leaves. This is obviously generally undesirable and
    /// to some extent undermines some of the point of a committed bulletin board. However,
    /// most if not all countries have censorship laws that require operators of bulletin boards
    /// to do censorship, regardless of whether or not you agree with such laws. The system is
    /// designed to have the following properties in the presence of censorship:
    ///  * Censorship does not invalidate any published root.
    ///  * Censorship, even post a published root, does not invalidate or even affect any proof chain
    ///    other than the particular leaf being censored.
    ///  * Even the leaf being censored can still be verified should you happen to know
    ///    what had been present before the censorship.
    ///  * It is impossible to hide the fact that a censorship post published root has occurred.
    ///  * If you do not use the censorship feature, then the overhead of having it present is negligible.
    ///
    /// Censorship is accomplished by simply not providing the censored text; the leaf and its
    /// associated hash value (and timestamp) are still present. The hash value cannot be modified
    /// post published root, as that would invalidate the parent branch. The timestamp cannot
    /// however be verified unless you happen to know the uncensored text.
    ///
    /// # Example
    ///
    /// ```
    /// use merkle_tree_bulletin_board::hash_history::{HashSource, LeafHashHistory};
    ///
    /// let mut board = merkle_tree_bulletin_board::BulletinBoard::new(
    ///     merkle_tree_bulletin_board::backend_memory::BackendMemory::default()).unwrap();
    /// let hash = board.submit_leaf("A").unwrap();
    /// // get uncensored leaf
    /// let info = board.get_hash_info(hash).unwrap();
    /// assert_eq!(info.parent,None);
    /// match info.source {
    ///         HashSource::Leaf(LeafHashHistory{data:Some(d),timestamp:_}) => assert_eq!(d,"A"),
    ///         _ => panic!("Not an uncensored leaf"),
    /// }
    ///
    /// board.censor_leaf(hash).unwrap();
    /// // get censored leaf. Identical except data is missing.
    /// let info = board.get_hash_info(hash).unwrap();
    /// assert_eq!(info.parent,None);
    /// match info.source {
    ///         HashSource::Leaf(LeafHashHistory{data:None,timestamp:_}) => {}
    ///         _ => panic!("Not a censored leaf"),
    /// }
    /// ```
    pub fn censor_leaf(&mut self,leaf_to_censor:HashValue) -> Result<(),BulletinBoardError> {
        self.backend.censor_leaf(leaf_to_censor)
    }
}
