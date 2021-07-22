use crate::{BulletinBoardBackend, DatabaseTransaction};
use crate::hash::HashValue;
use crate::growing_forest::{GrowingForest, HashAndDepth};
use crate::hash_history::{HashInfo, HashSource, RootHashHistory};
use anyhow::anyhow;

/// Deduce the set of transactions needed to go from state 'from' to state 'to'
/// where the states are the list of leaves or branches without parents.
///
/// If a desired state is
///  - The beginning of time, this is the empty vector
///  - A published root, this is the list of nodes in the root
///  - The present, this is the current list of leaves or branches without parents.
///
/// Note that this journal will not include root publishing, just leaves and branches.
///
/// This works by deducing the state of the [GrowingForest] at the *to* state, and then
/// working back until the from state is recovered, on the basis that hash creation involves
/// the creation of the last element in the [GrowingForest], and each set of hash creations starting
/// with a leaf creation is a single transaction.
///
/// You will probably want to use specialized versions, [deduce_journal_last_published_root_to_present]
/// and [deduce_journal_from_prior_root_to_given_root]
///
/// # Example
/// ```
/// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
/// use merkle_tree_bulletin_board::{BulletinBoard, DatabaseTransaction, BulletinBoardBackend};
/// use merkle_tree_bulletin_board::deduce_journal::deduce_journal;
///
/// let backend = BackendMemory::default();
/// let mut board = BulletinBoard::new(backend).unwrap();
/// board.submit_leaf("A").unwrap();
/// board.submit_leaf("B").unwrap();
/// board.submit_leaf("C").unwrap();
/// board.order_new_published_root().unwrap();
/// board.submit_leaf("D").unwrap();
/// let journal : Vec<DatabaseTransaction> = deduce_journal(&board.backend,&vec![],&board.backend.get_all_leaves_and_branches_without_a_parent().unwrap()).unwrap();
/// assert_eq!(journal.len(),4); // published roots are not included!
/// assert_eq!(journal[0].pending.len(),1); // just publish the "A" leaf.
/// assert_eq!(journal[1].pending.len(),2); // publish "B", make a tree with "A".
/// assert_eq!(journal[2].pending.len(),1); // publish "C"
/// assert_eq!(journal[3].pending.len(),3); // publish "D", make a tree with "C", make a tree with "AB".
/// ```

pub fn deduce_journal(board:&impl BulletinBoardBackend,from:&Vec<HashValue>,to:&Vec<HashValue>) -> anyhow::Result<Vec<DatabaseTransaction>> {
    let mut res = vec![];
    let from_last = GrowingForest::new(from,|h|board.left_depth(h))?.last();
    let mut work = GrowingForest::new(to,|h|board.left_depth(h))?;
    let mut current_trans : Vec<(HashValue,HashSource)> = vec![];
    while work.last() != from_last {
        if let Some(HashAndDepth{ hash, depth }) = work.forest.pop() {
            match board.get_hash_info(hash)?.ok_or_else(||anyhow!("Hash {} does not have any info",hash))?.source {
                HashSource::Leaf(history) => { // undo the leaf. This is the start of a transaction.
                    let mut transaction = DatabaseTransaction::default();
                    transaction.pending.push((hash,HashSource::Leaf(history)));
                    while let Some(branch) = current_trans.pop() { transaction.pending.push(branch); }
                    res.push(transaction);
                }
                HashSource::Branch(history) => { // undo the branch. This is the continuation of a transaction.
                    work.forest.push(HashAndDepth{hash:history.left,depth:depth-1 });
                    work.forest.push(HashAndDepth{hash:history.right,depth:depth-1 });
                    current_trans.push((hash,HashSource::Branch(history)));
                }
                HashSource::Root(_) => { return Err(anyhow!("Should not have a root {} in a growing forest",hash)); }
            }
        } else { return Err(anyhow!("Can't get from {:#?} to {:#?}",from,to));}
    }
    if !current_trans.is_empty() { return Err(anyhow!("Initial state from starts in the middle of branch creation : {:#?} ",from));}
    res.reverse();
    Ok(res)
}

