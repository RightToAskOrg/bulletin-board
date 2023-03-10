# Merkle tree Bulletin board, mysql backend

This is a mysql/mariadb based backend for the merkle-tree-bulletin-board crate.

It is partly present as a demonstration of how a sql based backend could be,
but it is also usable in its own right.

A provided binary test_mysql is available; this is only for demo purposes and
has no function in a production system.

I am not a SQL tuning expert; this is not as careful code as the
merkle-tree-bulletin-board crate, however every operation is 
O(mysql single indexed operation)*O(data size) and data size is generally
O(log bulletin board size). That is, no operation should take long.

## How to use

Define some function to get a mysql connection to the data base such as
```rust
fn get_bulletin_board_connection() -> Conn {
    let opts = Opts::from_url(&CONFIG.database.bulletinboard).expect("Could not parse bulletin_board_url url");
    Conn::new(opts).expect("Could not connect to bulletin board database")
}
```

Then initialise the database with something such as
```rust
/// Delete all data and recreate the schema.
pub fn initialize_bulletin_board_database() -> anyhow::Result<()> {
    let mut conn = get_bulletin_board_connection();
    conn.query_drop("drop table if exists PUBLISHED_ROOTS")?;
    conn.query_drop("drop table if exists PUBLISHED_ROOT_REFERENCES")?;
    conn.query_drop("drop table if exists BRANCH")?;
    conn.query_drop("drop table if exists LEAF")?;

    let schema = merkle_tree_bulletin_board_backend_mysql::SCHEMA;
    conn.query_drop(schema)?;
    Ok(())
}
```

Then create a backend by something like
```rust
   let conn = get_bulletin_board_connection();
   let backend = merkle_tree_bulletin_board_backend_mysql::BackendMysql{ connection: std::sync::Mutex::new(Box::new(conn)) };
```

## License

Copyright 2021 Thinking Cybersecurity Pty. Ltd.

Licensed under either of

* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Version notes

0.2: Change of mysql version in dependencies. Old version had a transitive dependency funty 1.2 that was yanked.

0.3: Change to match 0.3 bulletin board - better error handling (API change for errors).
