use mysql::{Conn, from_value, Value, TxOpts};
use std::ops::DerefMut;
use merkle_tree_bulletin_board::{BulletinBoardBackend, DatabaseTransaction, BulletinBoardError};
use merkle_tree_bulletin_board::hash::HashValue;
use merkle_tree_bulletin_board::hash_history::{HashInfo, HashSource, LeafHashHistory, BranchHashHistory, RootHashHistory};
use mysql::prelude::{Queryable};
use std::sync::{Mutex, MutexGuard};
use std::convert::TryInto;

/// A mysql/mariadb backend for merkle-tree-bulletin-board.
/// This is usable but is not extensively optimized; an expert in mysql/databases/sql could probably improve efficiency.
///
/// All operations are O(sql index lookup)*O(data size) and data size is generally O(log n) where n
/// is the number of items in the bulletin board.
///
/// There is a demo program in bin/test_mysql.rs which uses this on a small dataset.
///
/// This uses the schema:
/// ```sql
#[doc = include_str!("Schema.sql")]
/// ```
pub struct BackendMysql<C:DerefMut<Target=Conn>> {
    pub connection : Mutex<C>,
}

fn mysql_to_bb_error(error: mysql::Error) -> BulletinBoardError {
    BulletinBoardError::BackendIOError(format!("MySql error {}",error))
}
impl <C:DerefMut<Target=Conn>> BackendMysql<C> {
    fn get_connection_lock(&self) -> Result<MutexGuard<'_, C>,BulletinBoardError> {
        let mut guard = self.connection.lock().unwrap();
        if !guard.ping() { guard.reset().map_err(|e|BulletinBoardError::BackendIOError(format!("Connection to bulletin board database was lost, and could not be recovered with Conn::reset() : {}",e)))? }
        Ok(guard)
    }

    fn query_hashes(&self, query : &'_ str) -> Result<Vec<HashValue>,BulletinBoardError> {
        let res : mysql::Result<Vec<Result<HashValue,InvalidHashFormatError>>> = self.get_connection_lock()?.query_map(query,|(v,)| hash_from_value(v));
        if let Err(e) = &res {
            eprintln!("Had error {} running {}",e,query)
        }
        let res : Result<Vec<HashValue>,InvalidHashFormatError> = res.map_err(mysql_to_bb_error)?.into_iter().collect();
        Ok(res?)
    }
}

/// The schema used; can be used to initialize the database.
pub const SCHEMA : &'static str = include_str!("Schema.sql");

pub struct InvalidHashFormatError {}

impl From<InvalidHashFormatError> for BulletinBoardError {
    fn from(_value: InvalidHashFormatError) -> Self {
        BulletinBoardError::BackendInconsistentError("an element of the bulletin board database that was supposed to be a hash was not".to_string())
    }
}

/// Convert v into a HashValue where you know v will be a 32 byte value
pub fn hash_from_value(v:Value) -> Result<HashValue,InvalidHashFormatError> {
    match v {
        Value::Bytes(b) if b.len()==32 => Ok(HashValue(b.try_into().map_err(|_|InvalidHashFormatError{})?)),
        _ => { Err(InvalidHashFormatError{}) }
    }
}

/// Convert v into a HashValue where you know v will be a 32 byte value or null
pub fn opt_hash_from_value(v:Value) -> Result<Option<HashValue>,InvalidHashFormatError> {
    match v {
        Value::Bytes(b) if b.len()==32 => Ok(Some(HashValue(b.try_into().map_err(|_|InvalidHashFormatError{})?))),
        Value::NULL => Ok(None),
        _ => { Err(InvalidHashFormatError{}) }
    }
}



impl <C:DerefMut<Target=Conn>> BulletinBoardBackend for BackendMysql<C> {
    fn get_all_published_roots(&self) -> Result<Vec<HashValue>,BulletinBoardError> {
        let res = self.query_hashes("SELECT hash from PUBLISHED_ROOTS order by serial")?;
        Ok(res)
    }

    fn get_most_recent_published_root(&self) -> Result<Option<HashValue>,BulletinBoardError> {
        let res = self.query_hashes("SELECT hash from PUBLISHED_ROOTS order by serial DESC LIMIT 1")?;
        Ok(res.first().cloned())
    }

    fn get_all_leaves_and_branches_without_a_parent(&self) -> Result<Vec<HashValue>,BulletinBoardError> {
        let mut res_leaves = self.query_hashes("SELECT hash from LEAF where parent IS NULL")?;
        // println!("leaves : {:#?}",res_leaves);
        let mut res_branches = self.query_hashes("SELECT hash from BRANCH where parent IS NULL")?;
        // println!("branches : {:#?}",res_branches);
        res_leaves.append(&mut res_branches);
        Ok(res_leaves)
    }

