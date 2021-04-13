# Bulletin board demo

This is a proof of concept for a server implementing a bulletin board and providing
proofs of inclusion via Merkle trees.

There is a simple REST API with JSON encoding.

There is a simple html/javascript client provided for testing that uses the REST API. Associated files are in the WebResources folder.

## To run

* Make sure rust is installed on your computer.
* In this directory, execute `cargo run` which will download dependencies, compile, and run the server. You can stop it with control C.
* Open a web browser at http://localhost:8090

**Add an entry to the board** (http://localhost:8090/AddEntry.html) allows you to enter a series of strings, getting in
     response a hash value. The board is committing to include that hash value in the next published Merkle tree.
     Add at least one. The corresponding hash will appear in the status, with a link.

**Publish a new root**
     When you click 'Publish new root', they are hashed and then incorporated as leaf nodes into a new Merkle Tree. All prior
     entries are included.

**Get a proof of inclusion** Open (preferably in a new tab) one of the links for the hash values obtained for a string you entered.
     This gives information about that hash value, and how to rederive it yourself. Click on 'Show Full Text Inclusion Proof' to
      get a detailed proof linking your entered node to the newly published root.

It saves and loads data from the directory "csv_database". 
The files in there are human readable.

## Understanding

The general theory can be found here https://sites.google.com/site/certificatetransparency/log-proofs-work.

The following diagram shows the state of Merkle Tree after the inclusion of "a" and "b".

![Screen Shot 2021-04-12 at 15.33.47](README.assets/Screen Shot 2021-04-12 at 15.33.47.png)