"use strict";

const hashForThisPage = (new URL(document.location.href)).searchParams.get("hash");

function addTimestamp(where,timestamp) {
    const div = add(where,"div");
    const date = new Date(timestamp*1000);
    div.innerText="Timestamp : "+timestamp+"  which means "+date.toString();
}

/**
 * Make a div explaining where the hash for some explained hash has come from
 * @param where{HTMLElement} Where the text should go.
 * @param source{{Leaf:{timestamp:number,data:string},Branch:{left:string,right:string},Root:{timestamp:number,elements:[string]}}}
 * @param expecting{?string} Optional hash that we are expecting.
 * @param lookingFor{?string} Optional hash that should be included in this explanation and which we want to highlight.
 * @returns {Promise<{computedHashLocation:HTMLElement,foundLookingFor:HTMLElement}>} The HTML element containing the computed hash, and the element we were looking for. Or null if not found.
 */
async function explainHowHashWasComputed(where,source,expecting,lookingFor) {
    add(where,"h5").innerText="How the hash value was computed"
    let table = add(where,"table");
    let bytesToHash = [];
    let hexStrBuildup = "";
    let linuxCommand = "";
    let foundLookingFor = null;
    function hashComponent(title,contents,suffix,highlight) {
        const tr = add(table,"tr");
        add(tr,"td").innerText=title;
        const td = add(tr,"td");
        const contentsSpan = add(td,"span",highlight?"PartOfChain":null);
        if (highlight) foundLookingFor=contentsSpan;
        contentsSpan.innerText=contents;
        if (suffix) add(td,"span").innerText=suffix;
    }
    function hashHex(title,contents,numBytes) {
        if (typeof contents==="number") contents=contents.toString(16);
        contents = contents.padStart(2*numBytes,"0");
        hexStrBuildup+=contents;
        bytesToHash.push(...contents.match(/[\da-f]{2}/gi).map(h => parseInt(h, 16)));
        hashComponent(title,contents," ("+numBytes+" hex bytes)",contents===lookingFor);
    }
    function hashString(title,contents) {
        hashComponent(title,contents," ("+contents.length+" string bytes)",true);
        bytesToHash.push(...new TextEncoder().encode(contents));
        linuxCommand = "echo -n "+hexStrBuildup+' | xxd -r -p | cat - <(echo -n "'+contents+'")';
        hexStrBuildup = "";
    }
    if (source.Leaf) {
        hashHex("Leaf prefix",0,1);
        hashHex("Timestamp",source.Leaf.timestamp,8);
        hashString("Posted Data",source.Leaf.data);
    } else if (source.Branch) {
        hashHex("Branch prefix",1,1);
        hashHex("Left hash",source.Branch.left,32);
        hashHex("Right hash",source.Branch.right,32);
    } else if (source.Root) {
        hashHex("Published Root prefix",2,1);
        hashHex("Timestamp",source.Root.timestamp,8);
        for (const element of source.Root.elements) hashHex("Element",element,32);
    }
    // hash it.
    const hashBuffer = await crypto.subtle.digest('SHA-256', Uint8Array.from(bytesToHash));           // hash the message
    const hashArray = Array.from(new Uint8Array(hashBuffer));                     // convert buffer to byte array
    const computedHash = hashArray.map(b => b.toString(16).padStart(2, '0')).join(''); // convert bytes to hex string
    const computedHashDiv = add(where,"div");
    add(computedHashDiv,"span").innerText="The Sha256 hash of the above elements concatenated is ";
    const computedHashLocation = add(computedHashDiv,"span","PartOfChain");
    computedHashLocation.innerText=computedHash;
    if (expecting && expecting!==computedHash) {
        add(where,"b").innerText="THIS IS NOT WHAT IS EXPECTED. Something is going badly wrong";
    }
    if (hexStrBuildup.length>0) linuxCommand="echo -n "+hexStrBuildup+" | xxd -r -p";
    linuxCommand+=" | sha256sum";
    let linuxDiv = add(where,"div");
    add(linuxDiv,"div").innerText="This can be checked by the Linux command : "
    add(linuxDiv,"pre").innerText=linuxCommand;
    return { computedHashLocation:computedHashLocation,foundLookingFor:foundLookingFor};
}

