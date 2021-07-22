//! A journalling backend for the database based on csv files.

use std::path::PathBuf;
use crate::{DatabaseTransaction, BulletinBoardBackend};
use crate::hash_history::{HashSource, HashInfo};
use crate::hash::HashValue;
use std::fs::{OpenOptions, File};
use anyhow::anyhow;
use crate::backend_flatfile::{write_transaction_to_csv, TransactionIterator};
use std::collections::HashSet;
use std::iter::FromIterator;

/// Add journalling suitable for bulk verification to some other backend.
///
/// This creates text files in the format used by [write_transaction_to_csv] which can be
/// read by someone trying to do a bulk verification of the bulletin board.
///
/// New transactions are appended into a file called "pending.csv". When a new root is published,
/// pending.csv is renamed to a filename consisting of the new root hash, followed by ".csv".
///
/// There are a variety of things a verifier may wish to check:
///   - That a particular hash is included in a particular publication. This
///     is quickly and easily handled with the [BulletinBoard::get_proof_chain] function.
///   - The complete list of actions between two consecutive published roots *R* and *S*.
///     - Get information on root *R* with [BulletinBoard::get_hash_info], and check its hash.
///       The nodes in this will be the "starting" condition for running this algorithm. If you want to
///       verify the balanced property of the tree (which is important only for performance),
///       then you can also get their depth via repeated [BulletinBoard::get_hash_info].
///     - Get the file *S*.csv which contains all transactions between *R* and *S* (assuming *R* is the root before *S*)
///     - Play said file, checking all the hashings it implies, and whatever else you want to check about the content (content specific).
///     - Compare the resulting nodes to [BulletinBoard::get_hash_info] applied to *S* and check its hash.
///   - The complete list of current actions between now and the last published root *R*. There
///     is not much point in doing this for security purposes, since different people could
///     be given current states by a malicious adversary. However it is worth doing from a file
///     consistency point of view; the Journalling backend actually does this on start up to make
///     sure that the current.csv file is consistent with the underlying database. This is to detect
///     file corruption in case the prior shutdown was not clean.
///     - Get information on root *R* as above.
///     - Get the file current.csv, as above. If the file does not exist, treat it as empty.
///     - Play said file, as above.
///     - Compare the resulting nodes to [BulletinBoard::get_pending_hash_values], remembering that
///       that does not include nodes that were in *R*.
///   - The entire transcript from the beginning of time.
///     - Get a list of published roots via [BulletinBoard::get_all_published_roots]
///     - Iterate the above steps for each consecutive pair of roots.
pub struct BackendJournal<B:BulletinBoardBackend> {
    main_journal: B,
    directory : PathBuf,
}

impl <B:BulletinBoardBackend> BulletinBoardBackend for BackendJournal<B> {
    fn get_all_published_roots(&self) -> anyhow::Result<Vec<HashValue>> { self.main_journal.get_all_published_roots()  }

    fn get_most_recent_published_root(&self) -> anyhow::Result<Option<HashValue>> { self.main_journal.get_most_recent_published_root() }

    fn get_all_leaves_and_branches_without_a_parent(&self) -> anyhow::Result<Vec<HashValue>> { self.main_journal.get_all_leaves_and_branches_without_a_parent() }

    fn get_hash_info(&self, query: HashValue) -> anyhow::Result<Option<HashInfo>> { self.main_journal.get_hash_info(query) }

    /// Publish both to the original backend, and the journal.
    /// The original is published to first; this means that in the case of an unfortunate power loss or similar, the journal may miss the last record.
    fn publish(&mut self, transaction: &DatabaseTransaction) -> anyhow::Result<()> {
        self.main_journal.publish(transaction)?;
        {
            let file = OpenOptions::new().append(true).create(true).open(&self.pending_path())?;
            write_transaction_to_csv(&transaction,&file)?;
            file.sync_data()?;
        }
        fn is_root(source:&HashSource) -> bool {
            match source {
                HashSource::Root(_) => true,
                _ => false,
            }
        }
        if let Some((last_hash,last_source)) = &transaction.pending.last() {
            if is_root(last_source) {
                // this is a published root.
                if self.pending_path().exists() {
                    std::fs::rename(&self.pending_path(),&self.hash_path(*last_hash))?;
                } else {
                    OpenOptions::new().append(true).create(true).open(&self.hash_path(*last_hash))?; // create a blank file.
                }
            }
        }
        Ok(())
    }
}

