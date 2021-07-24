//! A backend for the database based on csv files.

use crate::backend_memory::BackendMemory;
use std::path::PathBuf;
use crate::{DatabaseTransaction, BulletinBoardBackend};
use csv::{WriterBuilder, ReaderBuilder, StringRecord};
use std::io::{Write, Read};
use crate::hash_history::{HashSource, HashInfo, Timestamp, LeafHashHistory, BranchHashHistory, RootHashHistory};
use crate::hash::HashValue;
use std::fs::{OpenOptions, File};
use std::str::FromStr;
use itertools::Itertools;
use anyhow::anyhow;
use crate::deduce_journal::deduce_journal;

/// Store the "database" in a flat, csv file.
/// This is actually mostly a wrapper around BackendMemory, except transactions also get written to a file, and there is a load from file method.
/// Not usually useful for production as everything needs to be stored in memory.
///
/// Data is stored in a file in the format used by [write_transaction_to_csv]. The file is appended to for each transaction,
/// and not held open, although this may change in the future for performance reasons.
///
/// Censorship is supported but is horrendously inefficient - the entire file is rewritten after each censorship.
pub struct BackendFlatfile {
    memory : BackendMemory,
    file : PathBuf,
}

impl BulletinBoardBackend for BackendFlatfile {
    fn get_all_published_roots(&self) -> anyhow::Result<Vec<HashValue>> { self.memory.get_all_published_roots()  }

    fn get_most_recent_published_root(&self) -> anyhow::Result<Option<HashValue>> { self.memory.get_most_recent_published_root() }

    fn get_all_leaves_and_branches_without_a_parent(&self) -> anyhow::Result<Vec<HashValue>> { self.memory.get_all_leaves_and_branches_without_a_parent() }

    fn get_hash_info(&self, query: HashValue) -> anyhow::Result<Option<HashInfo>> { self.memory.get_hash_info(query) }

    fn publish(&mut self, transaction: &DatabaseTransaction) -> anyhow::Result<()> {
        let file = OpenOptions::new().append(true).create(true).open(&self.file)?;
        write_transaction_to_csv(&transaction,&file)?;
        file.sync_data()?;
        self.memory.publish(transaction)
    }

    /// Horrendously inefficient - re-deduce order and write out whole file.
    fn censor_leaf(&mut self, leaf_to_censor: HashValue) -> anyhow::Result<()> {
        self.memory.censor_leaf(leaf_to_censor)?;
        let file = OpenOptions::new().write(true).truncate(true).create(true).open(&self.file)?; // Don't append!
        for transaction in deduce_journal(&self.memory,&vec![],&self.get_all_leaves_and_branches_without_a_parent()?,true)? {
            write_transaction_to_csv(&transaction,&file)?;
        }
        file.sync_data()?;
        Ok(())
    }
}

impl BackendFlatfile {
    /// Create a new flat file backed backend, storing data in the provided file.
    /// The file will be read if it exists, and used to initialize the database.
    /// When new elements are published, the file will be appended.
    pub fn new<P>(path: P) -> anyhow::Result<Self>
    where PathBuf: From<P>
    {
        let file = PathBuf::from(path);
        let mut memory = BackendMemory::default();
        if let Ok(file_reader) = File::open(&file) { // file may not exist.
            for transaction in TransactionIterator::new(file_reader) {
                memory.publish(&transaction?)?
            }
        }
        Ok(BackendFlatfile{ memory, file })
    }
}


