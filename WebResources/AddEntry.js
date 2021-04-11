"use strict";



/** Add a line to the status display.
 * @param line{string} line to add to the status */
function status(line) {
    add(document.getElementById("status"),"div").innerText=line;
}
function failure(error) {
    status("Error : "+error);
}

function updatePending() {
    function success(data) {
        // console.log(data);
        const div = document.getElementById("PendingList");
        removeAllChildElements(div);
        for (const line of data) add(div,"div").innerText=JSON.stringify(line);
    }
    getWebJSON("get_pending",success,failure);
}

function updateMerkleTrees() {
    function success(data) {
        // console.log(data);
        const div = document.getElementById("MerkleTreeList");
        removeAllChildElements(div);
        for (const line of data) add(div,"div").innerText=JSON.stringify(line);
    }
    getWebJSON("get_merkle_trees",success,failure);
}

function addEntry() {
    let value_to_add = document.getElementById("entry").value;
    function success(result) {
        status("Added "+value_to_add);
        document.getElementById("entry").value="";
        updatePending();
    }
    const message = {
        hash : value_to_add
    }
    getWebJSON("add_to_board",success,failure,JSON.stringify(message),"application/json")
}

window.onload = function () {
    document.getElementById("AddEntry").onclick = addEntry;
    document.getElementById("entry").addEventListener("keyup",function(event) {
        if (event.key==="Enter") addEntry();
    });
    document.getElementById("DoMerkle").onclick = function () {
        function success(result) {
            status("Made Merkle Tree "+result);
            updatePending();
            updateMerkleTrees();
        }
        getWebJSON("initiate_merkle_now",success,failure,JSON.stringify(""),"application/json")
    }
    updatePending();
    updateMerkleTrees();
}