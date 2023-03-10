//! A journalling backend for the database based on csv files.

use std::path::PathBuf;
use crate::{DatabaseTransaction, BulletinBoardBackend, BulletinBoardError};
use crate::hash_history::{HashSource, HashInfo};
use crate::hash::HashValue;
use std::fs::{OpenOptions, File};
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
///     is quickly and easily handled with the [crate::BulletinBoard::get_proof_chain] function.
///   - The complete list of actions between two consecutive published roots *R* and *S*.
///     - Get information on root *R* with [crate::BulletinBoard::get_hash_info], and check its hash.
///       The nodes in this will be the "starting" condition for running this algorithm. If you want to
///       verify the balanced property of the tree (which is important only for performance),
///       then you can also get their depth via repeated [crate::BulletinBoard::get_hash_info].
///     - Get the file *S*.csv which contains all transactions between *R* and *S* (assuming *R* is the root before *S*)
///     - Play said file, checking all the hashings it implies, and whatever else you want to check about the content (content specific).
///     - Compare the resulting nodes to [crate::BulletinBoard::get_hash_info] applied to *S* and check its hash.
///     This is implemented in [crate::verifier::bulk_verify_between_two_consecutive_published_roots].
///   - The complete list of current actions between now and the last published root *R*. There
///     is not much point in doing this for security purposes, since different people could
///     be given current states by a malicious adversary. However it is worth doing from a file
///     consistency point of view; the Journalling backend actually does this on start up to make
///     sure that the current.csv file is consistent with the underlying database. This is to detect
///     file corruption in case the prior shutdown was not clean.
///     - Get information on root *R* as above.
///     - Get the file current.csv, as above. If the file does not exist, treat it as empty.
///     - Play said file, as above.
///     - Compare the resulting nodes to [crate::BulletinBoard::get_parentless_unpublished_hash_values], remembering that
///       that does not include nodes that were in *R*.
///   - The entire transcript from the beginning of time.
///     - Get a list of published roots via [crate::BulletinBoard::get_all_published_roots]
///     - Iterate the above steps for each consecutive pair of roots.
///
/// Note that the journal backend does not support censorship efficiently, and rebuilds everything.
pub struct BackendJournal<B:BulletinBoardBackend> {
    main_backend: B,
    directory : PathBuf,
}

impl <B:BulletinBoardBackend> BulletinBoardBackend for BackendJournal<B> {
    fn get_all_published_roots(&self) -> Result<Vec<HashValue>,BulletinBoardError> { self.main_backend.get_all_published_roots()  }

    fn get_most_recent_published_root(&self) -> Result<Option<HashValue>,BulletinBoardError> { self.main_backend.get_most_recent_published_root() }

    fn get_all_leaves_and_branches_without_a_parent(&self) -> Result<Vec<HashValue>,BulletinBoardError> { self.main_backend.get_all_leaves_and_branches_without_a_parent() }

    fn get_hash_info(&self, query: HashValue) -> Result<Option<HashInfo>,BulletinBoardError> { self.main_backend.get_hash_info(query) }

