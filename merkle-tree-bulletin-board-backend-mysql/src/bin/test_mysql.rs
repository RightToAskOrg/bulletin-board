use mysql::{Conn, Opts};
use mysql::prelude::Queryable;
use std::sync::Mutex;
use merkle_tree_bulletin_board::{BulletinBoard, BulletinBoardBackend};
use merkle_tree_bulletin_board::hash_history::{HashSource, LeafHashHistory, HashInfo, BranchHashHistory, RootHashHistory};
use merkle_tree_bulletin_board::hash::HashValue;

/// Demo of use of the mysql backend.
/// requires setting up a mysql or mariadb server.
///
/// On Ubuntu this can be done by
/// ```
/// sudo apt install mariadb-server
/// sudo mysql_secure_installation
/// ```
/// Then set up a non-admin account via `sudo mariadb` and
/// execute
/// ```sql
/// CREATE DATABASE IF NOT EXISTS bulletinboard;
/// GRANT ALL PRIVILEGES ON bulletinboard.* TO 'bulletinboard'@'localhost' IDENTIFIED BY 'ThisShouldBeReplacedByAPassword';
/// FLUSH PRIVILEGES;
/// ```
///
/// Make sure you change the password above to something sensible, and also change the
/// "url" line just below to match the same password. DO NOT USE "ThisShouldBeReplacedByAPassword"
/// as the password, apart from any other problem, there are bots that sift through
/// github looking for such hard coded credentials.
///
fn main() -> anyhow::Result<()> {
    // The below string is NOT the actual credentials and should be changed prior to use.
    // This is NOT how it should be run in production. This is just a demo.
    // Our password is NOT ThisShouldBeReplacedByAPassword.
    let url = "mysql://bulletinboard:ThisShouldBeReplacedByAPassword@localhost:3306/bulletinboard";

    let opts = Opts::from_url(url)?;
    let mut conn = Conn::new(opts)?;

    conn.query_drop("drop table if exists PUBLISHED_ROOTS")?;
    conn.query_drop("drop table if exists PUBLISHED_ROOT_REFERENCES")?;
    conn.query_drop("drop table if exists BRANCH")?;
    conn.query_drop("drop table if exists LEAF")?;

    let schema = include_str!("Schema.sql");
    // println!("Running schema\n{}",&schema);

    conn.query_drop(schema)?;

    let backend = merkle_tree_bulletin_board_backend_mysql::BackendMysql{ connection: Mutex::new(Box::new(conn)) };

    let mut board = BulletinBoard::new(backend).unwrap();
    // utility function to check that something is indeed a leaf with the expected data.
    fn assert_is_leaf(source:HashSource,expected_data:&str) {
       match source {
         HashSource::Leaf(LeafHashHistory{data:Some(d),timestamp:_}) => assert_eq!(d,expected_data),
         _ => panic!("Not a leaf"),
       }
    }

    assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
    assert_eq!(board.get_most_recent_published_root().unwrap(),None);
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![]);

    println!("Done first asserts");

    #[allow(non_snake_case)]
    let hash_A : HashValue = board.submit_leaf("A").unwrap();
    // we have inserted A, which is a single tree but nothing is published.
    assert_eq!(board.get_hash_info(hash_A).unwrap().parent,None);
    assert_is_leaf(board.get_hash_info(hash_A).unwrap().source,"A");
    assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![hash_A]);

    #[allow(non_snake_case)]
    let hash_B : HashValue = board.submit_leaf("B").unwrap();
    // we have now inserted B, which will be merged into a tree with A on the left and B on the right.
    #[allow(non_snake_case)]
    let branch_AB : HashValue = board.get_hash_info(hash_A).unwrap().parent.unwrap();
    assert_eq!(board.get_hash_info(hash_B).unwrap().parent,Some(branch_AB));
    assert_is_leaf(board.get_hash_info(hash_B).unwrap().source,"B");
    assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![branch_AB]);
    assert_eq!(board.get_hash_info(branch_AB).unwrap(), HashInfo{
          source: HashSource::Branch(BranchHashHistory{left:hash_A,right:hash_B}) ,parent: None});


    #[allow(non_snake_case)]
    let hash_C : HashValue = board.submit_leaf("C").unwrap();
    // we have now inserted C, which will not be merged with branchAB
    // as they are different depths and that would lead to an unbalanced tree.
    assert_eq!(board.get_hash_info(hash_C).unwrap().parent,None);
    assert_is_leaf(board.get_hash_info(hash_C).unwrap().source,"C");
    assert_eq!(board.get_all_published_roots().unwrap(),vec![]);
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![branch_AB,hash_C]);

    // now publish! This will publish branch_AB and hash_C.
    let published1 : HashValue = board.order_new_published_root().unwrap();
    match board.get_hash_info(published1).unwrap().source {
        HashSource::Root(RootHashHistory{timestamp:_,elements:e,prior:None}) =>
           assert_eq!(e,vec![branch_AB,hash_C]),
        _ => panic!("Should be a root"),
    }
    assert_eq!(board.get_all_published_roots().unwrap(),vec![published1]);
    assert_eq!(board.get_most_recent_published_root().unwrap(),Some(published1));
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![]);
    // branch_AB,hash_C are still parentless and can be merged with, but are no longer unpublished.

    // now, just to check it is fine, drop the connection, and open a new one,
    println!("Dropping the connection and opening a new one");
    drop(board);
    let opts = Opts::from_url(url)?;
    let backend = merkle_tree_bulletin_board_backend_mysql::BackendMysql{ connection: Mutex::new(Box::new(Conn::new(opts)?)) };
    let mut board = BulletinBoard::new(backend).unwrap();

    // check that the state is still good.

    assert_eq!(board.get_hash_info(hash_C).unwrap().parent,None);
    assert_is_leaf(board.get_hash_info(hash_C).unwrap().source,"C");
    match board.get_hash_info(published1).unwrap().source {
        HashSource::Root(RootHashHistory{timestamp:_,elements:e,prior:None}) =>
            assert_eq!(e,vec![branch_AB,hash_C]),
        _ => panic!("Should be a root"),
    }
    assert_eq!(board.get_all_published_roots().unwrap(),vec![published1]);
    assert_eq!(board.get_most_recent_published_root().unwrap(),Some(published1));
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![]);

    assert_eq!(board.backend.get_all_leaves_and_branches_without_a_parent().unwrap(),vec![hash_C,branch_AB]); // will be in that order for this particular implementation of this backend, but no particular reason that this be so ordered.

    println!("Everything seems good. Continuing...");


    // add another element D, which will merge with C, making branch_CD,
    // which will then merge with AB making a single tree ABCD.
    #[allow(non_snake_case)]
    let hash_D : HashValue = board.submit_leaf("D").unwrap();
    #[allow(non_snake_case)]
    let branch_CD : HashValue = board.get_hash_info(hash_C).unwrap().parent.unwrap();
    assert_eq!(board.get_hash_info(hash_D).unwrap().parent,Some(branch_CD));
    assert_is_leaf(board.get_hash_info(hash_D).unwrap().source,"D");
    #[allow(non_snake_case)]
    let branch_ABCD : HashValue = board.get_hash_info(branch_AB).unwrap().parent.unwrap();
    assert_eq!(board.get_hash_info(branch_CD).unwrap(),HashInfo{
        source: HashSource::Branch(BranchHashHistory{left:hash_C,right:hash_D}) ,
        parent: Some(branch_ABCD)});
    assert_eq!(board.get_hash_info(branch_ABCD).unwrap(),HashInfo{
        source: HashSource::Branch(BranchHashHistory{left:branch_AB,right:branch_CD}) ,
        parent: None});
    assert_eq!(board.get_all_published_roots().unwrap(),vec![published1]);
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![branch_ABCD]);

    // do another publication, which now only has to contain branchABCD which includes everything,
    // including things from before the last publication.
    let published2 = board.order_new_published_root().unwrap();
    match board.get_hash_info(published2).unwrap().source {
        HashSource::Root(RootHashHistory{timestamp:_,elements:e,prior:Some(prior)}) => {
            assert_eq!(e,vec![branch_ABCD]);
            assert_eq!(prior,published1);
        }
        _ => panic!("Should be a root"),
    }
    assert_eq!(board.get_all_published_roots().unwrap(),vec![published1,published2]);
    assert_eq!(board.get_most_recent_published_root().unwrap(),Some(published2));
    assert_eq!(board.get_parentless_unpublished_hash_values().unwrap(),vec![]);
    // branch_ABCD is still parentless and can be merged with, but is no longer unpublished.

    println!("Censoring the evil A.");
    // test censorship
    board.censor_leaf(hash_A)?;
    match board.get_hash_info(hash_A)?.source {
        HashSource::Leaf(LeafHashHistory{ data : None, .. }) => {}
        _ => panic!("hash_A should be a leaf with no data!"),
    }

    println!("All seems to work fine.");
    Ok(())
}