function getBodyRelativePosition(elem) {
    const  box = elem.getBoundingClientRect();
    return { top : box.top+window.scrollY-document.body.clientTop, left: box.left+window.scrollX-document.body.clientLeft, width : box.width, height : box.height};
}

/**
 * Draw a line between two html elements, from the middle of e1 to the middle of e2.
 * @param e1{HTMLElement}
 * @param e2{HTMLElement}
 */
function drawLineBetween(e1,e2) {
    if (!(e1&&e2)) return; // don't draw if the elements don't exist.
    let p1 = getBodyRelativePosition(e1);
    let p2 = getBodyRelativePosition(e2);
    let x1 = p1.left+0.5*p1.width;
    let y1 = p1.top+0.5*p1.height;
    let x2 = p2.left+0.5*p2.width;
    let y2 = p2.top+0.5*p2.height;
    let xMax = Math.max(x1,x2);
    let yMax = Math.max(y1,y2);
    const svg = prependSVG(document.body,"svg");
    svg.style.position="absolute";
    svg.style.top="0";
    svg.style.left="0";
    svg.style["pointer-events"]="none";
    svg.setAttribute ("viewBox", "0 0 "+xMax+" "+yMax );
    svg.setAttribute ("width", xMax );
    svg.setAttribute ("height", yMax );
    const line = addSVG(svg,"line")
    line.setAttribute("x1", x1);
    line.setAttribute("x2", x2);
    line.setAttribute("y1", y1);
    line.setAttribute("y2", y2);
    line.setAttribute("stroke", "red");
    line.setAttribute("stroke-width", "3px");
}

function showTextInclusionProof(where,lastComputedHash) {
    removeAllChildElements(where);
    add(where,"h2").innerText="Full text inclusion proof"
    add(where,"p").innerText="The purpose of this is to demonstrate that this hash value is included in the bulletin board. This is done by showing a chain of hash values leading up to a published hash value. Reversing the Sha256 hash function is (as far as we can tell) impractical. This means that other people who see the same published hash value as you, can tell if something nefarious is attempted with this node. The above explanation of the hash value proves that this hash value represents the values it is claimed for at the top of this page."
    function failure(message) {
        add(where,"b").innerText="Error : "+message;
    }
    async function success(data) {
        if (data.Ok) {
            const proof = data.Ok;
            for (let i=1;i<proof.chain.length;i++) {
                let info = proof.chain[i];
                addLabeledLink(add(where,"p"),"This node's parent is ",info.hash);
                describeNode(where,info.source);
                let locations = await explainHowHashWasComputed(where, info.source, info.hash,proof.chain[i-1].hash);
                drawLineBetween(lastComputedHash,locations.foundLookingFor);
                lastComputedHash=locations.computedHashLocation;
            }
            if (proof.published_root) {
                addLabeledLink(add(where,"p"),"This node is listed in the published root node ",proof.published_root.hash);
                describeNode(where,proof.published_root.source);
                let locations = await explainHowHashWasComputed(where, proof.published_root.source, proof.published_root.hash,proof.chain.length>0?proof.chain[proof.chain.length-1].hash:null);
                drawLineBetween(lastComputedHash,locations.foundLookingFor);
            } else {
                add(where,"p").innerText="This node has not been published yet. Try refreshing this page after the next public published hash value"
            }
        } else failure(data.Err);
    }
    getWebJSON(getURL("get_proof_chain",{hash:hashForThisPage}),success,failure);
}

function getSourceOfHash(hash) {
    return new Promise((resolve, reject) => {
        function success(data) {
            if (data.Ok && data.Ok.source) resolve(data.Ok.source);
            else reject(data.Err);
        }
        getWebJSON(getURL("get_hash_info",{hash:hash}),success,reject);
    });
}

