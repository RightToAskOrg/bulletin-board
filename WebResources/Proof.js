"use strict";

/**
 * Explain the proof
 * @param proof{{leaf:string,index:number,proof:[string]}} The proof the explain
 * @param explainDiv{HTMLElement} Where to put the explanation
 */
async function explain(proof, explainDiv) {
    function explainSingleHash(input,output,isHexString) {
        const div = add(explainDiv,"div");
        div.appendChild(document.createTextNode("The hash of "+(isHexString?"the hex encoded data ":"the string ")+input+" is "));
        add(div,"b").innerText=output;
        div.appendChild(document.createTextNode(" which can be verified by the Linux command : "));
        add(div,"code").innerText="echo -n "+input+(isHexString?" | xxd -r -p":"")+" | sha256sum";
    }
    add(explainDiv, "div").innerText = "We are trying to prove that `" + proof.leaf + "' is contained in the Merkle tree."
    const leafHash = await hashString(proof.leaf);
    explainSingleHash(proof.leaf,leafHash,false);
    const leafHash2 = await hashHex("00"+leafHash);
    add(explainDiv,"div").innerText = "This is converted into a leaf hash by hashing it again, first prepending the byte 0.";
    explainSingleHash("00"+leafHash,leafHash2,true);
    if (proof.proof[0]===leafHash2) add(explainDiv,"div").innerText="This matches the first hash value in the raw proof above.";
    else {
        add(explainDiv,"b").innerText="This should match the first hash value in the raw proof above. As it doesn't, this proof is invalid!";
        return;
    }
    let index = proof.index;
    let lemmas = proof.proof.slice(1,proof.proof.length-1);
    let lastHash = leafHash2;
    while (lemmas.length>0) {
        const toAdd = lemmas[0];
        lemmas = lemmas.slice(1);
        const isLeft = index%2===0;
        index=Math.floor(index/2);
        add(explainDiv,"div").innerText="The next hash value in the proof is "+toAdd+" which is the neighbouring node to the "+(isLeft?"right":"left")+" which is concatenated with our last hash with the byte 1 at the start.";
        const input = "01"+(isLeft?lastHash:toAdd)+(isLeft?toAdd:lastHash);
        const output = await hashHex(input);
        explainSingleHash(input,output,true);
        lastHash=output;
    }
    if (proof.proof[proof.proof.length-1]===lastHash) add(explainDiv,"div").innerText="This matches the last hash value in the raw proof above. This is the root of the Merkle tree. This can be verified (in an ideal world) by travelling to Flinders St station, and gazing up in awe and delight at the publicly announced root hash on the billboards there.";
    else {
        add(explainDiv,"b").innerText="This should match the last hash value in the raw proof above. As it doesn't, this proof is invalid!";
    }
}

/** Hash a string, producing a hexadecimal string as result.
    basically copied from MDN documentation on crypto.subtle.digest */
async function hashString(message) {
    const msgUint8 = new TextEncoder().encode(message);                           // encode as (utf-8) Uint8Array
    const hashBuffer = await crypto.subtle.digest('SHA-256', msgUint8);           // hash the message
    const hashArray = Array.from(new Uint8Array(hashBuffer));                     // convert buffer to byte array
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join(''); // convert bytes to hex string
}

/** Hash a hexadecimal string, producing a hexadecimal string as result. */
async function hashHex(hexstr) {
    const msgUint8 = new Uint8Array(hexstr.match(/[\da-f]{2}/gi).map(h => parseInt(h, 16))); // credit Gabriel Hautclocq in stack overflow https://stackoverflow.com/questions/53203560/how-to-generate-sha256-hashes-of-hexadecimal-binary-data-in-javascript
    const hashBuffer = await crypto.subtle.digest('SHA-256', msgUint8);           // hash the message
    const hashArray = Array.from(new Uint8Array(hashBuffer));                     // convert buffer to byte array
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join(''); // convert bytes to hex string
}




function getProof() {
    const index = document.getElementById("entry").value;
    console.log(index);
    const rawDiv = document.getElementById("RawResult")
    const explainDiv = document.getElementById("Explanation")
    function success(proof) {
        rawDiv.innerText=JSON.stringify(proof);
        removeAllChildElements(explainDiv);
        explain(proof,explainDiv);
    }
    function failure(error) {
        rawDiv.innerText=error;
        removeAllChildElements(explainDiv);
    }
    getWebJSON(getURL("get_merkle_proof",{index:index}),success,failure);
}


window.onload = function () {
    document.getElementById("GetProof").onclick = getProof;
    document.getElementById("entry").addEventListener("keyup",function(event) {
        if (event.key==="Enter") getProof();
    });
    getProof();
}