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

/// Store the "database" in a flat, csv file.
/// This is actually mostly a wrapper around BackendMemory, except transactions also get written to a file, and there is a load from file method.
/// Not usually useful for production as everything needs to be stored in memory.
pub struct BackendFlatfile {
    memory : BackendMemory,
    file : PathBuf,
}

impl BulletinBoardBackend for BackendFlatfile {
    fn get_all_published_roots(&self) -> anyhow::Result<Vec<HashValue>> { self.memory.get_all_published_roots()  }

    fn get_most_recent_published_root(&self) -> anyhow::Result<Option<HashValue>> { self.memory.get_most_recent_published_root() }

    fn get_all_leaves_and_branches_without_a_parent(&self) -> anyhow::Result<Vec<HashValue>> { self.memory.get_all_leaves_and_branches_without_a_parent() }

    fn lookup_hash(&self, query: HashValue) -> anyhow::Result<Option<HashInfo>> { self.memory.lookup_hash(query) }

    fn publish(&mut self, transaction: DatabaseTransaction) -> anyhow::Result<()> {
        let file = OpenOptions::new().append(true).create(true).open(&self.file)?;
        write_transaction_to_csv(&transaction,file)?;
        self.memory.publish(transaction)
    }
}

impl BackendFlatfile {
    pub fn new<P>(path: P) -> anyhow::Result<Self>
    where PathBuf: From<P>
    {
        let file = PathBuf::from(path);
        let mut memory = BackendMemory::default();
        if let Ok(file_reader) = File::open(&file) { // file may not exist.
            for transaction in TransactionIterator::new(file_reader) {
                memory.publish(transaction?)?
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
///   * 1 means a branch, history is the left and right hashes.
///   * 2 means a published root, history is the timestamp and then the hashes in this node.
///
/// # Examples
///
/// ```
/// use merkle_tree_bulletin_board::DatabaseTransaction;
/// use merkle_tree_bulletin_board::hash_history::LeafHashHistory;
/// let mut output : Vec<u8> = vec![];
/// let mut transaction : DatabaseTransaction = DatabaseTransaction::default();
/// let history = LeafHashHistory{timestamp: 42 ,data: "The answer".to_string() };
/// let hash = history.compute_hash();
/// assert_eq!(hash.to_string(),"68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee");
/// transaction.add_leaf_hash(hash,history);
/// merkle_tree_bulletin_board::backend_flatfile::write_transaction_to_csv(&transaction,&mut output).unwrap();
/// assert_eq!(String::from_utf8(output).unwrap(),"0,68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee,42,The answer\n\n");
/// ```
pub fn write_transaction_to_csv<W: Write>(transaction:&DatabaseTransaction, writer:W) -> std::io::Result<()> {
    let mut csv_writer = WriterBuilder::new().flexible(true).from_writer(writer);
    for (hash,source) in &transaction.pending {
        match source {
            HashSource::Leaf(history) => {
                csv_writer.write_record(&["0",&hash.to_string(),&history.timestamp.to_string(),&history.data])?;
            }
            HashSource::Branch(history) => {
                csv_writer.write_record(&["1",&hash.to_string(),&history.left.to_string(),&history.right.to_string()])?;
            }
            HashSource::Root(history) => {
                csv_writer.write_field("2")?;
                csv_writer.write_field(&hash.to_string())?;
                csv_writer.write_field(&history.timestamp.to_string())?;
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
    /// build a new transaction reader from a file.
    /// # Examples
    /// ```
    /// use merkle_tree_bulletin_board::backend_flatfile::TransactionIterator;
    /// use merkle_tree_bulletin_board::hash_history::HashSource;
    /// use merkle_tree_bulletin_board::DatabaseTransaction;
    /// let file = "0,68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee,42,The answer\n\n";
    /// let transactions = TransactionIterator::new(file.as_bytes());
    /// let as_vec : Vec<anyhow::Result<DatabaseTransaction>> = transactions.collect();
    /// assert_eq!(as_vec.len(),1);
    /// let trans1 : &DatabaseTransaction = as_vec[0].as_ref().unwrap();
    /// assert_eq!(trans1.pending.len(),1);
    /// let (hash,source) = trans1.pending[0].clone();
    /// assert_eq!(hash.to_string(),"68c3cefbe5b64fc51713cabe524cd35f2be6e52148a0f201476f16f378cb1aee");
    /// if let HashSource::Leaf(history) = source {
    ///   assert_eq!(history.timestamp,42);
    ///   assert_eq!(history.data,"The answer");
    /// } else {
    ///   panic!("source is wrong type");
    /// }
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
                    if record.len()!=4 { return Err(anyhow!("Leaf node should have 4 fields")); }
                    HashSource::Leaf(LeafHashHistory{ timestamp : Timestamp::from_str(record.get(2).unwrap())?, data: record.get(3).unwrap().to_string() })
                }
                Some("1") => { // branch
                    if record.len()!=4 { return Err(anyhow!("Branch node should have 4 fields")); }
                    HashSource::Branch(BranchHashHistory{ left : HashValue::from_str(record.get(2).unwrap())?, right: HashValue::from_str(record.get(3).unwrap())?})
                }
                Some("2") => { // published
                    if record.len()<3 { return Err(anyhow!("Publish node should have at least 3 fields")); }
                    let mut elements = vec![];
                    for contained_hash in record.iter().dropping(3) {
                        elements.push(HashValue::from_str(contained_hash)?);
                    }
                    HashSource::Root(RootHashHistory{ timestamp : Timestamp::from_str(record.get(2).unwrap())?, elements })
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
                    println!("Read record with {} entries line {} byte {} self line {} byte {} beefore reading line {}",self.record.len(),self.record.position().unwrap().line(),self.record.position().unwrap().byte(),self.csv_reader.position().line(),self.csv_reader.position().byte(),line_before_reading_record);
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
                    return Some(Err(anyhow!("transaction not complete")));
                }
            }
        }
    }
}


