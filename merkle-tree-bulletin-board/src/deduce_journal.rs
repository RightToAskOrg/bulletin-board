use crate::{BulletinBoardBackend, DatabaseTransaction};
use crate::hash::HashValue;
use crate::growing_forest::{GrowingForest, HashAndDepth};
use crate::hash_history::{HashInfo, HashSource, RootHashHistory};
use anyhow::anyhow;
use std::collections::HashMap;

/// Deduce the set of transactions needed to go from state 'from' to state 'to'
/// where the states are the list of leaves or branches without parents.
///
/// If a desired state is
///  - The beginning of time, this is the empty vector
///  - A published root, this is the list of nodes in the root
///  - The present, this is the current list of leaves or branches without parents.
///
/// That this journal will include root publishing iff include_published_roots is true.
/// You should be careful using this, as you may get more than you expect as there may be
/// multiple published roots with the same state. The roots included will be inclusive of
/// the from and to.
///
/// This works by deducing the state of the [GrowingForest] at the *to* state, and then
/// working back until the from state is recovered, on the basis that hash creation involves
/// the creation of the last element in the [GrowingForest], and each set of hash creations starting
/// with a leaf creation is a single transaction.
///
/// You will probably want to use specialized versions, [deduce_journal_last_published_root_to_present]
/// and [deduce_journal_from_prior_root_to_given_root]
///
/// # Examples
///
/// Not including roots
///
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
/// let journal : Vec<DatabaseTransaction> = deduce_journal(&board.backend,&vec![],
///     &board.backend.get_all_leaves_and_branches_without_a_parent().unwrap(),false).unwrap();
/// assert_eq!(journal.len(),4); // published roots are not included!
/// assert_eq!(journal[0].pending.len(),1); // just publish the "A" leaf.
/// assert_eq!(journal[1].pending.len(),2); // publish "B", make a tree with "A".
/// assert_eq!(journal[2].pending.len(),1); // publish "C"
/// assert_eq!(journal[3].pending.len(),3); // publish "D", make a tree with "C", make a tree with "AB".
/// ```
///
/// Including roots
/// ```
/// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
/// use merkle_tree_bulletin_board::{BulletinBoard, DatabaseTransaction, BulletinBoardBackend};
/// use merkle_tree_bulletin_board::deduce_journal::deduce_journal;
///
/// let backend = BackendMemory::default();
/// let mut board = BulletinBoard::new(backend).unwrap();
/// board.order_new_published_root().unwrap();
/// board.submit_leaf("A").unwrap();
/// board.submit_leaf("B").unwrap();
/// board.submit_leaf("C").unwrap();
/// board.order_new_published_root().unwrap();
/// board.submit_leaf("D").unwrap();
/// board.order_new_published_root().unwrap();
/// let journal : Vec<DatabaseTransaction> = deduce_journal(&board.backend,&vec![],
///     &board.backend.get_all_leaves_and_branches_without_a_parent().unwrap(),true).unwrap();
/// assert_eq!(journal.len(),7); // published roots are included!
/// assert_eq!(journal[0].pending.len(),1); // published root
/// assert_eq!(journal[1].pending.len(),1); // just publish the "A" leaf.
/// assert_eq!(journal[2].pending.len(),2); // publish "B", make a tree with "A".
/// assert_eq!(journal[3].pending.len(),1); // publish "C"
/// assert_eq!(journal[4].pending.len(),1); // published root
/// assert_eq!(journal[5].pending.len(),3); // publish "D", make a tree with "C", make a tree with "AB".
/// assert_eq!(journal[6].pending.len(),1); // published root
/// ```

pub fn deduce_journal(board:&impl BulletinBoardBackend,from:&Vec<HashValue>,to:&Vec<HashValue>,include_published_roots:bool) -> anyhow::Result<Vec<DatabaseTransaction>> {
    let mut res = vec![];
    let from_last = GrowingForest::new(from,|h|board.left_depth(h))?.last();
    let mut work = GrowingForest::new(to,|h|board.left_depth(h))?;
    let mut current_trans : Vec<(HashValue,HashSource)> = vec![];
    let mut at_very_start : Vec<DatabaseTransaction> = vec![];
    let mut check_for_published_roots : HashMap<HashValue,Vec<DatabaseTransaction>> = if include_published_roots {
        let mut check : HashMap<HashValue,Vec<DatabaseTransaction>> = Default::default();
        for root in board.get_all_published_roots()? {
            let info = board.get_hash_info(root)?.unwrap();
            match &info.source {
                HashSource::Root(RootHashHistory{elements,..}) => {
                    if let Some(last) = elements.last() {
                        check.entry(*last).or_default().push(DatabaseTransaction::singleton(root,info.source))
                    } else { // published a root before anything else!
                        if from_last.is_none() { // want from the start
                            at_very_start.push(DatabaseTransaction::singleton(root,info.source));
                        }
                    }
                }
                _ => return Err(anyhow!("Claimed root {} is not a root",root))
            }
        }
        check
    } else { HashMap::default() };
    while { // somewhat cumbersome while loop as want the following code executed at both the start and end inclusive. But the alternative, repeated code, will lead to a plague of zombies or worse, a bug!
        if include_published_roots {
            if let Some(last) = work.last() {
                if let Some(mut roots) = check_for_published_roots.remove(&last) {
                    while let Some(e) = roots.pop() { res.push(e); }
                }
            }
        }
        work.last() != from_last // the actual condition of the while loop.
          } {
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
    while let Some(e) = at_very_start.pop() { res.push(e); }
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
    deduce_journal(board,&get_hashes_for_optional_root(board,board.get_most_recent_published_root()?)?,&board.get_all_leaves_and_branches_without_a_parent()?,false)
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
            let mut journal = deduce_journal(board,&get_hashes_for_optional_root(board,prior)?,&elements,false)?;
            journal.push(DatabaseTransaction::singleton(root,HashSource::Root(RootHashHistory{ elements,prior,timestamp })));
            Ok(journal)
        },
        _ => Err(anyhow!("{} is not a root",root))
    }
}



#[cfg(test)]
mod tests {
    use crate::backend_memory::BackendMemory;
    use crate::{BulletinBoard, DatabaseTransaction, BulletinBoardBackend};
    use crate::deduce_journal::deduce_journal;

    #[test]
    /// Test deducing journals with the roots in the correct order.
    fn test_deduce_root_order() {
        let mut board = BulletinBoard::new(BackendMemory::default()).unwrap();
        board.order_new_published_root().unwrap();
        board.order_new_published_root().unwrap();
        board.submit_leaf("A").unwrap();
        board.submit_leaf("B").unwrap();
        board.submit_leaf("C").unwrap();
        board.order_new_published_root().unwrap();
        board.order_new_published_root().unwrap();
        board.submit_leaf("D").unwrap();
        board.order_new_published_root().unwrap();
        board.order_new_published_root().unwrap();
        let order_roots = board.get_all_published_roots().unwrap();

        let journal : Vec<DatabaseTransaction> = deduce_journal(&board.backend,&vec![],
             &board.backend.get_all_leaves_and_branches_without_a_parent().unwrap(),true).unwrap();
        assert_eq!(journal.len(),10);
        // play these transactions back.
        let mut new_backend = BackendMemory::default();
        for transaction in journal {
            new_backend.publish(&transaction).unwrap();
        }
        let new_order_roots = new_backend.get_all_published_roots().unwrap();
        assert_eq!(order_roots,new_order_roots);
    }
}