//! The back end for storing and querying Merkle trees/hashes.

// TODO need to parameterize everything on a generic backend.



use crate::hash::{HashValue, parse_string_to_hash_vec};
use crate::hash_history::{BranchHashHistory, HashInfo, HashSource, PublishedRootHistory, LeafHashHistory, Timestamp};
use std::collections::{HashMap, HashSet};
use std::fs::{OpenOptions};
use std::fs;
use std::path::{PathBuf};
use crate::build_merkle::{HashesInPlay};
use std::io::Write;
use itertools::Itertools;

/// Store the logs in a flat file
/// Read the files in when the program starts.
/// Append to the logs when a modification occurs.
pub struct BackendFlatfile {
    hash_lookup : HashMap<HashValue,HashInfo>,
    leaf_lookup : HashMap<String,HashValue>,
    published : Vec<HashValue>,
    file_leaf : PathBuf,
    file_branch : PathBuf,
    file_published : PathBuf,
}

impl BackendFlatfile {
    
    pub fn new(directory : &str) -> anyhow::Result<Self> {
        let pathbuf = PathBuf::from(directory);
        let path = pathbuf.as_path();
        fs::create_dir_all(directory)?;
        let res = BackendFlatfile{
            hash_lookup: Default::default(),
            leaf_lookup: Default::default(),
            published: vec![],
            file_leaf : path.join("leaves.csv"),
            file_branch : path.join("branches.csv"),
            file_published : path.join("published.csv"),
        };
        Ok(res)
    }
    pub fn reload_and_recreate_inplay(&mut self) -> anyhow::Result<HashesInPlay> {
        {
            // read leaf file
            #[derive(Debug, serde::Deserialize)]
            struct LeafRecord { hash:HashValue, timestamp:Timestamp, data : String }
            if let Ok(mut rdr) = csv::ReaderBuilder::new().has_headers(false).from_path(&self.file_leaf) { // file may not exist.
                println!("Reading leaf file");
                for result in rdr.deserialize() {
                    let record: LeafRecord = result?;
                    self.leaf_lookup.insert(record.data.clone(),record.hash);
                    self.hash_lookup.insert(record.hash,HashInfo{ source: HashSource::Leaf(LeafHashHistory{ timestamp: record.timestamp, data: record.data }), parent: None });
                }
            }
        }
        {
            // read branch file
            #[derive(Debug, serde::Deserialize)]
            struct BranchRecord { hash:HashValue, timestamp:Timestamp, left : HashValue, right:HashValue }
            if let Ok(mut rdr) = csv::ReaderBuilder::new().has_headers(false).from_path(&self.file_branch) { // file may not exist.
                println!("Reading branch file");
                for result in rdr.deserialize() {
                    let record: BranchRecord = result?;
                    self.hash_lookup.insert(record.hash,HashInfo{ source: HashSource::Branch(BranchHashHistory{timestamp: record.timestamp,left: record.left,right: record.right}), parent: None });
                    self.add_parent(&record.left,record.hash);
                    self.add_parent(&record.right,record.hash);
                }
            }
        }
        // get the elements with no parent
        let mut elements_with_no_parent = self.hash_lookup.iter().filter(|(_,info)|info.parent.is_none()).map(|(hash,info)|(*hash,info.source.timestamp())).collect_vec();
        elements_with_no_parent.sort_by_key(|(_,timestamp)|*timestamp);
        {
            // read published file
            #[derive(Debug, serde::Deserialize)]
            struct PublishedRecord { hash:HashValue, timestamp:Timestamp, elements:String }
            if let Ok(mut rdr) = csv::ReaderBuilder::new().has_headers(false).from_path(&self.file_published) { // file may not exist.
                println!("Reading published file");
                for result in rdr.deserialize() {
                    let record: PublishedRecord = result?;
                    let history = PublishedRootHistory { timestamp: record.timestamp, elements: parse_string_to_hash_vec(&record.elements)? };
                    self.hash_lookup.insert(record.hash, HashInfo { source: HashSource::Root(history.clone()), parent: None });
                    self.published.push(record.hash);
                }
            }
        }
        let most_recently_published : Option<HashValue> = self.published.last().map(|h|*h);
        let in_most_recently_published : HashSet<HashValue> = match most_recently_published.as_ref().and_then(|h|self.hash_lookup.get(h)).map(|info|&info.source) {
            Some(HashSource::Root(history)) => history.elements.iter().map(|e|*e).collect(),
            _ => Default::default()
        };
        // pending elements are elements without parents.
        let mut no_parent_unpublished = vec![];
        let mut no_parent_published = vec![];
        for (hash,timestamp) in &elements_with_no_parent {
            if in_most_recently_published.contains(&hash) { no_parent_published.push((*hash,*timestamp)); } else { no_parent_unpublished.push((*hash,self.left_depth(hash))); }
        }
        // don't need to sort as sorted above.
        //no_parent_published.sort_by_key(|(_,timestamp)|*timestamp);
        //no_parent_unpublished.sort_by_key(|(_,depth)| usize::MAX-*depth);
        let no_parent_published = no_parent_published.iter().rev().map(|(hash,_)|hash).enumerate().rev().map(|(depth,hash)|(*hash,depth)).collect_vec(); // approximation to size.
        let in_play = HashesInPlay::build_from(no_parent_unpublished,no_parent_published,most_recently_published);
        Ok(in_play)
    }

