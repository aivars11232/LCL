"use strict";

// UI-01 acceptance against the complete, unmodified production script.
// The DOM is a controlled test double, not graphical/browser evidence. With
// --server, every request goes to the real product and disk bytes are checked.
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const path = require("node:path");
const vm = require("node:vm");
const { createHash } = require("node:crypto");

assert(Number(process.versions.node.split(".")[0]) >= 22, "editor regressions require Node >=22");
const hash = text => createHash("sha256").update(text).digest("hex");
const jsonReply = (value, status = 200) => new Response(JSON.stringify(value), { status });
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
async function bounded(promise, label) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label}: five-second deadline`)), 5000);
    })]);
  } finally { clearTimeout(timer); }
}

class Node {
  constructor(tag = "div") {
    this.tagName = tag;
    this.children = [];
    this.events = new Map();
    this.style = {};
    this.dataset = {};
    this.value = "";
    this.textContent = "";
    this.hidden = true;
    this.scrollTop = this.scrollLeft = this.selectionStart = this.selectionEnd = 0;
    const classes = new Set();
    this.classList = {
      add: (...items) => items.forEach(item => classes.add(item)),
      remove: (...items) => items.forEach(item => classes.delete(item)),
      toggle: (item, on) => on ? classes.add(item) : classes.delete(item),
    };
  }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  querySelector(selector) {
    for (const child of this.children) {
      if (selector.split(", ").includes(child.tagName)) return child;
      const found = child.querySelector(selector);
      if (found) return found;
    }
    return null;
  }
  addEventListener(name, callback) {
    if (!this.events.has(name)) this.events.set(name, []);
    this.events.get(name).push(callback);
  }
  dispatchEvent(event) { for (const callback of this.events.get(event.type) || []) callback(event); }
  focus() {}
  remove() {}
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
}

async function harness(options) {
  const origin = options.server ? new URL(options.server).origin : "http://127.0.0.1:12345";
  const token = options.server ? new URL(options.server).searchParams.get("t") : "fixture-token";
  const nodes = new Map();
  const get = selector => {
    if (!nodes.has(selector)) nodes.set(selector, new Node());
    return nodes.get(selector);
  };
  const document = new Node("document");
  document.querySelector = selector => selector.startsWith("meta[") ? { content: token } : get(selector);
  document.querySelectorAll = () => [];
  document.createElement = tag => new Node(tag);
  document.createTextNode = text => Object.assign(new Node("text"), { textContent: text });
  document.createDocumentFragment = () => new Node("fragment");
  document.body = new Node("body");
  document.documentElement = new Node("html");
  const stored = new Map();
  const puts = [];
  let nextHold = null;
  let failListing = false;
  const request = async (url, init = {}) => {
    url = new URL(url, origin);
    const method = init.method || "GET";
    const id = url.searchParams.get("id");
    const hold = method === "PUT" ? nextHold : null;
    if (hold) nextHold = null;
    if (method === "PUT") puts.push({ id, body: init.body });
    let reply;
    if (url.pathname === "/api/documents" && failListing) {
      failListing = false;
      reply = jsonReply({ error: "listing unavailable after persistence" }, 503);
    } else if (options.server) {
      reply = await fetch(url, {
        ...init,
        headers: { ...init.headers, Origin: origin, "X-LCL-Token": token },
        signal: AbortSignal.timeout(4000),
      });
    } else if (url.pathname === "/api/session") {
      reply = jsonReply({ root: "/fixture", spec: {
        formal_version: "0.1.0", authority: "authoritative", identity_digest: "fixture", root: "/spec",
      } });
    } else if (url.pathname === "/api/documents") {
      reply = jsonReply({ entries: [] });
    } else if (url.pathname === "/api/tokens") {
      reply = jsonReply({ tokens: [] });
    } else if (url.pathname === "/api/document" && method === "PUT") {
      if (init.body.includes("\r")) reply = jsonReply({ error: "carriage return refused" }, 422);
      else {
        const added = init.body.length > 0 && !init.body.endsWith("\n");
        const text = init.body + (added ? "\n" : "");
        stored.set(id, text);
        reply = jsonReply({ id, digest: hash(text), bytes: Buffer.byteLength(text), final_line_feed_added: added });
      }
    } else if (url.pathname === "/api/document" && method === "GET" && stored.has(id)) {
      const text = stored.get(id);
      reply = jsonReply({ id, text, digest: hash(text) });
    } else throw new Error(`unexpected fixture request ${method} ${url}`);
    if (hold) {
      hold.reached.resolve();
      await bounded(hold.release.promise, "held save response");
    }
    return reply;
  };
  const context = vm.createContext({
    document, location: { origin, search: `?t=${token}` }, fetch: request,
    URL, URLSearchParams, Event, getComputedStyle: () => ({ getPropertyValue: () => "20" }),
    // Analysis/toast timers are controlled; no production function is replaced.
    setTimeout: () => 1, clearTimeout: () => {}, console,
  });
  const source = options.server
    ? await (await fetch(new URL(`/app.js?t=${token}`, origin), { signal: AbortSignal.timeout(4000) })).text()
    : await fs.readFile(options.app, "utf8");
  await bounded(vm.runInContext(source, context, { filename: "production-app.js", timeout: 3000 }), "frontend boot");
  assert(!document.body.innerHTML, document.body.innerHTML || "frontend boot failed");
  const run = code => vm.runInContext(code, context, { timeout: 3000 });
  return {
    sourceHash: hash(source), puts, get, run,
    hold() {
      assert.equal(nextHold, null);
      const held = { reached: deferred(), release: deferred() };
      nextHold = held;
      return { reached: held.reached.promise, release: () => held.release.resolve() };
    },
    failNextListing() { failListing = true; },
    async add(id, text = "saved\n") {
      const reply = await request(new URL(`/api/document?id=${encodeURIComponent(id)}`, origin), { method: "PUT", body: text });
      assert(reply.ok, await reply.text());
      await bounded(run(`openDocument(${JSON.stringify(id)})`), "open document");
      puts.length = 0;
    },
    edit(text) { get("#code").value = text; get("#code").dispatchEvent(new Event("input")); },
    doc(id) { return JSON.parse(run(`JSON.stringify((() => { const d=state.docs.get(${JSON.stringify(id)}); return d ? {text:d.text,saved:d.saved,dirty:dirty(d)} : null; })())`)); },
    close(id) { run(`closeDocument(${JSON.stringify(id)})`); },
    choose(label) {
      const button = get("#modal-actions").children.find(child => child.textContent === label);
      assert(button, `missing modal action ${label}`);
      return button.onclick();
    },
    async persisted(id) {
      return options.project ? fs.readFile(path.join(options.project, id), "utf8") : stored.get(id);
    },
    reset() {
      assert.equal(nextHold, null, "a test leaked a pending held response");
      run("state.docs.clear(); state.order=[]; state.active=null; code.value=''; closeModal();");
      puts.length = 0;
      get("#toasts").replaceChildren();
    },
  };
}

const cases = [
  ["failed close-save retains document, edits and dirty state", async h => {
    await h.add("failed.lcl.txt"); h.edit("unsaved\rtext");
    h.close("failed.lcl.txt"); await bounded(h.choose("Save"), "failed close save");
    assert.deepEqual(h.doc("failed.lcl.txt"), { text: "unsaved\rtext", saved: "saved\n", dirty: true });
    assert.equal(await h.persisted("failed.lcl.txt"), "saved\n");
    assert(h.get("#toasts").children.some(node => node.textContent.startsWith("Not saved.")));
  }],
  ["closing inactive A saves A and preserves active B", async h => {
    await h.add("inactive A.lcl.txt"); h.edit("A edits\n");
    await h.add("active B.lcl"); h.edit("B unsaved\n");
    h.close("inactive A.lcl.txt"); await bounded(h.choose("Save"), "inactive close save");
    assert.equal(await h.persisted("inactive A.lcl.txt"), "A edits\n");
    assert.equal(h.doc("inactive A.lcl.txt"), null);
    assert.equal(h.doc("active B.lcl").text, "B unsaved\n");
    assert.equal(h.doc("active B.lcl").dirty, true);
    assert.equal(await h.persisted("active B.lcl"), "saved\n");
    assert.equal(h.get("#code").value, "B unsaved\n");
  }],
  ["delayed acknowledgement leaves newer edits dirty", async h => {
    await h.add("delayed.lcl.txt"); h.edit("submitted");
    const hold=h.hold(), saving=h.run("save()");
    await bounded(hold.reached,"save submitted"); h.edit("newer unsent"); hold.release();
    const result = await bounded(saving,"save acknowledged");
    assert.deepEqual(h.doc("delayed.lcl.txt"), { text:"newer unsent", saved:"submitted\n", dirty:true });
    assert.equal(h.get("#code").value,"newer unsent");
    assert.equal(await h.persisted("delayed.lcl.txt"),"submitted\n");
    assert.equal(result, true);
  }],
  ["normalizing inactive save cannot overwrite another tab's textarea", async h => {
    await h.add("switch A.lcl.txt"); h.edit("A without newline");
    const hold=h.hold(), saving=h.run("save()"); await bounded(hold.reached,"save submitted");
    await h.add("switch B.lcl"); h.edit("B current\n"); hold.release();
    await bounded(saving,"inactive acknowledgement");
    assert.equal(h.get("#code").value,"B current\n");
    assert.deepEqual(h.doc("switch A.lcl.txt"), { text:"A without newline\n", saved:"A without newline\n", dirty:false });
  }],
  ["overlapping saves serialize submitted revisions and final disk bytes", async h => {
    await h.add("overlap.lcl.txt"); h.edit("first\n");
    const hold=h.hold(), first=h.run("save()"); await bounded(hold.reached,"first submitted");
    h.edit("second\n"); const second=h.run("save()"); await tick();
    const beforeRelease = h.puts.length;
    hold.release();
    const results=await bounded(Promise.all([first,second]),"both saves");
    assert.equal(beforeRelease,1,"second write must wait for first acknowledgement");
    assert.deepEqual(results,[true,true]);
    assert.equal(h.doc("overlap.lcl.txt").dirty,false);
    assert.equal(h.doc("overlap.lcl.txt").saved,"second\n");
    assert.equal(await h.persisted("overlap.lcl.txt"),"second\n");
  }],
  ["edits during close-save keep the tab open", async h => {
    await h.add("close delay.lcl.txt"); h.edit("submitted\n");
    const hold=h.hold(); h.close("close delay.lcl.txt"); const closing=h.choose("Save");
    await bounded(hold.reached,"close submitted"); h.edit("later edits\n"); hold.release();
    await bounded(closing,"close acknowledged");
    assert.equal(h.doc("close delay.lcl.txt").text,"later edits\n");
    assert.equal(h.doc("close delay.lcl.txt").dirty,true);
  }],
  ["pending save prevents silent close after text reverts to old baseline", async h => {
    await h.add("pending.lcl.txt"); h.edit("in flight\n");
    const hold=h.hold(), saving=h.run("save()"); await bounded(hold.reached,"pending write");
    h.edit("saved\n"); h.close("pending.lcl.txt");
    const remainedOpen = h.doc("pending.lcl.txt");
    hold.release();
    await bounded(saving,"pending acknowledged");
    assert(remainedOpen,"pending work must keep the document open");
    assert.equal(h.doc("pending.lcl.txt").dirty,true);
  }],
  ["old response cannot alter a discarded and reopened document", async h => {
    await h.add("reopened.lcl.txt"); h.edit("old request\n");
    const hold=h.hold(), saving=h.run("save()"); await bounded(hold.reached,"old write");
    h.close("reopened.lcl.txt"); h.choose("Discard");
    await bounded(h.run('openDocument("reopened.lcl.txt")'),"reopen"); h.edit("new document edits\n");
    hold.release(); await bounded(saving,"old response");
    assert.equal(h.doc("reopened.lcl.txt").text,"new document edits\n");
    assert.equal(h.doc("reopened.lcl.txt").dirty,true);
  }],
  ["Cancel retains changes and explicit Discard closes without saving", async h => {
    await h.add("cancel.lcl.txt"); h.edit("discard me\n"); h.close("cancel.lcl.txt"); h.choose("Cancel");
    assert(h.doc("cancel.lcl.txt").dirty); h.close("cancel.lcl.txt"); h.choose("Discard");
    assert.equal(h.doc("cancel.lcl.txt"),null); assert.equal(h.puts.length,0);
    assert.equal(await h.persisted("cancel.lcl.txt"),"saved\n");
  }],
  ["successful toolbar Save acknowledges final LF and can close", async h => {
    await h.add("legacy spaces.lcl"); h.edit("final text");
    assert.equal(await bounded(h.get("#act-save").onclick(new Event("click")),"toolbar save"),true);
    assert.deepEqual(h.doc("legacy spaces.lcl"),{text:"final text\n",saved:"final text\n",dirty:false});
    h.close("legacy spaces.lcl"); assert.equal(h.doc("legacy spaces.lcl"),null);
    assert.equal(await h.persisted("legacy spaces.lcl"),"final text\n");
  }],
  ["post-save tree failure is not reported as persistence failure", async h => {
    await h.add("refresh.lcl.txt"); h.edit("persisted\n"); h.failNextListing();
    assert.equal(await bounded(h.run("save()"),"save despite listing failure"),true);
    assert.equal(h.doc("refresh.lcl.txt").dirty,false);
    assert.equal(await h.persisted("refresh.lcl.txt"),"persisted\n");
    assert(!h.get("#toasts").children.some(node => node.textContent.startsWith("Not saved.")));
  }],
];

async function main() {
  const options = { app: path.join(__dirname,"../assets/app.js") };
  for (let i=2;i<process.argv.length;i+=2) {
    const key=process.argv[i].slice(2);
    assert(["app","server","project"].includes(key),`unexpected option ${key}`);
    assert(process.argv[i+1],`missing value for ${key}`); options[key]=process.argv[i+1];
  }
  assert(!options.server || options.project,"real-server mode requires --project for disk assertions");
  const h=await harness(options);
  console.log(`production app.js sha256 ${h.sourceHash}; transport ${options.server ? "real HTTP + disk" : "controlled"}; DOM test double`);
  let failures=0;
  for (const [name, test] of cases) {
    h.reset();
    try { await test(h); console.log(`PASS ${name}`); }
    catch (error) { failures++; console.error(`FAIL ${name}\n${error.stack}`); }
  }
  console.log(`${cases.length-failures} passed; ${failures} failed; 0 skipped`);
  process.exitCode=failures ? 1 : 0;
}
main().catch(error => { console.error(error.stack); process.exitCode=1; });
