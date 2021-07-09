# Bulletin board demo

This is a proof of concept for a server implementing a bulletin board and providing
proofs of inclusion via Merkle trees.

There is a simple REST API with JSON encoding. Run `cargo doc` to generate rust docs - the API is a simple wrapper around the functions described in `target/doc/bulletin_board_demo/datasource/struct.DataSource.html`

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

<img src="README.assets/Screen Shot 2021-04-12 at 15.33.47.png" class="img-responsive" alt=""> </div>

The bulletin board is built up incrementally as new nodes are added, with a "pending" list showing those that have not yet been incorporated into a full (sub-)tree, which needs 2^n nodes (for some n).  For example, when there are 4 elements, they are all included in a built tree, but when a 5th is added it is "pending" until another node is added.  The 5th and 6th nodes are combined into a pair, which is "pending" until a 7th and 8th are added, allowing the full tree to be built.

If the user clicks "publish a new root" then all the pending nodes are included into the tree.  This is not guaranteed to produce a balanced tree, or even one in which the second-last row is full, but it does guarantee that the depth is no more than the log of the tree's total number of leaves, unless the computation has been interrupted. We call the root of this tree the **pre-root**.  The pre-root is then hashed, **along with the complete list of all other pre-roots generated so far** with a 02 byte and a timestamp prepended - the resulting hash is called the **published root**.

The user can then continue add new strings, which are used to make a new tree.  This can be used to make many different trees.

There are three kinds of nodes, distinguished by a different prefix byte:

- leaf nodes hash a 00 byte, an 8-byte timestamp, then the data;
- intermediate nodes, including the pre-root, hash a 01 byte, an 8-byte timestamp, then the child hashes;
- the published root node hashes a 02 byte, an 8-byte timestamp, and the list of all pre-roots computed so far.

This is intended to make an easy way for producing a new tree at regular intervals, such as daily.  At any time, a proof of inclusion into the current published root via any (current or past) tree can be requested.  The proof consists of

- a list of sibling hashes for the tree in which the data item is included (i.e. a standard Merle-Tree inclusion proof, for the relevant pre-root); and
- the list of all pre-roots included in the published root
- the list of relevant timestamps