    fn left_depth(&self,hash:&HashValue) -> usize {
        let mut res = 0;
        let mut hash = *hash;
        loop {
            match self.hash_lookup.get(&hash).map(|info|&info.source) {
                Some(HashSource::Branch(history)) => {
                    res+=1;
                    hash = history.left;
                }
                _ => break
            }
        }
        res
    }

    pub fn lookup_hash(&self,query:HashValue) -> Option<HashInfo> {
        self.hash_lookup.get(&query).map(|r|r.clone())
    }

    fn add_parent(&mut self,child:&HashValue,parent:HashValue) {
        self.hash_lookup.get_mut(child).unwrap().parent=Some(parent);
    }
    pub fn add_branch_hash(&mut self,new_hash:HashValue,history:BranchHashHistory) -> anyhow::Result<()> {
        self.hash_lookup.insert(new_hash,HashInfo{ source: HashSource::Branch(history), parent: None });
        self.add_parent(&history.left,new_hash);
        self.add_parent(&history.right,new_hash);
        let mut file = OpenOptions::new().append(true).create(true).open(&self.file_branch)?;
        write!(file,"{},{},{},{}\n",new_hash,history.timestamp,history.left,history.right)?;
        Ok(())
    }

    pub fn add_published_hash(&mut self,new_hash:HashValue,history:PublishedRootHistory) -> anyhow::Result<()> {
        self.hash_lookup.insert(new_hash,HashInfo{ source: HashSource::Root(history.clone()), parent: None });
        self.published.push(new_hash);
        let mut file = OpenOptions::new().append(true).create(true).open(&self.file_published)?;
        write!(file,"{},{},{}\n",new_hash,history.timestamp,&history.elements.iter().map(|h|h.to_string()).collect_vec().join(";"))?;
        Ok(())
    }

    pub fn add_leaf_hash(&mut self,new_hash:HashValue,history:LeafHashHistory) -> anyhow::Result<()> {
        self.leaf_lookup.insert(history.data.clone(),new_hash);
        self.hash_lookup.insert(new_hash,HashInfo{ source: HashSource::Leaf(history.clone()), parent: None });
        let mut file = OpenOptions::new().append(true).create(true).open(&self.file_leaf)?;
        write!(file,"{},{},{}\n",new_hash,history.timestamp,&history.data)?;
        Ok(())
    }

    pub fn get_all_published_heads(&self) -> Vec<HashValue> {
        self.published.clone()
    }

    pub fn save_inplay(&mut self,_in_play:&HashesInPlay) -> anyhow::Result<()> {
        Ok(()) // done implictly; one can rederive in_play from the recorded data.
    }
}