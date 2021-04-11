# Bulletin board demo

This is a proof of concept for a server implementing a bulletin board and providing
proofs of inclusion via Merkle trees.

The API is a REST with JSON encoding.

There is a simple html/javascript client provided for testing that uses the REST API. These are in the WebResources folder.

## To run

* Make sure rust is installed on your computer.
* In this directory, execute `cargo run` which will download dependencies, compile, and run the server. You can stop it with control C.
* Open a web browser at http://localhost:8090

Everything is stored in memory; restarting the server will discard any changes.

