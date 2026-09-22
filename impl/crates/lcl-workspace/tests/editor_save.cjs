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
/// Let every already-scheduled continuation run. The fixture transport answers
/// synchronously, so this is a bounded drain and not a sleep.
async function settle() {
  for (let i = 0; i < 50; i++) await tick();
}

/// Wait for a condition, bounded, then confirm it still holds after everything
/// else has run. A condition that is briefly true and then clobbered is not a
/// pass.
async function until(condition, label, ms = 4000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (condition()) {
      await settle();
      assert(condition(), `${label}: held briefly and was then undone`);
      return;
    }
    await tick();
  }
  await settle();
  assert(condition(), `${label}: not reached within ${ms}ms`);
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
    let hold = null;
    if (nextHold && method === nextHold.method && (!nextHold.path || url.pathname === nextHold.path)) {
      if (nextHold.skip > 0) nextHold.skip--;
      else { hold = nextHold; nextHold = null; }
    }
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
      // One token spanning exactly the bytes that were submitted. The real
      // engine's tokens cover its input the same way, so an assertion about
      // how far the applied tokens reach means the same thing in either mode.
      reply = jsonReply({
        tokens: [{ class: "ident", start: 0, end: Buffer.byteLength(init.body || "") }],
      });
    } else if (url.pathname === "/api/inspect") {
      // Accepted or rejected according to the submitted bytes, which is the
      // property these cases use to tell one document's answer from another's.
      const accepted = (init.body || "").includes("SPECIFICATION:");
      reply = jsonReply({
        outcome: accepted ? "accepted" : "rejected",
        reached: "static_checking",
        diagnostics: [],
        units: [],
        navigation: null,
      });
    } else if (url.pathname === "/api/check") {
      const accepted = (init.body || "").includes("SPECIFICATION:");
      reply = jsonReply({
        outcome: accepted ? "accepted" : "rejected",
        reached: "static_checking",
        diagnostics: [],
        units: [],
      });
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
    URL, URLSearchParams, Event, TextEncoder,
    getComputedStyle: () => ({ getPropertyValue: () => "20" }),
    // Analysis/toast timers are controlled; no production function is replaced.
    setTimeout: () => 1, clearTimeout: () => {}, console,
  });
  const source = options.server
    ? await (await fetch(new URL(`/app.js?t=${token}`, origin), { signal: AbortSignal.timeout(4000) })).text()
    : await fs.readFile(options.app, "utf8");
  await bounded(vm.runInContext(source, context, { filename: "production-app.js", timeout: 3000 }), "frontend boot");
  assert(!document.body.children.length, document.body.children.map(node => node.textContent).join("\n") || "frontend boot failed");
  const run = code => vm.runInContext(code, context, { timeout: 3000 });
  return {
    sourceHash: hash(source), puts, get, run,
    /// Hold one matching request until the case releases it.
    ///
    /// `path` and `skip` exist because one production call can make several
    /// requests — `runAnalysis` asks for tokens before it asks for the
    /// analysis — and a case about the second one must not stop the first.
    hold(method = "PUT", { path = null, skip = 0 } = {}) {
      assert.equal(nextHold, null);
      const held = { method, path, skip, reached: deferred(), release: deferred() };
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
    /// Dispatch one keydown at the production listener.
    ///
    /// The event goes to the same `document` the script registered on, so what
    /// answers it is the shipped handler and its real branch order.
    press(key, modifiers = {}) {
      run(`(() => {
        const e = new Event("keydown");
        e.key = ${JSON.stringify(key)};
        e.ctrlKey = ${Boolean(modifiers.ctrl)};
        e.metaKey = ${Boolean(modifiers.meta)};
        e.shiftKey = ${Boolean(modifiers.shift)};
        e.preventDefault = () => {};
        document.dispatchEvent(e);
      })()`);
    },
    /// The text of every toast raised so far.
    toasts() {
      return get("#toasts").children.map(node => node.textContent);
    },
    /// How far the tokens currently applied to one document reach.
    ///
    /// Tokens describe the bytes they were produced from, so a set that
    /// reaches past the document's current bytes was produced from other
    /// bytes — which is exactly the claim "these tokens describe this text"
    /// being false.
    tokenReach(id) {
      return JSON.parse(run(`JSON.stringify((() => {
        const d = state.docs.get(${JSON.stringify(id)});
        if (!d || !d.tokens) return null;
        return d.tokens.reduce((max, t) => Math.max(max, t.end), 0);
      })())`));
    },
    byteLength(id) {
      return JSON.parse(run(`JSON.stringify((() => {
        const d = state.docs.get(${JSON.stringify(id)});
        return d ? new TextEncoder().encode(d.text).length : null;
      })())`));
    },
    outcome(id) {
      return JSON.parse(run(`JSON.stringify((() => {
        const d = state.docs.get(${JSON.stringify(id)});
        return d && d.report ? d.report.outcome : null;
      })())`));
    },
    active() { return run("state.active"); },
    /// Everything the diagnostics panel is currently showing.
    ///
    /// The panel is shared by every tab, so what it says is a claim about the
    /// document the person is looking at — whichever document's answer put it
    /// there.
    diagnosticsText() {
      const collect = node =>
        [node.textContent || ""].concat(node.children.flatMap(collect)).join(" ");
      return collect(get('.view[data-view="diagnostics"]')).trim();
    },
    reset() {
      assert.equal(nextHold, null, "a test leaked a pending held response");
      run("state.docs.clear(); state.order=[]; state.active=null; code.value=''; closeModal();");
      puts.length = 0;
      get("#toasts").replaceChildren();
    },
  };
}