    /// Publish both to the original backend, and the journal.
    /// The original is published to first; this means that in the case of an unfortunate power loss or similar, the journal may miss the last record.
    fn publish(&mut self, transaction: &DatabaseTransaction) -> Result<(),BulletinBoardError> {
        self.main_backend.publish(transaction)?;
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

    /// Horrendously inefficient - rebuilds all.
    fn censor_leaf(&mut self, leaf_to_censor: HashValue) -> Result<(),BulletinBoardError> {
        // Err(anyhow!("BackendJournal does not support censorship! Keep {} free!",leaf_to_censor)) // OK, I could have just prepended leaf_to_censor by an underscore to stop the compiler complaining about leaf_to_censor not being used, and that would have produced slightly smaller code. But...I also could have just returned RTFM which would be shorter still and who would want that?
        self.main_backend.censor_leaf(leaf_to_censor)?;
        self.rebuild_all_journals()
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
    pub fn verify_current_consistent(&self) -> Result<(),BulletinBoardError> {
        // First get the nodes that are left over from the last publication.
        let preexisting_nodes : Vec<HashValue> = if let Some(last_root) = self.main_backend.get_most_recent_published_root()? {
            if !std::path::Path::new(&self.hash_path(last_root)).exists() { return Err(BulletinBoardError::BackendInconsistentError(format!("Last published root hash does not exist"))); } // TODO should these not be top level roots?
            match self.main_backend.get_hash_info(last_root)? {
                Some(HashInfo { source: HashSource::Root(history), .. }) => history.elements,
                _ => return Err(BulletinBoardError::BackendInconsistentError(format!("Last oublished root hash is not actually a root node"))),
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
                            if !current_nodes.remove(&history.left) { return Err(BulletinBoardError::BackendInconsistentError(format!("Pending file contains a branch with unexpected left hash {}",history.left))); }
                            if !current_nodes.remove(&history.right) { return Err(BulletinBoardError::BackendInconsistentError(format!("Pending file contains a branch with unexpected right hash {}",history.right))); }
                        }
                        HashSource::Root(_) => return Err(BulletinBoardError::BackendInconsistentError(format!("Pending file contains a root"))),
                    }
                }
            }
        }
        let expected : Vec<HashValue> = self.main_backend.get_all_leaves_and_branches_without_a_parent()?;
        let expected_set : HashSet<HashValue> = HashSet::from_iter(expected.into_iter());
        if expected_set != current_nodes { return Err(BulletinBoardError::BackendInconsistentError(format!("Expecting to get {:#?} as nodes without parents; actually got {:#?}.",&expected_set,&current_nodes)))}
        Ok(())
    }

    /// recreate a data file from a set of transactions.
    fn recreate(&self,name:PathBuf,should_be:Vec<DatabaseTransaction>) -> Result<(),BulletinBoardError> {
        let recreate_name = self.rel_path("recreating.csv");
        { // make the file in a different name to prevent clobbering something of possible diagnostic use if all is stuffed up to badly to recover.
            let file = File::create(&recreate_name)?;
            for transaction in should_be {
                write_transaction_to_csv(&transaction,&file)?;
            }
            file.sync_data()?;
        }
        std::fs::rename(recreate_name,&name)?;
        println!("Successfully recreated {}.",name.file_name().unwrap().to_string_lossy());
        Ok(())
    }

    /// Get the underlying backend. Used mainly for testing.
    pub fn into_inner(self) -> B { self.main_backend }

    /// Add journalling to an existing backend, keeping journals in the provided directory (which may not exist).
    ///
    /// This will create the directory if it does not exist, and run verification based on the verification
    /// flag. See [StartupVerification] for details.
    ///
    /// # Examples
    ///
    /// Normal use:
    /// ```
    /// use merkle_tree_bulletin_board::backend_journal::{BackendJournal, StartupVerification};
    /// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
    /// use merkle_tree_bulletin_board::BulletinBoard;
    /// let dir = tempdir::TempDir::new("journal").unwrap();
    /// let journal = BackendJournal::new(BackendMemory::default(),dir.path(),
    ///     StartupVerification::SanityCheckAndRepairPending).unwrap();
    /// let mut board = BulletinBoard::new(journal).unwrap();
    /// board.submit_leaf("a").unwrap();
    /// assert_eq!(true,dir.path().join("pending.csv").exists());
    /// let hash = board.order_new_published_root().unwrap();
    /// assert_eq!(false,dir.path().join("pending.csv").exists());
    /// assert_eq!(true,dir.path().join(&(hash.to_string()+".csv")).exists());
    /// board.submit_leaf("b").unwrap();
    /// assert_eq!(true,dir.path().join("pending.csv").exists());
    /// ```
    ///
    /// Showing startup verification
    ///
    /// ```
    /// use merkle_tree_bulletin_board::backend_journal::{BackendJournal, StartupVerification};
    /// use merkle_tree_bulletin_board::{DatabaseTransaction, BulletinBoardBackend};
    /// use merkle_tree_bulletin_board::hash_history::{LeafHashHistory, HashSource};
    /// use merkle_tree_bulletin_board::backend_memory::BackendMemory;
    /// let dir = tempdir::TempDir::new("journal").unwrap();
    /// let mut  journal = BackendJournal::new(BackendMemory::default(),dir.path(),
    ///     StartupVerification::SanityCheckAndRepairPending).unwrap();
    /// let history = LeafHashHistory{timestamp: 42 ,data: Some("The answer".to_string()) };
    /// let hash = history.compute_hash().unwrap();
    /// journal.publish(&DatabaseTransaction{pending:vec![(hash,HashSource::Leaf(history))]});
    /// assert_eq!(
    ///     "0,68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee,42,The answer\n\n",
    ///     std::fs::read_to_string(dir.path().join("pending.csv")).unwrap()
    /// );
    /// // Now create a new journalling backend with the old data and check it is OK.
    /// let journal = BackendJournal::new(journal.into_inner(),dir.path(),
    ///     StartupVerification::SanityCheckPending).unwrap();
    /// // Now delete the journal, and restart with the sanity-check-and-repair
    /// std::fs::remove_file(dir.path().join("pending.csv"));
    /// let journal = BackendJournal::new(journal.into_inner(),dir.path(),
    ///     StartupVerification::SanityCheckAndRepairPending).unwrap();
    /// // it should have automatically recreated the journal for us.
    /// assert_eq!(
    ///     "0,68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee,42,The answer\n\n",
    ///     std::fs::read_to_string(dir.path().join("pending.csv")).unwrap()
    /// );
    /// // Now delete, and do sanity check, but don't recreate. Should produce an error.
    /// std::fs::remove_file(dir.path().join("pending.csv"));
    /// assert!(BackendJournal::new(journal.into_inner(),dir.path(),
    ///     StartupVerification::SanityCheckPending).is_err());
    /// ```
    pub fn new<P>(main_backend:B,directory: P,verification:StartupVerification) -> Result<Self,BulletinBoardError>
    where PathBuf: From<P>
    {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        let res = BackendJournal{ main_backend, directory };
        match verification {
            StartupVerification::None => {}
            StartupVerification::SanityCheckPending => { res.verify_current_consistent()? }
            StartupVerification::SanityCheckAndRepairPending => {
                match res.verify_current_consistent() {
                    Ok(()) => {}
                    Err(e) => {
                        println!("The pending journal is corrupt. Attempting to recreate. Error was {}",e);
                        res.recreate(res.pending_path(),crate::deduce_journal::deduce_journal_last_published_root_to_present(&res)?)?;
                        res.verify_current_consistent()?; // check again, just to be sure.
                    }
                }
            }
            StartupVerification::RebuildAllJournals => {
                res.rebuild_all_journals()?;
            }
        }
        Ok(res)
    }

    fn rebuild_all_journals(&self) -> Result<(),BulletinBoardError> {
        for root in self.get_all_published_roots()?.into_iter().rev() {
            self.recreate(self.hash_path(root),crate::deduce_journal::deduce_journal_from_prior_root_to_given_root(self,root)?)?;
        }
        self.recreate(self.pending_path(),crate::deduce_journal::deduce_journal_last_published_root_to_present(self)?)?;
        self.verify_current_consistent()?; // check again, just to be sure.
        Ok(())
    }
}