/// Write out a transaction to a csv file. The format is
/// * Blank lines represent the end of a transaction.
/// * Otherwise, the first field is an integer 0, 1 or 2 specifying the type of the node being created,
///   and the second field is the hash value. After that are fields specifying how the object was created.
///   * 0 means a leaf, history is the timestamp (seconds since epoch) and then the string it was created from (appropriately csv escaped).
///      - If the leaf data has been censored, then there is only a timestamp field, no fourth field.
///   * 1 means a branch, history is the left and right hashes.
///   * 2 means a published root, history is the timestamp, then the prior published root or empty field, and then the hashes in this node.
///
/// To read in transactions from a file, into an iterator, see [TransactionIterator::new]
///
/// All character encoding is UTF-8.
///
/// # Examples
///
/// ```
/// use merkle_tree_bulletin_board::DatabaseTransaction;
/// use merkle_tree_bulletin_board::hash_history::LeafHashHistory;
/// use merkle_tree_bulletin_board::backend_flatfile::write_transaction_to_csv;
/// let mut output : Vec<u8> = vec![];
/// let mut transaction : DatabaseTransaction = DatabaseTransaction::default();
/// let history = LeafHashHistory{timestamp: 42 ,data: Some("The answer".to_string()) };
/// let hash = history.compute_hash().unwrap();
/// assert_eq!(hash.to_string(),"68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee");
/// transaction.add_leaf_hash(hash,history);
/// write_transaction_to_csv(&transaction,&mut output).unwrap();
/// assert_eq!(String::from_utf8(output).unwrap(),
/// "0,68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee,42,The answer\n\n");
/// ```
///
/// The following more complex example shows multiple entries, and CSV sensitive characters "" and ,
/// Note that in practice, you would never have a transaction with two leaves in it.
/// ```
/// use merkle_tree_bulletin_board::DatabaseTransaction;
/// use merkle_tree_bulletin_board::hash_history::LeafHashHistory;
/// use merkle_tree_bulletin_board::backend_flatfile::write_transaction_to_csv;
/// let mut output : Vec<u8> = vec![];
/// let mut transaction : DatabaseTransaction = DatabaseTransaction::default();
/// let history = LeafHashHistory{timestamp: 42 ,data: Some("The answer".to_string()) };
/// let hash = history.compute_hash().unwrap();
/// assert_eq!(hash.to_string(),"68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee");
/// transaction.add_leaf_hash(hash,history);
/// let history = LeafHashHistory{timestamp: 43 ,data: Some(r#"The new improved, "web 2.0" answer
/// with a newline in the middle"#.to_string()) };
/// let hash = history.compute_hash().unwrap();
/// assert_eq!(hash.to_string(),"1d1633c405293e54ac8434c34dfa2532d59172979d1dc38a6389485b35f51762");
/// transaction.add_leaf_hash(hash,history);
/// write_transaction_to_csv(&transaction,&mut output).unwrap();
/// assert_eq!(String::from_utf8(output).unwrap(),
/// r#"0,68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee,42,The answer
/// 0,1d1633c405293e54ac8434c34dfa2532d59172979d1dc38a6389485b35f51762,43,"The new improved, ""web 2.0"" answer
/// with a newline in the middle"
///
/// "#);
/// ```
pub fn write_transaction_to_csv<W: Write>(transaction:&DatabaseTransaction, writer:W) -> std::io::Result<()> {
    let mut csv_writer = WriterBuilder::new().flexible(true).from_writer(writer);
    for (hash,source) in &transaction.pending {
        match source {
            HashSource::Leaf(history) => {
                if let Some(uncensored_data) = &history.data {
                    csv_writer.write_record(&["0",&hash.to_string(),&history.timestamp.to_string(),uncensored_data])?;
                } else {
                    csv_writer.write_record(&["0",&hash.to_string(),&history.timestamp.to_string()])?;
                }
            }
            HashSource::Branch(history) => {
                csv_writer.write_record(&["1",&hash.to_string(),&history.left.to_string(),&history.right.to_string()])?;
            }
            HashSource::Root(history) => {
                csv_writer.write_field("2")?;
                csv_writer.write_field(&hash.to_string())?;
                csv_writer.write_field(&history.timestamp.to_string())?;
                match history.prior {
                    None => csv_writer.write_field("")?,
                    Some(prior) => csv_writer.write_field(&prior.to_string())?,
                }
                for e in &history.elements {
                    csv_writer.write_field(&e.to_string())?;
                }
                csv_writer.write_record(None::<&[u8]>)?;
            }
        }
    }
    // flush and put a blank line at end to delimit transactions. Doing this via the csv writer write_record causes it to write a single blank field.
    match csv_writer.into_inner() { // can't just use ? as embeds w in the error which causes all sorts of thread issues.
        Ok(mut w) => w.write_all(b"\n")?,
        Err(_) => return Err(std::io::Error::new(std::io::ErrorKind::Other, "Could not execute into_inner on csv writer")),
    }
    Ok(())
}

/// Iterate over transactions in a csv file produced by multiple invocations of [write_transaction_to_csv].
pub struct TransactionIterator<R:Read> {
    csv_reader : csv::Reader<R>, // the source of the records
    record: StringRecord, // reused buffer
    read_ahead : Option<(HashValue,HashSource)> // work around for csv reader not being able to detect blank lines means sometimes a read ahead is done.
}


impl<R: Read> TransactionIterator<R> {
    /// Read in transactions from a CSV file written via [write_transaction_to_csv].
    ///
    /// The result is an iterator over transactions.
    ///
    /// # Examples
    /// ```
    /// use merkle_tree_bulletin_board::backend_flatfile::TransactionIterator;
    /// use merkle_tree_bulletin_board::hash_history::{HashSource, LeafHashHistory};
    /// use merkle_tree_bulletin_board::DatabaseTransaction;
    /// let file = "0,68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee,42,The answer\n\n";
    /// let transactions = TransactionIterator::new(file.as_bytes());
    /// let as_vec : Vec<anyhow::Result<DatabaseTransaction>> = transactions.collect();
    /// assert_eq!(as_vec.len(),1);
    /// let trans1 : &DatabaseTransaction = as_vec[0].as_ref().unwrap();
    /// assert_eq!(trans1.pending.len(),1);
    /// let (hash,source) = trans1.pending[0].clone();
    /// assert_eq!(hash.to_string(),"68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee");
    /// assert_eq!(source,HashSource::Leaf(
    ///       LeafHashHistory{timestamp:42,data:Some("The answer".to_string())}));
    /// ```
    pub fn new(reader: R) -> TransactionIterator<R> {
        TransactionIterator { csv_reader : ReaderBuilder::new().has_headers(false).flexible(true).from_reader(reader), record: StringRecord::new() , read_ahead:None }
    }

}