/// A document the engine accepts, used to tell one answer from another.
const VALID_DOCUMENT = [
  "LCL:",
  "    VERSION: \"0.1.0\"",
  "",
  "SPECIFICATION:",
  "    ID: example.editor",
  "    NAME: \"Editor fixture\"",
  "    VERSION: \"1.0.0\"",
  "    KIND: kind.data",
  "",
  "DATA:",
  "    ID: data.subject",
  "    TYPE: INTEGER",
  "    VALUE: 1",
  "",
].join("\n");

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
  // UI-02: a reload response may only replace what it was asked about.
  ["reload from a clean buffer does not overwrite text typed while it was in flight", async h => {
    await h.add("reload clean.lcl.txt");
    const hold = h.hold("GET"), reloading = h.run("reload()");
    await bounded(hold.reached, "reload requested");
    h.edit("typed while reloading\n");
    hold.release();
    await bounded(reloading, "reload answered");
    await until(
      () => h.doc("reload clean.lcl.txt").text === "typed while reloading\n",
      "edits made during a reload survive it",
    );
    assert.equal(h.doc("reload clean.lcl.txt").dirty, true, "and are still unsaved");
    assert.equal(h.get("#code").value, "typed while reloading\n");
  }],
  ["a confirmed discard covers the edits it was shown, not later ones", async h => {
    await h.add("reload dirty.lcl.txt");
    h.edit("discarded edits\n");
    const hold = h.hold("GET");
    h.run("reload()");
    h.choose("Discard and reload");
    await bounded(hold.reached, "reload requested after the discard");
    h.edit("typed after the discard\n");
    hold.release();
    await until(
      () => h.doc("reload dirty.lcl.txt").text === "typed after the discard\n",
      "text typed after the discard decision is not covered by it",
    );
    assert.equal(h.doc("reload dirty.lcl.txt").dirty, true);
  }],
  ["a reload response cannot reach a document that was closed and reopened", async h => {
    await h.add("reload reopen.lcl.txt");
    const hold = h.hold("GET");
    h.run("reload()");
    await bounded(hold.reached, "reload requested");
    h.close("reload reopen.lcl.txt");
    await bounded(h.run('openDocument("reload reopen.lcl.txt")'), "reopen");
    h.edit("edits in the reopened document\n");
    hold.release();
    await until(
      () => h.doc("reload reopen.lcl.txt").text === "edits in the reopened document\n",
      "an old reload cannot alter a reopened document",
    );
    assert.equal(h.doc("reload reopen.lcl.txt").dirty, true);
  }],
  ["a reload of an inactive tab does not touch the active one", async h => {
    await h.add("reload A.lcl.txt");
    const hold = h.hold("GET"), reloading = h.run("reload()");
    await bounded(hold.reached, "reload requested for A");
    await h.add("reload B.lcl");
    h.edit("B is being edited\n");
    hold.release();
    await bounded(reloading, "reload answered");
    await settle();
    assert.equal(h.get("#code").value, "B is being edited\n", "the active tab is untouched");
    assert.equal(h.doc("reload B.lcl").text, "B is being edited\n");
    assert.equal(h.doc("reload A.lcl.txt").text, "saved\n", "and A did reload");
    assert.equal(h.doc("reload A.lcl.txt").dirty, false);
  }],
  ["post-save tree failure is not reported as persistence failure", async h => {
    await h.add("refresh.lcl.txt"); h.edit("persisted\n"); h.failNextListing();
    assert.equal(await bounded(h.run("save()"),"save despite listing failure"),true);
    assert.equal(h.doc("refresh.lcl.txt").dirty,false);
    assert.equal(await h.persisted("refresh.lcl.txt"),"persisted\n");
    assert(!h.get("#toasts").children.some(node => node.textContent.startsWith("Not saved.")));
  }],
  // -------------------------------------------------------------------------
  // N-02 — Shift+F12 reaches its own binding
  // -------------------------------------------------------------------------
  //
  // The listener tested `e.key === "F12"` before the Shift branch, and that
  // test is true whether or not Shift is held. Shift+F12 therefore ran
  // `goToDefinition`, and the `findReferences` branch behind it could not be
  // reached at all.
  //
  // The two are told apart by the message each raises when the cursor is on
  // nothing: they differ, and only the one that actually ran says its own.
  ["F12 and Shift+F12 reach their own bindings", async h => {
    await h.add("keys.lcl.txt");
    // Resolver data the navigation functions can consult, holding nothing, so
    // each takes its "cursor is on nothing" branch and names itself.
    h.run(`state.docs.get("keys.lcl.txt").navigation = { declarations: [], references: [] };`);

    h.get("#toasts").replaceChildren();
    h.press("F12");
    assert.deepEqual(h.toasts(), ["The cursor is not on a reference."], "F12 goes to a definition");

    h.get("#toasts").replaceChildren();
    h.press("F12", { shift: true });
    assert.deepEqual(
      h.toasts(),
      ["Put the cursor on a declaration or a reference."],
      "Shift+F12 finds references",
    );

    h.get("#toasts").replaceChildren();
    h.press("F", { ctrl: true, shift: true });
    assert.deepEqual(
      h.toasts(),
      ["Put the cursor on a declaration or a reference."],
      "Ctrl+Shift+F finds references, as it always did",
    );
  }],
  // -------------------------------------------------------------------------
  // A-05 — a reply describes the text it was asked about, or it is discarded
  // -------------------------------------------------------------------------
  //
  // `refreshTokens`, `runAnalysis` and the explicit check each capture the
  // current document, send its text, and assign the answer when it comes back.
  // Nothing between those two moments is checked, so an answer about text that
  // no longer exists was stored as a description of the text that does — and
  // painted.
  //
  // The document already carries the fact that settles it. `revision` is bumped
  // on every edit, and `save` and `reload` already refuse to apply an answer
  // whose revision has moved on. These cases hold that same rule to the three
  // paths that did not apply it.
  //
  // Revision is the whole of the question for these answers, because each is a
  // function of the submitted bytes alone: two replies for one revision are
  // interchangeable, and a reply for any other revision describes other bytes.

  ["an edit during an outstanding token request discards the stale answer", async h => {
    // Opened short, so the tokens already applied describe six bytes. The
    // request that is held describes the long text, and by the time it comes
    // back the document is short again — so applying it would replace a
    // description that fits with one that does not.
    await h.add("stale-tokens.lcl.txt", "short\n");
    h.edit("a document long enough to tell apart\n");
    const hold = h.hold("POST");
    const pending = h.run("refreshTokens()");
    await bounded(hold.reached, "token request reached the fixture");
    h.edit("short\n");
    hold.release();
    await bounded(pending, "token request answered");
    await settle();
    assert(
      h.tokenReach("stale-tokens.lcl.txt") <= h.byteLength("stale-tokens.lcl.txt"),
      `tokens reach ${h.tokenReach("stale-tokens.lcl.txt")} bytes into a ` +
        `${h.byteLength("stale-tokens.lcl.txt")}-byte document`,
    );
  }],

  ["a token reply that arrives after a newer one does not replace it", async h => {
    await h.add("inverted-tokens.lcl.txt", "a document long enough to tell apart\n");
    const hold = h.hold("POST");
    const first = h.run("refreshTokens()");
    await bounded(hold.reached, "first token request reached the fixture");
    h.edit("short\n");
    await bounded(h.run("refreshTokens()"), "second token request answered");
    const current = h.tokenReach("inverted-tokens.lcl.txt");
    hold.release();
    await bounded(first, "first token request answered");
    await settle();
    assert.equal(
      h.tokenReach("inverted-tokens.lcl.txt"),
      current,
      "the older reply overwrote the newer one",
    );
  }],

  ["an edit during an outstanding analysis discards the stale report", async h => {
    await h.add("stale-analysis.lcl.txt", "not a document\n");
    // The first analysis describes text with no SPECIFICATION, so it is
    // rejected; the edit makes the document one the engine accepts.
    const hold = h.hold("POST", { path: "/api/inspect" });
    const pending = h.run("runAnalysis()");
    await bounded(hold.reached, "analysis reached the fixture");
    h.edit(VALID_DOCUMENT);
    hold.release();
    await bounded(pending, "analysis answered");
    await settle();
    assert.notEqual(
      h.outcome("stale-analysis.lcl.txt"),
      "rejected",
      "a verdict on text that was replaced was kept as a verdict on the new text",
    );
  }],

  ["an analysis that completes for another tab does not describe this one", async h => {
    await h.add("tab-a.lcl.txt", "not a document\n");
    const hold = h.hold("POST", { path: "/api/inspect" });
    const pending = h.run("runAnalysis()");
    await bounded(hold.reached, "tab A analysis reached the fixture");
    await h.add("tab-b.lcl.txt", VALID_DOCUMENT);
    assert.equal(h.active(), "tab-b.lcl.txt");
    hold.release();
    await bounded(pending, "tab A analysis answered");
    await settle();
    assert.equal(h.active(), "tab-b.lcl.txt", "the finished request switched tabs");
    // Tab A keeping its own verdict is correct per-document caching. What may
    // not happen is that verdict being painted into the panel while tab B is
    // the document on screen.
    assert(
      !h.diagnosticsText().includes("rejected"),
      `tab A's verdict is on screen while tab B is open: ${h.diagnosticsText()}`,
    );
  }],

  ["a document closed while its analysis was outstanding is not resurrected", async h => {
    await h.add("closed.lcl.txt", "not a document\n");
    const hold = h.hold("POST", { path: "/api/inspect" });
    const pending = h.run("runAnalysis()");
    await bounded(hold.reached, "analysis reached the fixture");
    h.close("closed.lcl.txt");
    hold.release();
    await bounded(pending, "analysis answered");
    await settle();
    assert.equal(
      JSON.parse(h.run(`JSON.stringify(state.docs.has("closed.lcl.txt"))`)),
      false,
      "a reply for a closed document put it back",
    );
    assert(
      !h.diagnosticsText().includes("rejected"),
      `a closed document's verdict is still on screen: ${h.diagnosticsText()}`,
    );
  }],

  ["an explicit check that an edit outran is discarded", async h => {
    await h.add("checked.lcl.txt", "not a document\n");
    const hold = h.hold("POST");
    const pending = h.run(`$("#act-check").onclick()`);
    await bounded(hold.reached, "check reached the fixture");
    h.edit(VALID_DOCUMENT);
    hold.release();
    await bounded(pending, "check answered");
    await settle();
    assert.notEqual(
      h.outcome("checked.lcl.txt"),
      "rejected",
      "a check of replaced text was kept as a verdict on the new text",
    );
  }],

  ["the ordinary in-order case still applies its answers", async h => {
    await h.add("in-order.lcl.txt", VALID_DOCUMENT);
    await bounded(h.run("runAnalysis()"), "analysis answered");
    await settle();
    assert.equal(h.outcome("in-order.lcl.txt"), "accepted", "a current answer must be applied");
    const reach = h.tokenReach("in-order.lcl.txt");
    const bytes = h.byteLength("in-order.lcl.txt");
    // Tokens describe this document and no more of it than there is. The
    // engine does not necessarily emit a token for a final line feed, so the
    // upper bound is the document's length rather than exactly it.
    assert(reach > 0 && reach <= bytes, `tokens reach ${reach} of ${bytes} bytes`);
    await bounded(h.run(`$("#act-check").onclick()`), "check answered");
    await settle();
    assert.equal(h.outcome("in-order.lcl.txt"), "accepted");
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
