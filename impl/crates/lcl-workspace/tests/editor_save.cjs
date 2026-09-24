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

/// The element that last took focus in the DOM double, as `document.activeElement`.
let focused = null;

/// Elements by id, as `document.querySelector("#id")` finds them. An element
/// the page creates and gives an id is registered, so a case that reads or
/// sets a control by id reaches the control the page made.
let registry = null;

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
    // Custom properties as the CSSOM exposes them; the production script sets
    // the editor's font size and row height through setProperty.
    const properties = new Map();
    this.style = {
      setProperty: (name, value) => properties.set(name, String(value)),
      getPropertyValue: name => properties.get(name) || "",
      removeProperty: name => properties.delete(name),
    };
    this.dataset = {};
    this.value = "";
    this.textContent = "";
    this.hidden = true;
    this.isConnected = true;
    this.scrollTop = this.scrollLeft = this.selectionStart = this.selectionEnd = 0;
    const classes = new Set();
    this.classList = {
      add: (...items) => items.forEach(item => classes.add(item)),
      remove: (...items) => items.forEach(item => classes.delete(item)),
      toggle: (item, on) => on ? classes.add(item) : classes.delete(item),
      contains: item => classes.has(item),
    };
  }
  get id() { return this._id || ""; }
  set id(value) {
    this._id = value;
    if (registry) registry.set(`#${value}`, this);
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
  focus() { focused = this; }
  remove() {}
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
}