/// Get the hashes for the given root, should it exist. If not, empty vec.
fn get_hashes_for_optional_root(board:&impl BulletinBoardBackend,root:Option<HashValue>) -> anyhow::Result<Vec<HashValue>> {
    if let Some(root) = root {
        match board.get_hash_info(root)? {
            Some(HashInfo{source:HashSource::Root(RootHashHistory{ elements,.. }),..}) => Ok(elements),
            _ => Err(anyhow!("{} is not a root",root))
        }
    } else { Ok(vec![]) }
}

/// A special case of [deduce_journal] that gets the journal from the last published hash to the present day.
/// # Example
///
/// ```
/// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
/// use merkle_tree_bulletin_board::{BulletinBoard, DatabaseTransaction, BulletinBoardBackend};
/// use merkle_tree_bulletin_board::deduce_journal::deduce_journal_last_published_root_to_present;
///
/// let backend = BackendMemory::default();
/// let mut board = BulletinBoard::new(backend).unwrap();
/// board.submit_leaf("A").unwrap();
/// board.submit_leaf("B").unwrap();
/// board.submit_leaf("C").unwrap();
/// board.order_new_published_root().unwrap();
/// let d = board.submit_leaf("D").unwrap();
/// let journal : Vec<DatabaseTransaction> = deduce_journal_last_published_root_to_present(&board.backend).unwrap();
/// assert_eq!(journal.len(),1); // just after the published root
/// assert_eq!(journal[0].pending.len(),3); // publish "D", make a tree with "C", make a tree with "AB".
/// assert_eq!(journal[0].pending[0].0,d); // publish D
/// ```
pub fn deduce_journal_last_published_root_to_present(board:&impl BulletinBoardBackend) -> anyhow::Result<Vec<DatabaseTransaction>> {
    deduce_journal(board,&get_hashes_for_optional_root(board,board.get_most_recent_published_root()?)?,&board.get_all_leaves_and_branches_without_a_parent()?)
}

/// A special case of [deduce_journal] that gets the journal from the last published hash to the present day.
///
/// # Example
///
/// ```
/// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
/// use merkle_tree_bulletin_board::{BulletinBoard, DatabaseTransaction, BulletinBoardBackend};
/// use merkle_tree_bulletin_board::deduce_journal::deduce_journal_from_prior_root_to_given_root;
///
/// let backend = BackendMemory::default();
/// let mut board = BulletinBoard::new(backend).unwrap();
/// board.submit_leaf("A").unwrap();
/// board.submit_leaf("B").unwrap();
/// board.submit_leaf("C").unwrap();
/// let published = board.order_new_published_root().unwrap();
/// board.submit_leaf("D").unwrap();
/// let journal : Vec<DatabaseTransaction> = deduce_journal_from_prior_root_to_given_root(&board.backend,published).unwrap();
/// assert_eq!(journal.len(),4); // published roots is included!
/// assert_eq!(journal[0].pending.len(),1); // just publish the "A" leaf.
/// assert_eq!(journal[1].pending.len(),2); // publish "B", make a tree with "A".
/// assert_eq!(journal[2].pending.len(),1); // publish "C"
/// assert_eq!(journal[3].pending.len(),1); // root
/// ```
pub fn deduce_journal_from_prior_root_to_given_root(board:&impl BulletinBoardBackend,root:HashValue) -> anyhow::Result<Vec<DatabaseTransaction>> {
    match board.get_hash_info(root)? {
        Some(HashInfo{source: HashSource::Root(RootHashHistory{ elements,prior,timestamp }),..}) => {
            let mut journal = deduce_journal(board,&get_hashes_for_optional_root(board,prior)?,&elements)?;
            let mut root_transaction = DatabaseTransaction::default();
            root_transaction.pending.push((root,HashSource::Root(RootHashHistory{ elements,prior,timestamp })));
            journal.push(root_transaction);
            Ok(journal)
        },
        _ => Err(anyhow!("{} is not a root",root))
    }
}