/// When the journal starts up, it can do a sanity check to see if the journal is consistent with
/// the database. This is useful for recovering from power outages in the middle of disk writes, etc.
/// This enum gives options about what should be checked, and what should be done about it.
pub enum StartupVerification {
    /// Don't do any verification
    None,
    /// Check the sanity of the pending.csv file which may easily get truncated or corrupted if there is not a graceful shutdown. Uses [BackendJournal::verify_current_consistent].
    SanityCheckPending,
    /// Check the sanity of the pending.csv file like SanityCheckPending, and regenerate it automatically if needed using [crate::deduce_journal::deduce_journal_last_published_root_to_present]. This is probably the most useful in production.
    SanityCheckAndRepairPending,
    /// Something has gone badly wrong. Regenerate all journal files from the database. This may take some time...
    RebuildAllJournals,
}

#[cfg(test)]
mod tests {
    use crate::backend_journal::{BackendJournal, StartupVerification};
    use crate::backend_memory::BackendMemory;
    use crate::BulletinBoard;

    #[test]
    /// Test StartupVerification::RebuildAllJournals
    fn test_rebuild_all_journals() {
        let dir = tempdir::TempDir::new("journal").unwrap();
        let journal = BackendJournal::new(BackendMemory::default(),dir.path(),StartupVerification::SanityCheckAndRepairPending).unwrap();
        let mut board = BulletinBoard::new(journal).unwrap();
        board.submit_leaf("a").unwrap();
        let pending_file = dir.path().join("pending.csv");
        assert_eq!(true,pending_file.exists());
        let hash1 = board.order_new_published_root().unwrap();
        let root_file1 = dir.path().join(&(hash1.to_string()+".csv"));
        assert_eq!(false,pending_file.exists());
        assert_eq!(true,root_file1.exists());
        board.submit_leaf("b").unwrap();
        assert_eq!(true,pending_file.exists());
        let hash2 = board.order_new_published_root().unwrap();
        let root_file2 = dir.path().join(&(hash2.to_string()+".csv"));
        assert_eq!(false,pending_file.exists());
        assert_eq!(true,root_file2.exists());
        board.submit_leaf("c").unwrap();
        assert_eq!(true,pending_file.exists());
        let data_root1 =  std::fs::read_to_string(&root_file1).unwrap();
        let data_root2 =  std::fs::read_to_string(&root_file2).unwrap();
        let data_pending =  std::fs::read_to_string(&pending_file).unwrap();
        std::fs::remove_file(&root_file1).unwrap();
        std::fs::remove_file(&root_file2).unwrap();
        std::fs::remove_file(&pending_file).unwrap();
        // recreate
        let _journal = BackendJournal::new(board.backend.into_inner(),dir.path(),StartupVerification::RebuildAllJournals).unwrap();
        assert_eq!(data_root1,std::fs::read_to_string(&root_file1).unwrap());
        assert_eq!(data_root2,std::fs::read_to_string(&root_file2).unwrap());
        assert_eq!(data_pending,std::fs::read_to_string(&pending_file).unwrap());
    }
}