async function showTreeView(where) {
    removeAllChildElements(where);
    add(where,"h2").innerText="Tree view of children"
    const svg = addSVG(where,"svg");
    let children = [{hash:hashForThisPage}];
    let width = 1000;
    let y = 30;
    let rectHeight = 40;
    let maxRectWidth = 80;
    while (children.length>0) {
        // draw children
        let gap = width/(children.length+1);
        let rectWidth = Math.min(maxRectWidth,gap/2);
        let x = gap;
        let nextChildren = [];
        for (const e of children) {
            const mx = x+rectWidth/2;
            const source = await getSourceOfHash(e.hash);
            if (e.parent) {
                const line = addSVG(svg,"line","TreeView");
                line.setAttribute("x1",e.parent.x);
                line.setAttribute("y1",e.parent.y);
                line.setAttribute("x2",mx);
                line.setAttribute("y2",y);
            }
            const a = addSVG(svg,"a");
            a.setAttribute("href","LookupHash.html?hash="+e.hash);
            const rect = addSVG(a,"rect","TreeView");
            rect.setAttribute("x",x);
            rect.setAttribute("y",y);
            rect.setAttribute("width",rectWidth);
            rect.setAttribute("height",rectHeight);
            let text2 = null;
            let descendents = [];
            if (source.Leaf) { text2=source.Leaf.data; }
            else if (source.Branch) {descendents = [source.Branch.left, source.Branch.right]; }
            else if (source.Root) { descendents=source.Root.elements; }
            for (const d of descendents) nextChildren.push({hash:d, parent:{x:mx,y:y+rectHeight}});
            let hashText = addSVG(a,"text","TreeViewHash");
            hashText.setAttribute("x",mx);
            hashText.setAttribute("y",y+rectHeight*(text2?0.3:0.5));
            hashText.appendChild(document.createTextNode(e.hash.substring(0,4)+"…"));
            if (text2) {
                let hashText2 = addSVG(svg,"text","TreeViewLeaf");
                hashText2.setAttribute("x",mx);
                hashText2.setAttribute("y",y+rectHeight*0.7);
                hashText2.appendChild(document.createTextNode(text2));
            }
            x+=gap;
        }
        y+=100;
        children=nextChildren;
    }
    svg.setAttribute ("viewBox", "0 0 "+width+" "+y );
    svg.setAttribute ("width", width );
    svg.setAttribute ("height", y );
}

function describeNode(where,source) {
    if (source.Leaf) {
        add(where, "h5").innerText = "Leaf";
        addTimestamp(where, source.Leaf.timestamp);
        add(where, "div").innerText = "Data : " + source.Leaf.data;
    }
    if (source.Branch) {
        add(where, "h5").innerText = "Branch";
        addLabeledLink(add(where, "div"), "Left ", source.Branch.left);
        addLabeledLink(add(where, "div"), "Right ", source.Branch.right);
    }
    if (source.Root) {
        add(where, "h5").innerText = "Published Root";
        addTimestamp(where, source.Root.timestamp);
        for (const line of source.Root.elements) addLabeledLink(add(where, "div"), "Reference ", line);
    }

}
window.onload = function () {
    document.getElementById("MainHeading").innerText=hashForThisPage;
    const status = document.getElementById("status");
    function failure(message) {
        status.innerText="Error : "+message;
        console.log(message);
    }
    async function success(result) {
        // console.log(result);
        if (result.Ok) {
            result=result.Ok;
            if (result.parent) {
                addLabeledLink(add(status, "div"), "Parent ", result.parent);
            }
            if (result.source) {
                describeNode(status,result.source);
                const locations = await explainHowHashWasComputed(status, result.source, hashForThisPage);

                if (!result.source.Root) { // add text proof option
                    const textProofDiv = add(status,"div");
                    const textProofButton = add(textProofDiv,"button");
                    textProofButton.innerText="Show Full Text Inclusion Proof";
                    textProofButton.onclick=function () { showTextInclusionProof(textProofDiv,locations.computedHashLocation); }
                }

                if (!result.source.Leaf) { // add tree view option
                    const treeViewDiv = add(status,"div");
                    const treeViewButton = add(treeViewDiv,"button");
                    treeViewButton.innerText="Show Children of this node graphically";
                    treeViewButton.onclick=function () { showTreeView(treeViewDiv); }
                }
            }
        } else failure(result.Err);
    }
    getWebJSON(getURL("get_hash_info",{hash:hashForThisPage}),success,failure);
}