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
        console.log(data);
        const div = document.getElementById("PendingList");
        removeAllChildElements(div);
        for (const line of data) addLink(add(div,"div"),line);
    }
    getWebJSON("get_pending_hash_values",success,failure);
}

function updatePublishedHead() {
    function success(data) {
        console.log(data);
        const div = document.getElementById("CurrentPublishedRoot");
        removeAllChildElements(div);
        if (data) addLink(div,data);
    }
    getWebJSON("get_current_published_head",success,failure);
}

function addEntry() {
    let value_to_add = document.getElementById("entry").value;
    function success(result) {
        console.log(result);
        if (result.Ok) {
            const div = add(document.getElementById("status"),"div");
            div.appendChild(document.createTextNode("Added "+value_to_add+" got commitment "))
            addLink(div,result.Ok);
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

window.onload = function () {
    document.getElementById("AddEntry").onclick = addEntry;
    document.getElementById("entry").addEventListener("keyup",function(event) {
        if (event.key==="Enter") addEntry();
    });
    document.getElementById("DoMerkle").onclick = function () {
        function success(result) {
            console.log(result);
            if (result.Ok) {
                status("Made new published head "+result.Ok);
            } else status("Tried to make new published head, got error "+result.Err);
            updatePending();
            updatePublishedHead();
        }
        getWebJSON("request_new_published_head",success,failure,JSON.stringify(""),"application/json")
    }
    updatePending();
    updatePublishedHead();
}