async function harness(options) {
  const origin = options.server ? new URL(options.server).origin : "http://127.0.0.1:12345";
  const token = options.server ? new URL(options.server).searchParams.get("t") : "fixture-token";
  const nodes = new Map();
  registry = nodes;
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
  Object.defineProperty(document, "activeElement", { get: () => focused || document.body });
  const stored = new Map();
  const puts = [];
  // Browser-local storage for workspace preferences. In real-server mode the
  // server never sees it either: these settings are presentation only.
  const local = new Map();
  const localStorage = {
    getItem: key => (local.has(key) ? local.get(key) : null),
    setItem: (key, value) => { local.set(key, String(value)); },
    removeItem: key => { local.delete(key); },
  };
  let nextHold = null;
  let failListing = false;
  let failTokens = false;
  // The computer's settings and the folders that exist, for controlled mode.
  // In real-server mode the server's own settings file and filesystem answer.
  const FIXTURE_SETTINGS = { default_extension: ".lcl", default_workspace: null };
  let fixtureSettings = { ...FIXTURE_SETTINGS };
  const folders = new Set(["/fixture"]);
  const settingsReply = () => jsonReply({
    available: true, file: "/fixture/.config/lcl/workspace-settings.json", problem: null, version: 1,
    ...fixtureSettings,
    default_workspace_exists: fixtureSettings.default_workspace === null ? null : folders.has(fixtureSettings.default_workspace),
    builtin_default_workspace: null, current_workspace: "/fixture",
  });
  const isDocumentName = name => [".lcl.txt", ".lcl"].some(s => name.length > s.length && name.endsWith(s));
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
    } else if (url.pathname === "/api/tokens" && failTokens) {
      failTokens = false;
      reply = jsonReply({ error: "tokens unavailable" }, 503);
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
      reply = jsonReply({ entries: [...stored.keys()].sort().map(id => ({ id, directory: false, bytes: null })) });
    } else if (url.pathname === "/api/document" && method === "POST") {
      // Create: the default ending for a name without one, an explicit one
      // kept, and never over an existing document.
      const name = id.trim();
      const created = name.endsWith(".lcl.txt") || name.endsWith(".lcl")
        ? name : name + fixtureSettings.default_extension;
      if (stored.has(created)) reply = jsonReply({ error: `${created} already exists` }, 409);
      else {
        stored.set(created, init.body);
        reply = jsonReply({ id: created, requested: id, digest: hash(init.body), bytes: Buffer.byteLength(init.body) });
      }
    } else if (url.pathname === "/api/document" && method === "DELETE") {
      const digest = url.searchParams.get("digest");
      if (!isDocumentName(id.split("/").pop())) reply = jsonReply({ error: `${id} is not an LCL document` }, 400);
      else if (!stored.has(id)) reply = jsonReply({ error: `${id} does not exist` }, 404);
      else if (hash(stored.get(id)) !== digest) {
        reply = jsonReply({ error: `${id} changed on disk after it was shown, so it was left alone` }, 409);
      } else {
        stored.delete(id);
        reply = jsonReply({ id, deleted: true });
      }
    } else if (url.pathname === "/api/settings" && method === "GET") {
      reply = settingsReply();
    } else if (url.pathname === "/api/settings" && method === "PUT") {
      const body = JSON.parse(init.body);
      const where = body.default_workspace || null;
      if (![".lcl", ".lcl.txt"].includes(body.default_extension)) {
        reply = jsonReply({ error: "the default file type must be .lcl or .lcl.txt" }, 422);
      } else if (where !== null && (!where.startsWith("/") || !folders.has(where))) {
        reply = jsonReply({ error: `${where} does not exist` }, 422);
      } else {
        fixtureSettings = { default_extension: body.default_extension, default_workspace: where };
        reply = settingsReply();
      }
    } else if (url.pathname === "/api/folder") {
      const where = url.searchParams.get("path");
      const absolute = where.startsWith("/");
      if (method === "POST" && !absolute) reply = jsonReply({ error: `${where} is not an absolute path` }, 400);
      else {
        const created = method === "POST" && !folders.has(where);
        if (method === "POST") folders.add(where);
        reply = jsonReply({ path: where, absolute, exists: folders.has(where), directory: folders.has(where), created });
      }
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
    } else if (url.pathname === "/api/document" && method === "GET") {
      reply = jsonReply({ error: `${id}: the document is not readable` }, 404);
    } else throw new Error(`unexpected fixture request ${method} ${url}`);
    if (hold) {
      hold.reached.resolve();
      await bounded(hold.release.promise, "held save response");
    }
    return reply;
  };
  const context = vm.createContext({
    document, location: { origin, search: `?t=${token}` }, fetch: request, localStorage,
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
    sourceHash: hash(source), puts, get, run, local,
    /// Whether files live on a real disk the case can inspect directly.
    realDisk: Boolean(options.project),
    /// The page as served: the real server's in real-server mode, the file
    /// the server embeds otherwise.
    async page() {
      return options.server
        ? await (await fetch(new URL(`/?t=${token}`, origin), { signal: AbortSignal.timeout(4000) })).text()
        : await fs.readFile(path.join(path.dirname(options.app), "index.html"), "utf8");
    },
    /// Everything painted on the visible layer, as text.
    painted() {
      const collect = node => (node.children.length
        ? node.children.map(collect).join("")
        : node.textContent || "");
      return collect(get("#paint"));
    },
    /// The line numbers shown. A fragment's children are inserted in its
    /// place, as the DOM does; this double keeps the fragment, so flatten it.
    gutterLines() {
      const flat = nodes => nodes.flatMap(node =>
        node.tagName === "fragment" ? flat(node.children) : [node]);
      return flat(get("#gutter").children).map(node => node.textContent);
    },
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
    failNextTokens() { failTokens = true; },
    /// The stylesheet as served, like `page()`.
    async stylesheet() {
      return options.server
        ? await (await fetch(new URL(`/app.css?t=${token}`, origin), { signal: AbortSignal.timeout(4000) })).text()
        : await fs.readFile(path.join(path.dirname(options.app), "app.css"), "utf8");
    },
    /// The painted layer as runs of text, each with the classes drawn on it.
    paintedRuns() {
      const runs = [];
      const walk = node => {
        if (node.tagName === "fragment") node.children.forEach(walk);
        else runs.push({ cls: node.className || "", text: node.textContent || "" });
      };
      get("#paint").children.forEach(walk);
      return runs;
    },
    /// Line numbers in the gutter that carry a diagnostic mark, as "line:kind".
    gutterMarks() {
      const flat = nodes => nodes.flatMap(node =>
        node.tagName === "fragment" ? flat(node.children) : [node]);
      return flat(get("#gutter").children).flatMap(node =>
        ["bad", "warn"].filter(kind => node.classList.contains(`has-${kind}`))
          .map(kind => `${node.textContent}:${kind}`));
    },
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
    /// Whether a document exists on disk (or in the fixture's store).
    async exists(id) {
      if (!options.project) return stored.has(id);
      return fs.access(path.join(options.project, id)).then(() => true, () => false);
    },
    /// Another writer changes a document behind the page's back.
    async writeBehind(id, text) {
      if (options.project) await fs.writeFile(path.join(options.project, id), text);
      else stored.set(id, text);
    },
    /// Another program removes a document behind the page's back.
    async removeBehind(id) {
      if (options.project) await fs.unlink(path.join(options.project, id));
      else stored.delete(id);
    },
    /// An absolute folder path for the default-workspace cases: inside the
    /// project's own dot directory in real-server mode, which the tree skips.
    folder(name) {
      return options.project ? path.join(options.project, ".folders", name) : `/fixture/${name}`;
    },
    /// Create a document the way a person does: + and the New document dialog.
    async create(name) {
      run("newDocument()");
      get("#new-path").value = name;
      await bounded(this.choose("Create"), `create ${name}`);
      return run("state.active");
    },
    /// The documents the project tree lists.
    treeIds() {
      return JSON.parse(run("JSON.stringify(state.entries.filter(e => !e.directory).map(e => e.id))"));
    },
    modalOpen() { return get("#modal-backdrop").hidden === false; },
    modalTitle() { return get("#modal-title").textContent; },
    modalText() {
      const collect = node => [node.textContent || ""].concat(node.children.flatMap(collect)).join(" ");
      return collect(get("#modal-body")).replace(/\s+/g, " ").trim();
    },
    lifecycle(id) { return run(`(state.docs.get(${JSON.stringify(id)}) || {}).lifecycle || null`); },
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
    /// Mark the report object a document currently holds.
    ///
    /// Every reply is a fresh object, so a mark that is still there afterwards
    /// says the stored report was not replaced — without needing the two
    /// replies to differ in content, which two analyses of one root revision
    /// need not do at all.
    markReport(id, mark) {
      run(`state.docs.get(${JSON.stringify(id)}).report.__mark = ${JSON.stringify(mark)}`);
    },
    reportMark(id) {
      return JSON.parse(run(`JSON.stringify((() => {
        const d = state.docs.get(${JSON.stringify(id)});
        return d && d.report ? d.report.__mark || null : null;
      })())`));
    },
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
    async reset() {
      assert.equal(nextHold, null, "a test leaked a pending held response");
      run("state.docs.clear(); state.order=[]; state.active=null; code.value=''; closeModal();");
      local.clear();
      // Guarded so the suite can also be pointed at an older page as a control.
      run('if (typeof applySettings === "function") applySettings({ ...DEFAULT_SETTINGS })');
      // The computer's settings go back to their defaults too, wherever they live.
      fixtureSettings = { ...FIXTURE_SETTINGS };
      if (options.server) {
        const reply = await request(new URL("/api/settings", origin), {
          method: "PUT", body: JSON.stringify({ default_extension: ".lcl", default_workspace: null }),
        });
        assert(reply.ok, `settings could not be reset: ${await reply.text()}`);
      }
      await run('typeof loadFileSettings === "function" ? loadFileSettings() : null');
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


/// Document-dependent controls, as `syncDocumentUI` leaves them.
function uiState(h) {
  const actions = ["#act-check", "#act-inspect", "#act-run", "#act-save", "#act-reload"];
  return {
    editable: h.get("#code").disabled === false,
    empty: h.get("#empty-state").hidden === false,
    disabled: actions.filter(a => h.get(a).disabled === true),
  };
}
const ALL_ACTIONS = ["#act-check", "#act-inspect", "#act-run", "#act-save", "#act-reload"];

const uiCases = [
  // -------------------------------------------------------------------------
  // UI-02 — the empty workspace, the visible editor and Settings
  // -------------------------------------------------------------------------
  //
  // The observed defect: with no document open, the source textarea (which is
  // transparent, so that the engine-painted layer underneath is what is seen)
  // stayed editable, and typing produced text nobody could see. This case is
  // first so that it sees the page exactly as it booted over an empty project.
  ["an empty workspace boots with no editable editor and no document actions", async h => {
    assert.equal(h.active(), null, "an empty project opened a document");
    assert.deepEqual(uiState(h), { editable: false, empty: true, disabled: ALL_ACTIONS });
    assert.notEqual(h.get("#act-new").disabled, true, "+ New document must stay available");
    // Anything that reaches the textarea anyway is discarded, not kept unseen.
    h.edit("typed into nothing");
    assert.equal(h.get("#code").value, "", "invisible text accumulated with no document");
    assert.equal(h.run("state.docs.size"), 0);
  }],

  ["the page ships with the editor and document actions disabled before any script runs", async h => {
    const page = await h.page();
    assert.match(page, /<textarea id="code"[^>]*\sdisabled>/, "the textarea is not disabled in the markup");
    for (const id of ["act-check", "act-inspect", "act-run", "act-save", "act-reload"]) {
      assert.match(page, new RegExp(`<button id="${id}"[^>]*\\sdisabled>`), `${id} is not disabled in the markup`);
    }
    assert.match(page, /<button id="act-new"[^>]*>\+<\/button>/, "+ must be present and enabled");
    assert.match(page, /<button id="act-settings"[^>]*title="Settings"/, "the Settings button is missing");
    assert.match(page, /No document open/, "the empty-state message is missing");
  }],

  ["opening a document makes the editor editable and its text visible with line numbers", async h => {
    const text = 'LCL:\n    VERSION: "0.1.0"\n';
    await h.add("visible.lcl", text);
    assert.deepEqual(uiState(h), { editable: true, empty: false, disabled: [] });
    assert.equal(h.painted(), text, "the painted layer does not show the document");
    assert.deepEqual(h.gutterLines(), ["1", "2", "3"]);

    // Typing repaints at once, before any analysis answers.
    h.edit(text + "A\nB\n");
    assert.equal(h.painted(), text + "A\nB\n", "typed text is not visible");
    assert.deepEqual(h.gutterLines(), ["1", "2", "3", "4", "5"], "new lines did not get numbers");

    h.edit("LCL:\n");
    assert.deepEqual(h.gutterLines(), ["1", "2"], "removed lines kept their numbers");

    // An empty open document still has line 1.
    h.edit("");
    assert.deepEqual(h.gutterLines(), ["1"]);
    assert.equal(h.painted(), "");
  }],

  ["without tokens the document is painted as plain text, never left invisible", async h => {
    const text = 'LCL:\n    VERSION: "0.1.0"\n';
    await h.add("plain.lcl.txt", text);
    h.run("current().tokens = null; render()");
    assert.equal(h.painted(), text, "no tokens left the text unpainted");
    // Tokens from an older revision cover fewer bytes than the new text: every
    // byte of the new text must still be painted.
    h.edit(text + "SPECIFICATION:\n");
    assert.equal(h.painted(), text + "SPECIFICATION:\n");
  }],

  ["closing the final tab returns to the empty state", async h => {
    await h.add("last.lcl", "LCL:\n");
    await bounded(h.run("runAnalysis()"), "analysis of the last document");
    assert.notEqual(h.diagnosticsText(), "Nothing checked yet.", "the case needs a report on screen first");
    assert.equal(uiState(h).editable, true);
    h.close("last.lcl");
    assert.equal(h.active(), null);
    assert.deepEqual(uiState(h), { editable: false, empty: true, disabled: ALL_ACTIONS });
    assert.equal(h.get("#code").value, "", "the closed document's text stayed in the textarea");
    assert.deepEqual(h.gutterLines(), [], "line numbers stayed after the last tab closed");
    assert.equal(h.painted(), "");
    assert.equal(h.diagnosticsText(), "Nothing checked yet.",
      "the diagnostics of the closed document are still shown");
  }],

  ["Settings offers the three themes and saves font size and line numbers", async h => {
    h.run("openSettings()");
    const find = (node, tag) => node.tagName === tag ? node
      : node.children.map(child => find(child, tag)).find(Boolean) || null;
    const select = find(h.get("#modal-body"), "select");
    assert(select, "the Settings modal has no theme choice");
    assert.deepEqual(select.children.map(o => o.value), ["system", "dark", "light"]);
    assert.equal(select.value, "system", "System is the default theme");

    h.get("#setting-theme").value = "light";
    h.get("#setting-font-size").value = "17";
    h.get("#setting-line-numbers").checked = false;
    h.choose("Save");
    assert.equal(h.run("document.documentElement.dataset.theme"), "light");
    assert.equal(h.run('document.documentElement.style.getPropertyValue("--editor-font")'), "17px");
    assert.equal(h.run('document.documentElement.style.getPropertyValue("--row")'), "26px");
    assert.equal(h.run('document.documentElement.classList.contains("no-gutter")'), true);
    const stored = JSON.parse(h.local.get("lcl.workspace.settings"));
    assert.deepEqual(stored, { version: 1, theme: "light", fontSize: 17, lineNumbers: false });

    // Line numbers back on, System theme: the override is removed again.
    h.run("openSettings()");
    h.get("#setting-theme").value = "system";
    h.get("#setting-font-size").value = "17";
    h.get("#setting-line-numbers").checked = true;
    h.choose("Save");
    assert.equal(h.run("document.documentElement.dataset.theme"), undefined);
    assert.equal(h.run('document.documentElement.classList.contains("no-gutter")'), false);
  }],

  ["the font size is bounded to 11 to 20 px", async h => {
    // A browser's number field reports text it cannot read as "", so "" is the
    // unreadable case in practice: it falls back like any other, not to 11.
    for (const [typed, applied] of [["99", 20], ["3", 11], ["14.6", 15], ["not a number", 13], ["", 13], ["  ", 13]]) {
      h.run("openSettings()");
      h.get("#setting-theme").value = "system";
      h.get("#setting-font-size").value = typed;
      h.get("#setting-line-numbers").checked = true;
      h.choose("Save");
      assert.equal(h.run("state.settings.fontSize"), applied, `typed ${JSON.stringify(typed)}`);
    }
  }],

  ["stored preferences are restored, and invalid ones fall back to defaults", async h => {
    const restore = raw => {
      if (raw === null) h.local.delete("lcl.workspace.settings");
      else h.local.set("lcl.workspace.settings", raw);
      h.run("applySettings(loadSettings())");
      return JSON.parse(h.run("JSON.stringify(state.settings)"));
    };
    const defaults = { version: 1, theme: "system", fontSize: 13, lineNumbers: true };

    assert.deepEqual(restore(JSON.stringify({ version: 1, theme: "dark", fontSize: 15, lineNumbers: false })),
      { version: 1, theme: "dark", fontSize: 15, lineNumbers: false });
    assert.equal(h.run("document.documentElement.dataset.theme"), "dark");
    assert.equal(h.run('document.documentElement.style.getPropertyValue("--editor-font")'), "15px");
    assert.equal(h.run('document.documentElement.classList.contains("no-gutter")'), true);

    assert.deepEqual(restore(null), defaults, "nothing stored");
    assert.deepEqual(restore("{not json"), defaults, "corrupt JSON");
    assert.deepEqual(restore(JSON.stringify({ version: 99, theme: "dark", fontSize: 15 })), defaults,
      "another version");
    // Field by field: a bad field falls back alone, good ones are kept.
    assert.deepEqual(restore(JSON.stringify({ version: 1, theme: "neon", fontSize: 40, lineNumbers: "yes" })),
      defaults);
    assert.deepEqual(restore(JSON.stringify({ version: 1, theme: "light", fontSize: 12.5, lineNumbers: true })),
      { ...defaults, theme: "light" });
    assert.equal(h.run('document.documentElement.classList.contains("no-gutter")'), false);
  }],

  ["preferences never reach the engine or the project", async h => {
    h.run("openSettings()");
    h.get("#setting-theme").value = "dark";
    h.get("#setting-font-size").value = "18";
    h.get("#setting-line-numbers").checked = false;
    h.choose("Save");
    await h.add("untouched.lcl", "LCL:\n");
    assert.equal(await h.persisted("untouched.lcl"), "LCL:\n", "a preference reached the document");
    assert(!h.puts.some(p => (p.body || "").includes("fontSize")), "a preference was sent to the server");
  }],

  // -------------------------------------------------------------------------
  // UI-03 — what a real browser showed that the cases above did not
  // -------------------------------------------------------------------------
  //
  // Found by driving the workspace in headless Firefox. Scrolled to the end of
  // a long document, the painted text sat one line below the text being typed:
  // a <pre> has no row after a final line feed and a textarea does. And while
  // an edit waited for the engine, the previous answer's spans were drawn at
  // their old byte offsets, so every colour after the edit slid onto other
  // characters. The rest are smaller: a cursor position shown with no document,
  // a failed token request that kept old colours, a dialog that dropped focus,
  // and a project tree that missed the first unsaved edit.
  ["the painted layer has the row a textarea gives a final line feed", async h => {
    // The rendering itself is checked in a browser; this keeps the rules that
    // make it right from being dropped.
    const css = await h.stylesheet();
    assert.match(css, /\.paint::after\s*\{\s*content:\s*"\\A";\s*\}/, "the row after a final line feed is gone");
    assert.match(css, /\.paint\s*\{[^}]*scrollbar-width:\s*none/, "the paint layer shows a scrollbar of its own");
  }],

  ["an edit moves the engine's colours with the text instead of leaving them behind", async h => {
    const text = 'LCL:\n    VERSION: "0.1.0"\n';
    await h.add("carry.lcl", text);
    // Spans as the engine gives them: LCL, its colon, VERSION.
    h.run(`current().tokens = [
      { class: "keyword", start: 0, end: 3 }, { class: "symbol", start: 3, end: 4 },
      { class: "keyword", start: 9, end: 16 }]; render()`);
    const drawn = () => h.paintedRuns().filter(r => r.cls).map(r => `${r.cls}=${r.text}`);
    assert.deepEqual(drawn(), ["t-keyword=LCL", "t-symbol=:", "t-keyword=VERSION"]);

    // Typed ahead of every span: they all move, and what was typed is plain.
    h.edit("XX" + text);
    assert.deepEqual(drawn(), ["t-keyword=LCL", "t-symbol=:", "t-keyword=VERSION"],
      "colours stayed at their old byte offsets and landed on other characters");
    assert.equal(h.paintedRuns()[0].cls, "", "typed text was coloured before the engine saw it");
    assert.equal(h.painted(), "XX" + text);

    // Typed inside a span: that one is dropped until the engine answers.
    h.edit("XX" + text.replace("VERSION", "VERXSION"));
    assert.deepEqual(drawn(), ["t-keyword=LCL", "t-symbol=:"]);
    assert.equal(h.painted(), "XX" + text.replace("VERSION", "VERXSION"));

    // Deleted from the start: the rest moves back.
    h.edit(text.replace("VERSION", "VERXSION"));
    assert.deepEqual(drawn(), ["t-keyword=LCL", "t-symbol=:"]);
  }],

  ["an edit moves squiggles and the gutter's diagnostic lines with the text", async h => {
    const text = 'LCL:\n    VERSION: "0.1.0"\n';
    await h.add("marks.lcl", text);
    h.run(`(() => {
      const doc = current();
      doc.tokens = [];
      doc.report = { outcome: "rejected", reached: "grammar_or_schema", diagnostics: [{
        id: "error.fixture", source: doc.id, span: { start: 9, end: 16 }, position: { line: 2, column: 5 },
        stage: "grammar_or_schema", default_status: "status.invalid", meaning: "fixture", detail: null,
        primary: true,
      }] };
      // Guarded so that an older page, which drew the report directly, can
      // still be run as a control and fail on what it draws.
      if (typeof markReport === "function") markReport(doc);
      render();
    })()`);
    const squiggled = () => h.paintedRuns().filter(r => r.cls.includes("sq-bad")).map(r => r.text);
    assert.deepEqual(squiggled(), ["VERSION"]);
    assert.deepEqual(h.gutterMarks(), ["2:bad"]);

    // Two lines added above it: the squiggle stays on VERSION, the mark on its line.
    h.edit("\n\n" + text);
    assert.deepEqual(squiggled(), ["VERSION"], "the squiggle stayed at its old byte offsets");
    assert.deepEqual(h.gutterMarks(), ["4:bad"], "the diagnostic line did not move with its text");

    // An edit inside it drops it until the engine looks again.
    h.edit("\n\n" + text.replace("VERSION", "VER SION"));
    assert.deepEqual(squiggled(), []);
    assert.deepEqual(h.gutterMarks(), []);
    // The report is the engine's record, and moving marks does not rewrite it.
    assert.equal(h.run("current().report.diagnostics[0].span.start"), 9);
  }],

  ["a failed token request paints the text plain instead of keeping old colours", async h => {
    const text = 'LCL:\n    VERSION: "0.1.0"\n';
    await h.add("failed-tokens.lcl", text);
    h.run(`current().tokens = [{ class: "keyword", start: 0, end: 3 }]; render()`);
    assert(h.paintedRuns().some(r => r.cls === "t-keyword"), "the case needs colours on screen first");
    h.failNextTokens();
    await bounded(h.run("refreshTokens()"), "failed token request");
    assert.equal(h.run("current().tokens"), null);
    assert.deepEqual(h.paintedRuns(), [{ cls: "", text }], "colours from an earlier answer are still drawn");
  }],

  ["the status bar shows a cursor position only while a document is open", async h => {
    assert.match(await h.page(), /<span id="cursor"><\/span>/, "the page ships a cursor position with no document");
    h.run("render()");
    assert.equal(h.get("#cursor").textContent, "");
    await h.add("cursor.lcl", "LCL:\n");
    assert.match(h.get("#cursor").textContent, /^1:1 /);
    h.close("cursor.lcl");
    assert.equal(h.get("#cursor").textContent, "", "the closed document's position is still shown");
  }],

  ["a dialog takes focus, holds the page behind it inert and gives focus back", async h => {
    await h.add("focus.lcl", "LCL:\n");
    const opener = () => h.run('document.activeElement === document.querySelector("#act-settings")');
    h.get("#act-settings").focus();
    h.run("openSettings()");
    assert.equal(h.get("#shell").inert, true, "the page behind the dialog still takes focus and clicks");
    assert.equal(h.run("document.activeElement.id"), "setting-theme", "Settings did not take focus");
    h.choose("Cancel");
    assert.equal(h.get("#shell").inert, false, "the page stayed inert after the dialog closed");
    assert.equal(opener(), true, "focus did not return to the Settings button");

    // Escape closes too, and a dialog opened over another keeps the first opener.
    h.get("#act-settings").focus();
    h.run("openSettings()");
    h.run("newDocument()");
    h.press("Escape");
    assert.equal(h.get("#modal-backdrop").hidden, true);
    assert.equal(h.get("#shell").inert, false);
    assert.equal(opener(), true);
  }],

  // -------------------------------------------------------------------------
  // UI-04 — a document's life: created, saved, discarded, deleted
  // -------------------------------------------------------------------------
  //
  // New document wrote its file at once, and Discard only closed the buffer,
  // so a document nobody chose to keep stayed in the project. Nothing could be
  // deleted at all. A document now records how it came to be open, which is
  // not the same question as whether it has unsaved edits, and only a new one
  // that nobody saved has its file removed when it is discarded.
  ["a new .lcl document that is discarded leaves no file, tab or tree entry", async h => {
    const id = await h.create("fresh-discard");
    assert.equal(id, "fresh-discard.lcl");
    assert.equal(h.lifecycle(id), "created");
    assert.equal(await h.exists(id), true, "creating writes the file");
    assert(h.treeIds().includes(id));
    // Not edited at all: closing is still the moment to decide.
    h.close(id);
    assert.equal(h.modalTitle(), "Unsaved new document");
    assert(h.modalText().includes(`${id} has not been saved`), h.modalText());
    await bounded(h.choose("Discard"), "discard");
    assert.equal(await h.exists(id), false, "the created file survived Discard");
    assert.equal(h.doc(id), null, "its tab stayed open");
    assert(!h.treeIds().includes(id), "the tree still lists it");
  }],

  ["a new .lcl.txt document that is discarded after edits leaves nothing either", async h => {
    const id = await h.create("fresh-discard.lcl.txt");
    assert.equal(id, "fresh-discard.lcl.txt", "an explicit ending was not kept");
    h.edit(`${h.doc(id).text}\nedited but never saved\n`);
    h.close(id);
    assert.equal(h.modalTitle(), "Unsaved new document");
    await bounded(h.choose("Discard"), "discard");
    assert.equal(await h.exists(id), false);
    assert.equal(h.doc(id), null);
    assert(!h.treeIds().includes(id));
  }],

  ["a new document saved from its close dialog is kept as an ordinary one", async h => {
    const id = await h.create("saved-on-close");
    h.close(id);
    assert.equal(h.modalTitle(), "Unsaved new document");
    await bounded(h.choose("Save"), "saved on close");
    assert.equal(h.doc(id), null, "the tab did not close after saving");
    assert.equal(await h.exists(id), true, "Save did not keep the file");
  }],

  ["a new document that was saved is kept, and a later discard drops only later edits", async h => {
    const id = await h.create("kept-new.lcl");
    h.edit('LCL:\n    VERSION: "0.1.0"\n');
    assert.equal(await bounded(h.run("save()"), "explicit save"), true);
    assert.equal(h.lifecycle(id), "saved");
    const saved = await h.persisted(id);
    h.edit(`${saved}unsaved later edit\n`);
    h.close(id);
    assert.equal(h.modalTitle(), "Unsaved changes", "a saved document was still treated as new");
    h.choose("Discard");
    assert.equal(h.doc(id), null);
    assert.equal(await h.exists(id), true, "a saved document was deleted by Discard");
    assert.equal(await h.persisted(id), saved, "the saved content changed");
  }],

  ["an existing document's discarded edits leave its file exactly as it was", async h => {
    await h.add("existing-kept.lcl", "original text\n");
    assert.equal(h.lifecycle("existing-kept.lcl"), "opened");
    h.edit("edited text\n");
    h.close("existing-kept.lcl");
    assert.equal(h.modalTitle(), "Unsaved changes");
    h.choose("Discard");
    assert.equal(h.doc("existing-kept.lcl"), null);
    assert.equal(await h.persisted("existing-kept.lcl"), "original text\n");
    // Opened and closed untouched: nothing asked, nothing removed.
    await h.add("existing-clean.lcl.txt", "clean\n");
    h.close("existing-clean.lcl.txt");
    assert.equal(h.modalOpen(), false);
    assert.equal(await h.persisted("existing-clean.lcl.txt"), "clean\n");
  }],

  ["a new document that something else rewrote is kept when it is discarded", async h => {
    const id = await h.create("changed-behind.lcl");
    await h.writeBehind(id, "written by something else\n");
    h.close(id);
    await bounded(h.choose("Discard"), "discard");
    assert.equal(h.doc(id), null, "the tab closes either way");
    assert.equal(await h.exists(id), true, "a file changed after creation was deleted");
    assert.equal(await h.persisted(id), "written by something else\n");
    assert(h.toasts().some(t => t.includes("was kept")), h.toasts().join(" | "));
  }],

  ["deleting asks first, and Cancel changes nothing", async h => {
    await h.add("delete-cancel.lcl", "keep me\n");
    await bounded(h.run('deleteDocument("delete-cancel.lcl")'), "delete asked");
    assert.equal(h.modalTitle(), 'Delete "delete-cancel.lcl"?');
    assert(h.modalText().includes("This permanently removes the file from the project."));
    assert(!h.modalText().includes("unsaved"), "a clean document was said to have unsaved edits");
    h.choose("Cancel");
    assert.equal(await h.persisted("delete-cancel.lcl"), "keep me\n");
    assert.notEqual(h.doc("delete-cancel.lcl"), null);
  }],

  ["deleting an inactive .lcl document leaves the active one open", async h => {
    await h.add("del-inactive.lcl", "a\n");
    await h.add("del-active.lcl.txt", "b\n");
    assert.equal(h.active(), "del-active.lcl.txt");
    await bounded(h.run('deleteDocument("del-inactive.lcl")'), "asked");
    await bounded(h.choose("Delete"), "deleted");
    assert.equal(await h.exists("del-inactive.lcl"), false);
    assert.equal(h.doc("del-inactive.lcl"), null, "its tab stayed");
    assert.equal(h.active(), "del-active.lcl.txt");
    assert(!h.treeIds().includes("del-inactive.lcl"), "the tree still lists it");
    assert(h.treeIds().includes("del-active.lcl.txt"));
  }],

  ["deleting the active .lcl.txt document moves to another tab, then to the empty state", async h => {
    await h.add("del-first.lcl", "first\n");
    await h.add("del-second.lcl.txt", "second\n");
    await bounded(h.run('deleteDocument("del-second.lcl.txt")'), "asked");
    await bounded(h.choose("Delete"), "deleted");
    assert.equal(await h.exists("del-second.lcl.txt"), false);
    assert.equal(h.active(), "del-first.lcl");
    assert.equal(h.get("#code").value, "first\n");
    await bounded(h.run('deleteDocument("del-first.lcl")'), "asked");
    await bounded(h.choose("Delete"), "deleted");
    assert.equal(h.active(), null);
    assert.deepEqual(uiState(h), { editable: false, empty: true, disabled: ALL_ACTIONS });
  }],

  ["deleting a document with unsaved edits says they are lost too", async h => {
    await h.add("del-dirty.lcl", "saved\n");
    h.edit("unsaved\n");
    await bounded(h.run('deleteDocument("del-dirty.lcl")'), "asked");
    assert(h.modalText().includes("unsaved edits"), h.modalText());
    await bounded(h.choose("Delete"), "deleted");
    assert.equal(await h.exists("del-dirty.lcl"), false);
    assert.equal(h.doc("del-dirty.lcl"), null);
  }],

  ["a file that changed while deletion was being confirmed is not deleted", async h => {
    await h.add("del-changed.lcl", "as shown\n");
    await bounded(h.run('deleteDocument("del-changed.lcl")'), "asked");
    await h.writeBehind("del-changed.lcl", "changed meanwhile\n");
    await bounded(h.choose("Delete"), "refused");
    assert.equal(await h.persisted("del-changed.lcl"), "changed meanwhile\n");
    assert.notEqual(h.doc("del-changed.lcl"), null, "the open tab was closed anyway");
    assert(h.toasts().some(t => t.includes("was not deleted")), h.toasts().join(" | "));
  }],

  ["a document that is already gone is reported, and the tree catches up", async h => {
    await h.add("del-gone.lcl", "x\n");
    await h.run("loadTree()");
    assert(h.treeIds().includes("del-gone.lcl"));
    await h.removeBehind("del-gone.lcl");
    await bounded(h.run('deleteDocument("del-gone.lcl")'), "asked");
    assert.equal(h.modalOpen(), false, "a question was asked about a file that is not there");
    assert(h.toasts().some(t => t.includes("could not be read")), h.toasts().join(" | "));
    assert(!h.treeIds().includes("del-gone.lcl"), "the tree still lists it");
  }],

  ["the default file type names a new document, and an explicit ending always wins", async h => {
    h.run("newDocument()");
    assert.equal(h.get("#new-path").value, "untitled.lcl");
    h.choose("Cancel");
    assert.equal(await h.create("typed-plain"), "typed-plain.lcl");

    h.run("openSettings()");
    assert.equal(h.get("#setting-file-type").value, ".lcl", ".lcl is the default");
    h.get("#setting-file-type").value = ".lcl.txt";
    await bounded(h.choose("Save"), "settings saved");
    assert.equal(h.modalOpen(), false, "Settings did not save");
    assert.equal(h.run("state.files.default_extension"), ".lcl.txt");

    h.run("newDocument()");
    assert.equal(h.get("#new-path").value, "untitled.lcl.txt");
    assert(h.modalText().includes("created as .lcl.txt"), h.modalText());
    h.choose("Cancel");
    assert.equal(await h.create("typed-text"), "typed-text.lcl.txt");
    assert.equal(await h.create("explicit-classic.lcl"), "explicit-classic.lcl");
    assert.equal(await h.create("explicit-text.lcl.txt"), "explicit-text.lcl.txt");
    assert.equal(await h.exists("typed-text.lcl"), false, "a stacked or converted twin exists");
    assert.equal(await h.exists("explicit-classic.lcl.txt"), false, "an explicit ending was converted");
    // Kept by the server, not only by the page.
    await h.run("loadFileSettings()");
    assert.equal(h.run("state.files.default_extension"), ".lcl.txt");
  }],

  ["Settings saves a default workspace that exists and creates a missing one only when asked", async h => {
    const statusText = () => {
      const collect = node => [node.textContent || ""].concat(node.children.flatMap(collect)).join(" ");
      return collect(h.get("#setting-workspace-status"));
    };
    const missing = h.folder("new default");
    h.run("openSettings()");
    h.get("#setting-workspace").value = "relative/folder";
    await bounded(h.choose("Save"), "relative refused");
    assert.equal(h.modalOpen(), true, "a relative path was accepted");
    assert.match(statusText(), /not an absolute path/);

    h.get("#setting-workspace").value = missing;
    await bounded(h.choose("Save"), "missing refused");
    assert.equal(h.modalOpen(), true, "a folder that does not exist was saved");
    assert.match(statusText(), /does not exist/);
    if (h.realDisk) assert.equal(await fs.access(missing).then(() => true, () => false), false,
      "the folder was created before anyone asked");

    const create = h.get("#setting-workspace-status").children.find(c => c.textContent === "Create this folder");
    assert(create, "no way to create the missing folder was offered");
    await bounded(create.onclick(), "folder created");
    assert.match(statusText(), /Created/);
    await bounded(h.choose("Save"), "saved");
    assert.equal(h.modalOpen(), false);
    assert.equal(h.run("state.files.default_workspace"), missing);
    assert(h.toasts().some(t => t.includes("Default workspace updated")), h.toasts().join(" | "));
    assert(h.toasts().some(t => t.includes("next time LCL Workspace is launched")));

    await h.run("loadFileSettings()");
    assert.equal(h.run("state.files.default_workspace"), missing, "the server did not keep it");
    // Emptied again: launches go back to the built-in folder.
    h.run("openSettings()");
    assert.equal(h.get("#setting-workspace").value, missing);
    h.get("#setting-workspace").value = "";
    await bounded(h.choose("Save"), "cleared");
    await h.run("loadFileSettings()");
    assert.equal(h.run("state.files.default_workspace"), null);
  }],

  ["the project tree marks a document unsaved from the first keystroke", async h => {
    await h.add("tree-dot.lcl", "LCL:\n");
    h.run('state.entries = [{ id: "tree-dot.lcl", directory: false }]; renderTree()');
    const dotted = () => h.get("#tree").children.some(li => li.children.some(c => c.className === "dot"));
    assert.equal(dotted(), false);
    h.edit("LCL:\nX\n");
    assert.equal(dotted(), true, "the tree did not show the unsaved edit");
    h.edit("LCL:\n");
    assert.equal(dotted(), false, "the tree kept its mark after the edit was undone");
  }],
];

const cases = [
  ...uiCases,
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

  // -------------------------------------------------------------------------
  // AB-05 — two analyses of one root revision still have an order
  // -------------------------------------------------------------------------
  //
  // The revision guard asks whether the root text changed. An analysis does
  // not read only the root: `inspect` resolves the document's imports from
  // disk, so two analyses issued at one root revision can describe different
  // inputs, and the guard admits both. Whichever arrives last then wins,
  // which on an inverted pair is the one describing the older state.
  //
  // What settles it is which request the answer belongs to. Each is issued
  // with a generation, and an answer older than one already accepted is not
  // applied — whatever the root revision says about it.
  ["an analysis answer older than one already accepted is discarded", async h => {
    await h.add("generation.lcl.txt", VALID_DOCUMENT);
    const hold = h.hold("POST", { path: "/api/inspect" });
    const first = h.run("runAnalysis()");
    await bounded(hold.reached, "the first analysis reached the fixture");
    // A second analysis of the same root revision — no edit between them.
    await bounded(h.run("runAnalysis()"), "the second analysis answered");
    h.markReport("generation.lcl.txt", "second");
    hold.release();
    await bounded(first, "the first analysis answered");
    await settle();
    assert.equal(
      h.reportMark("generation.lcl.txt"),
      "second",
      "the earlier analysis replaced the later one's report",
    );
  }],

  ["an explicit check older than an accepted analysis is discarded", async h => {
    // They share one `report`, so ordering has to hold across both.
    await h.add("shared.lcl.txt", VALID_DOCUMENT);
    const hold = h.hold("POST", { path: "/api/check" });
    const check = h.run(`$("#act-check").onclick()`);
    await bounded(hold.reached, "the check reached the fixture");
    await bounded(h.run("runAnalysis()"), "a later analysis answered");
    h.markReport("shared.lcl.txt", "analysis");
    hold.release();
    await bounded(check, "the check answered");
    await settle();
    assert.equal(
      h.reportMark("shared.lcl.txt"),
      "analysis",
      "an older check replaced a newer analysis in the shared report",
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
    await h.reset();
    try { await test(h); console.log(`PASS ${name}`); }
    catch (error) { failures++; console.error(`FAIL ${name}\n${error.stack}`); }
  }
  console.log(`${cases.length-failures} passed; ${failures} failed; 0 skipped`);
  process.exitCode=failures ? 1 : 0;
}
main().catch(error => { console.error(error.stack); process.exitCode=1; });
