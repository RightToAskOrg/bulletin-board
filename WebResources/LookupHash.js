"use strict";

const hash = (new URL(document.location.href)).searchParams.get("hash");


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
        console.log(data);
        const div = document.getElementById("PendingList");
        removeAllChildElements(div);
        for (const line of data) add(div,"div").innerText=line;
    }
    getWebJSON("get_pending_hash_values",success,failure);
}

function updatePublishedHead() {
    function success(data) {
        console.log(data);
        const div = document.getElementById("CurrentPublishedRoot");
        removeAllChildElements(div);
        if (data) div.innerText=data;
    }
    getWebJSON("get_current_published_head",success,failure);
}

function addEntry() {
    let value_to_add = document.getElementById("entry").value;
    function success(result) {
        console.log(result);
        if (result.Ok) {
            status("Added "+value_to_add+" got commitment "+result.Ok);
            document.getElementById("entry").value="";
        } else {
            status("Tried to add "+value_to_add+" got Error message "+result.Err);
        }
        updatePending();
    }
    const message = {
        data : value_to_add
    }
    getWebJSON("submit_entry",success,failure,JSON.stringify(message),"application/json")
}

function addTimestamp(where,timestamp) {
    const div = add(where,"div");
    div.innerText="Timestamp : "+timestamp;
}


window.onload = function () {
    document.getElementById("MainHeading").innerText=hash;
    const status = document.getElementById("status");
    function success(result) {
        console.log(result);
        if (result.parent) {
            addLabeledLink(add(status,"div"),"Parent ",result.parent);
        }
        if (result.source) {
            if (result.source.Leaf) {
                add(status,"h5").innerText="Leaf";
                const source = result.source.Leaf;
                addTimestamp(status,source.timestamp);
                add(status,"div").innerText="Data : "+source.data;
            }
            if (result.source.Branch) {
                add(status,"h5").innerText="Branch";
                const source = result.source.Branch;
                addTimestamp(status,source.timestamp);
                addLabeledLink(add(status,"div"),"Left ",source.left);
                addLabeledLink(add(status,"div"),"Right ",source.right);
            }
            if (result.source.Root) {
                add(status,"h5").innerText="Published Root";
                const source = result.source.Root;
                addTimestamp(status,source.timestamp);
                for (const line of source.elements) addLabeledLink(add(status,"div"),"Reference ",line);
            }
        }
    }
    function failure(message) {
        status.innerText="Error : "+message;
        console.log(message);
    }
    getWebJSON(getURL("lookup_hash",{hash:hash}),success,failure);
}