impl <B:BulletinBoardBackend> BackendJournal<B> {

    /// The path of a file named name
    fn rel_path(&self,name:&str) -> PathBuf {
        let mut res = self.directory.clone();
        res.push(name);
        res
    }

    /// the path of the pending data file.
    fn pending_path(&self) -> PathBuf { self.rel_path("pending.csv") }

    /// the path for a given hash
    fn hash_path(&self,hash:HashValue) -> PathBuf { self.rel_path(&(hash.to_string()+".csv")) }

    /// Verify that the pending data file is consistent with the database. Return Err if not.
    /// This is *not* a security check, it does *not* check any hashes. Rather it checks that
    /// there are no transactions missing.
    ///
    /// It does this by starting with the nodes listed in the last published root, should it exist.
    /// Then iterate through each transaction in the pending list, keeping track of which hashes
    /// should have no parents. As each transaction changes this list other than a root publication
    /// (which should not be in the pending file anyway), this is a reasonable check for a truncated
    /// pending file, which may have resulted from a bad shutdown.
    pub fn verify_current_consistent(&self) -> anyhow::Result<()> {
        // First get the nodes that are left over from the last publication.
        let preexisting_nodes : Vec<HashValue> = if let Some(last_root) = self.main_journal.get_most_recent_published_root()? {
            if !std::path::Path::new(&self.hash_path(last_root)).exists() { return Err(anyhow!("Last published root hash does not exist")); }
            match self.main_journal.get_hash_info(last_root)?.unwrap() {
                HashInfo { source: HashSource::Root(history), .. } => history.elements,
                _ => return Err(anyhow!("Last published root hash is not a root")),
            }
        } else {vec![]};
        let mut current_nodes : HashSet<HashValue> = HashSet::from_iter(preexisting_nodes.into_iter());
        // process the pending file.
        if let Ok(file_reader) = File::open(&self.pending_path()) { // file may not exist.
            for transaction in TransactionIterator::new(file_reader) {
                for (hash,source) in transaction?.pending {
                    current_nodes.insert(hash);
                    match source {
                        HashSource::Leaf(_) => { }
                        HashSource::Branch(history) => {
                            if !current_nodes.remove(&history.left) { return Err(anyhow!("Pending file contains a branch with unexpected left hash {}",history.left)); }
                            if !current_nodes.remove(&history.right) { return Err(anyhow!("Pending file contains a branch with unexpected right hash {}",history.right)); }
                        }
                        HashSource::Root(_) => return Err(anyhow!("Pending file contains a root")),
                    }
                }
            }
        }
        let expected : Vec<HashValue> = self.main_journal.get_all_leaves_and_branches_without_a_parent()?;
        let expected_set : HashSet<HashValue> = HashSet::from_iter(expected.into_iter());
        if expected_set != current_nodes { return Err(anyhow!("Expecting to get {:#?} as nodes without parents; actually got {:#?}.",&expected_set,&current_nodes))}
        Ok(())
    }

    /// Add journalling to an existing backend, keeping journals in the provided directory (which may not exist).
    ///
    /// This will create the directory if it does not exist, and run [BackendJournal::verify_current_consistent] to
    /// check that the pending file has not been truncated. If that fails, and recreate_current_if_corrupt is true,
    /// it will be recreated via [deduce_journal::deduce_journal_last_published_root_to_present].
    pub fn new<P>(main_journal:B,directory: P,recreate_current_if_corrupt:bool) -> anyhow::Result<Self>
    where PathBuf: From<P>
    {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        let res = BackendJournal{ main_journal, directory };
        match res.verify_current_consistent() {
            Ok(()) => {}
            Err(e) if recreate_current_if_corrupt => {
                println!("The pending journal is corrupt. Attempting to recreate. Error was {}",e);
                let should_be = crate::deduce_journal::deduce_journal_last_published_root_to_present(&res)?;
                let recreate_name = res.rel_path("recreating.csv");
                { // make the file in a different name to prevent clobbering something of possible diagnostic use if all is stuffed up to badly to recover.
                    let file = File::create(&recreate_name)?;
                    for transaction in should_be {
                        write_transaction_to_csv(&transaction,&file)?;
                    }
                    file.sync_data()?;
                }
                std::fs::rename(recreate_name,res.pending_path())?;
                println!("Successfully recreated pending journal. Continuing.")
            }
            Err(e) => return Err(e)
        }
        Ok(res)
    }
}

