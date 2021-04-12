"use strict";

function addLink(where,hashvalue) {
    const link = add(where,"a");
    link.innerText=hashvalue;
    link.href = "LookupHash.html?hash="+hashvalue;
}

/** Add a text node
 * @param where{HTMLElement}
 * @param label{string}
 */
function addText(where,label) {
    where.appendChild(document.createTextNode(label));
}

function addLabeledLink(where,label,hashvalue) {
    addText(where,label);
    addLink(where,hashvalue);
}