    fn get_hash_info(&self, query: HashValue) -> Result<Option<HashInfo>,BulletinBoardError> {
        let mut lock = self.get_connection_lock()?;
        // see if it is a leaf
        if let Some((timestamp,data,parent)) = lock.exec_first("SELECT timestamp,data,parent from LEAF WHERE hash=?",(query.0,)).map_err(mysql_to_bb_error)? {
            return Ok(Some(HashInfo{ source: HashSource::Leaf(LeafHashHistory{ timestamp: from_value(timestamp), data: from_value(data) }), parent : opt_hash_from_value(parent)? }))
        }
        // see if it is a branch
        if let Some((left_child,right_child,parent)) = lock.exec_first("SELECT left_child,right_child,parent from BRANCH WHERE hash=?",(query.0,)).map_err(mysql_to_bb_error)? {
            return Ok(Some(HashInfo{ source: HashSource::Branch(BranchHashHistory{ left: hash_from_value(left_child)?, right: hash_from_value(right_child)? }), parent : opt_hash_from_value(parent)? }))
        }
        // see if it is a root
        if let Some((prior_hash,timestamp)) = lock.exec_first("SELECT prior_hash,timestamp from PUBLISHED_ROOTS where hash=?",(query.0,)).map_err(mysql_to_bb_error)? {
            let elements : Vec<Result<HashValue,InvalidHashFormatError>> = lock.exec_map("SELECT referenced from PUBLISHED_ROOT_REFERENCES where published=? order by position",(query.0,),|(v,)|hash_from_value(v)).map_err(mysql_to_bb_error)?;
            let elements : Result<Vec<HashValue>,InvalidHashFormatError> = elements.into_iter().collect();
            let elements : Vec<HashValue> = elements?;
            return Ok(Some(HashInfo{ source: HashSource::Root(RootHashHistory{ timestamp: from_value(timestamp), prior: opt_hash_from_value(prior_hash)?, elements }), parent : None }))
        }
        Ok(None)
    }

    fn publish(&mut self, transaction: &DatabaseTransaction) -> Result<(),BulletinBoardError> {
        let mut lock = self.get_connection_lock()?;
        let mut tx = lock.start_transaction(TxOpts::default()).map_err(mysql_to_bb_error)?;
        for (hash,source) in &transaction.pending {
            match source {
                HashSource::Leaf(history) => {
                    // println!("Publishing leaf {} data {}",hash,history.data.as_ref().unwrap());
                    tx.exec_drop("insert into LEAF (hash,timestamp,data) values (?,?,?)",(hash.0,history.timestamp,&history.data)).map_err(mysql_to_bb_error)?;
                }
                HashSource::Branch(history) => {
                    tx.exec_drop("insert into BRANCH (hash,left_child,right_child) values (?,?,?)",(hash.0,history.left.0,history.right.0)).map_err(mysql_to_bb_error)?;
                    // update parents. Could optimize as prior insert probably has one of them.
                    tx.exec_drop("update BRANCH set parent=? where hash=? or hash=?",(hash.0,history.left.0,history.right.0)).map_err(mysql_to_bb_error)?;
                    tx.exec_drop("update LEAF set parent=? where hash=? or hash=?",(hash.0,history.left.0,history.right.0)).map_err(mysql_to_bb_error)?;
                }
                HashSource::Root(history) => {
                    tx.exec_drop("insert into PUBLISHED_ROOTS (hash,prior_hash,timestamp) values (?,?,?)",(hash.0,history.prior.map(|h|h.0),history.timestamp)).map_err(mysql_to_bb_error)?;
                    // update referenced elements
                    for position in 0..history.elements.len() {
                        let referenced = history.elements[position];
                        tx.exec_drop("insert into PUBLISHED_ROOT_REFERENCES (published,referenced,position) values (?,?,?)",(hash.0,referenced.0,position)).map_err(mysql_to_bb_error)?;
                    }
                }
            }
        }
        tx.commit().map_err(mysql_to_bb_error)?;
        Ok(())
    }

    fn censor_leaf(&mut self, leaf_to_censor: HashValue) -> Result<(),BulletinBoardError> {
        let mut lock = self.get_connection_lock()?;
        lock.exec_drop("update LEAF set data=null where hash=?",(leaf_to_censor.0,)).map_err(mysql_to_bb_error)?;
        Ok(())
    }
}