impl<'r, R: Read> Iterator for TransactionIterator<R> {
    type Item = anyhow::Result<DatabaseTransaction>;

    fn next(&mut self) -> Option<anyhow::Result<DatabaseTransaction>> {
        fn parse_record(record:&StringRecord) -> anyhow::Result<(HashValue,HashSource)> {
            let hash = match record.get(1) {
                Some(s) => HashValue::from_str(s)?,
                None => return Err(anyhow!("No hash")),
            };
            let history = match record.get(0) {
                Some("0") => { // leaf
                    if record.len()<3 || record.len()>4 { return Err(anyhow!("Leaf node should have 3 or 4 fields")); }
                    HashSource::Leaf(LeafHashHistory{ timestamp : Timestamp::from_str(record.get(2).unwrap())?, data: record.get(3).map(|e|e.to_string()) })
                }
                Some("1") => { // branch
                    if record.len()!=4 { return Err(anyhow!("Branch node should have 4 fields")); }
                    HashSource::Branch(BranchHashHistory{ left : HashValue::from_str(record.get(2).unwrap())?, right: HashValue::from_str(record.get(3).unwrap())?})
                }
                Some("2") => { // published
                    if record.len()<4 { return Err(anyhow!("Publish node should have at least 4 fields")); }
                    let mut elements = vec![];
                    for contained_hash in record.iter().dropping(4) {
                        elements.push(HashValue::from_str(contained_hash)?);
                    }
                    let prior_str = record.get(3).unwrap();
                    let prior = if prior_str.is_empty() { None } else { Some(HashValue::from_str(prior_str)?)};
                    HashSource::Root(RootHashHistory{ timestamp : Timestamp::from_str(record.get(2).unwrap())?, prior, elements })
                }
                _ => return Err(anyhow!("Invalid type specifier")),
            };
            Ok((hash,history))
        }
        let mut transaction = DatabaseTransaction::default();
        if let Some(pair) = self.read_ahead.take() { // if there was a read ahead in the past.
            transaction.pending.push(pair);
        }
        loop {
            // unfortunately the csv reader library skips blank lines. So detecting transactions is hard. See https://github.com/BurntSushi/rust-csv/issues/159
            // work around using line numbers, unfortunately the line counter is buggy in the presence of blank lines. See https://giters.com/BurntSushi/rust-csv/issues/208?amp=1 so have to work around said bug and possible fix.
            // work around is to do read ahead
            let line_before_reading_record = self.csv_reader.position().line();
            match self.csv_reader.read_record(&mut self.record) {
                Err(err) => return Some(Err(anyhow::Error::from(err))),
                Ok(true) => {
                    // println!("Read record with {} entries line {} byte {} self line {} byte {} before reading line {}",self.record.len(),self.record.position().unwrap().line(),self.record.position().unwrap().byte(),self.csv_reader.position().line(),self.csv_reader.position().byte(),line_before_reading_record);
                    if self.record.is_empty() { return Some(Ok(transaction)) } // This never triggers as blank records are silently skipped.
                    else {
                        match parse_record(&self.record) {
                            Ok(pair) => {
                                // see if this record is _after_ a transaction.
                                if line_before_reading_record+2==self.csv_reader.position().line() { // this should detect a single blank line without being affected by https://giters.com/BurntSushi/rust-csv/issues/208?amp=1 or a fix for said bug.
                                    self.read_ahead=Some(pair);
                                    return Some(Ok(transaction));
                                }
                                transaction.pending.push(pair)
                            },
                            Err(e) => return Some(Err(e)),
                        }

                    }
                }
                Ok(false) if transaction.pending.is_empty() => return None,
                Ok(false) => {
                    // hacky work around to see if the EOF has a transaction. This is still buggy as it can't detect any other blank lines.
                    if self.record.position().unwrap().line()+1==self.csv_reader.position().line() { // EOF after a blank line is dealt with OK here.
                        if !transaction.pending.is_empty() { return Some(Ok(transaction)) }
                    }
                    // println!("EOF record with {} entries line {} byte {} self line {} byte {}",self.record.len(),self.record.position().unwrap().line(),self.record.position().unwrap().byte(),self.csv_reader.position().line(),self.csv_reader.position().byte());
                    // println!("transaction.pending has {} entries, the first one being for {}",transaction.pending.len(),transaction.pending[0].0);
                    return Some(Err(anyhow!("transaction starting at line {} with hash {} not complete",self.record.position().unwrap().line(),transaction.pending[0].0)));
                }
            }
        }
    }
}


