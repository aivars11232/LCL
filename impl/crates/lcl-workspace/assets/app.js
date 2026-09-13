"use strict";

/* LCL Workspace frontend.
 *
 * The one rule this file obeys, everywhere: it does not know the LCL language.
 * It does not tokenize, parse, resolve, check or evaluate. It has no keyword
 * list, no grammar and no regular expression that matches LCL syntax. Every
 * span it paints, every diagnostic it shows, every reference it follows and
 * every status it reports arrived from the engine over HTTP.
 *
 * A JavaScript regular-expression highlighter would have been less code and a
 * second lexer, and the first time it disagreed with `lcl-lexer` the user would
 * be looking at a lie. So the browser asks for token spans instead.
 *
 * Byte offsets are normative. The engine speaks in bytes; JavaScript strings
 * are UTF-16. Every conversion goes through one index built per document, in
 * `Doc.index`, and nothing in this file counts characters to find a line.
 */

/* The session token.
 *
 * From the meta tag the server stamped into this page, which is how the
 * stylesheet and this script were fetched at all. The query string is the
 * fallback, and only for the very first load. */
const TOKEN =
  (document.querySelector('meta[name="lcl-token"]') || {}).content ||
  new URLSearchParams(location.search).get("t") ||
  "";

/* ------------------------------------------------------------------ http */

async function api(method, path, params, body) {
  const url = new URL(path, location.origin);
  for (const [k, v] of Object.entries(params || {})) url.searchParams.set(k, v);
  const headers = { "X-LCL-Token": TOKEN };
  const init = { method, headers, cache: "no-store" };
  if (body !== undefined) {
    init.body = body;
    headers["Content-Type"] = "text/plain; charset=utf-8";
  }
  const reply = await fetch(url, init);
  const text = await reply.text();
  let parsed = null;
  try { parsed = text ? JSON.parse(text) : null; } catch (_) { parsed = null; }
  if (!reply.ok) {
    const detail = (parsed && parsed.error) || text || reply.statusText;
    throw new Error(detail);
  }
  return parsed;
}

/* ------------------------------------------------- byte / UTF-16 mapping */

/* An index over one document's text.
 *
 * `charAt[b]` is the JS string index of the character containing byte `b`.
 * `byteAt[i]` is the byte offset of JS string index `i`.
 * `lineStarts` holds the byte offset of each line's first byte.
 *
 * Built once when a document loads or changes, so nothing downstream has to
 * count anything. */
function buildIndex(text) {
  const byteAt = new Int32Array(text.length + 1);
  const bytes = [];
  let b = 0;
  for (let i = 0; i < text.length; ) {
    byteAt[i] = b;
    const cp = text.codePointAt(i);
    const size = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    const units = cp >= 0x10000 ? 2 : 1;
    for (let k = 0; k < size; k++) bytes.push(i);
    if (units === 2) byteAt[i + 1] = b;
    b += size;
    i += units;
  }
  byteAt[text.length] = b;
  bytes.push(text.length);
  const charAt = Int32Array.from(bytes);

  const lineStarts = [0];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "\n") lineStarts.push(byteAt[i] + 1);
  }
  return {
    charAt, byteAt, lineStarts, byteLength: b,
    char(byte) {
      if (byte <= 0) return 0;
      if (byte >= charAt.length) return text.length;
      return charAt[byte];
    },
    byte(char) {
      if (char <= 0) return 0;
      if (char >= byteAt.length) return b;
      return byteAt[char];
    },
    /* One-based line and column for a byte offset, counted the way the
     * engine counts: lines by U+000A, columns in scalar values. Used only
     * for the cursor readout; anything the engine reported carries its own
     * position and that one is displayed instead. */
    position(byte) {
      let lo = 0, hi = lineStarts.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (lineStarts[mid] <= byte) lo = mid; else hi = mid - 1;
      }
      const start = this.char(lineStarts[lo]);
      const here = this.char(byte);
      return { line: lo + 1, column: [...text.slice(start, here)].length + 1 };
    },
  };
}

/* --------------------------------------------------------------- state */

const state = {
  session: null,
  entries: [],
  docs: new Map(),      // id -> Doc
  order: [],            // tab order
  active: null,         // id
  syntax: null,
  run: null,            // the live run, when there is one
  grants: { allow_read: [], allow_write: [], allow_program: [], allow_host: [] },
  inputs: [],
  breakOnEffects: true,   // ask before every effect, which is the safe default
  breakOnOperations: false,
};

function Doc(id, text, digest) {
  return {
    id, text, digest,
    saved: text,
    revision: 0,
    pendingSaves: 0,
    saveTail: Promise.resolve(),
    index: buildIndex(text),
    tokens: null,        // token spans from the engine
    report: null,        // the last engine report for this document
    navigation: null,
    breakpoints: new Set(),
    stepAt: null,
  };
}

const current = () => (state.active ? state.docs.get(state.active) : null);
const dirty = (doc) => doc && doc.text !== doc.saved;

/* ----------------------------------------------------------------- dom */

const $ = (sel) => document.querySelector(sel);
const el = (tag, cls, text) => {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
};

const code = $("#code");
const paint = $("#paint");
const gutter = $("#gutter");

function toast(message, kind = "") {
  const node = el("div", `toast ${kind}`, message);
  $("#toasts").append(node);
  setTimeout(() => node.remove(), kind === "bad" ? 8000 : 4000);
}

function modal(title, build, actions) {
  $("#modal-title").textContent = title;
  const body = $("#modal-body");
  body.replaceChildren();
  build(body);
  const bar = $("#modal-actions");
  bar.replaceChildren();
  for (const [label, kind, run] of actions) {
    const button = el("button", kind, label);
    button.onclick = () => run(closeModal);
    bar.append(button);
  }
  $("#modal-backdrop").hidden = false;
  const first = body.querySelector("input, textarea") || bar.querySelector("button");
  if (first) first.focus();
}
function closeModal() { $("#modal-backdrop").hidden = true; }

/* -------------------------------------------------------------- project */

async function loadSession() {
  state.session = await api("GET", "/api/session");
  $("#project-root").textContent = state.session.root;
  $("#project-root").title = state.session.root;
  const spec = state.session.spec;
  $("#spec-identity").textContent =
    `${spec.formal_version} · ${spec.authority.toLowerCase()} · ${spec.identity_digest.slice(0, 12)}`;
  $("#spec-identity").title =
    `Specification package ${spec.root}\nidentity ${spec.identity_digest}\n` +
    `Every result shown here was produced against this package.`;
}

async function loadTree() {
  const reply = await api("GET", "/api/documents");
  state.entries = reply.entries;
  renderTree();
}

function renderTree() {
  const list = $("#tree");
  list.replaceChildren();
  for (const entry of state.entries) {
    const depth = entry.id.split("/").length - 1;
    const name = entry.id.split("/").pop();
    const item = el("li", entry.directory ? "dir" : "");
    item.style.paddingLeft = `${10 + depth * 12}px`;
    if (!entry.directory) {
      const doc = state.docs.get(entry.id);
      if (dirty(doc)) item.append(el("span", "dot", "●"));
    }
    item.append(document.createTextNode(name));
    item.title = entry.id;
    if (entry.id === state.active) item.classList.add("open");
    if (!entry.directory) item.onclick = () => openDocument(entry.id);
    list.append(item);
  }
}

/* ------------------------------------------------------------ documents */

async function openDocument(id, { focusByte } = {}) {
  if (!state.docs.has(id)) {
    const reply = await api("GET", "/api/document", { id });
    state.docs.set(id, Doc(reply.id, reply.text, reply.digest));
    state.order.push(id);
  }
  state.active = id;
  renderTabs();
  renderTree();
  const doc = current();
  code.value = doc.text;
  render();
  await refreshTokens();
  if (focusByte !== undefined) revealByte(focusByte);
  code.focus();
}

function closeDocument(id) {
  const doc = state.docs.get(id);
  if (!doc) return;
  const drop = () => {
    if (state.docs.get(id) !== doc) return;
    state.docs.delete(id);
    state.order = state.order.filter((x) => x !== id);
    if (state.active === id) {
      state.active = state.order[state.order.length - 1] || null;
      if (state.active) {
        const next = state.docs.get(state.active);
        code.value = next.text;
      } else {
        code.value = "";
      }
    }
    renderTabs(); renderTree(); render();
  };
  if (dirty(doc) || doc.pendingSaves) {
    modal("Unsaved changes", (body) => {
      body.append(el("p", "", `${id} has unsaved edits.`));
    }, [
      ["Cancel", "", (close) => close()],
      ["Discard", "", (close) => { close(); drop(); }],
      ["Save", "primary", async (close) => {
        close();
        if (await save(doc)) {
          if (!dirty(doc) && !doc.pendingSaves) drop();
          else toast(`${id} still has unsaved changes.`, "warn");
        }
      }],
    ]);
    return;
  }
  drop();
}

function renderTabs() {
  const bar = $("#tabs");
  bar.replaceChildren();
  for (const id of state.order) {
    const doc = state.docs.get(id);
    const item = el("div", `tab-item${id === state.active ? " active" : ""}`);
    if (dirty(doc)) item.append(el("span", "dirty", "●"));
    item.append(el("span", "", id.split("/").pop()));
    const close = el("span", "close", "×");
    close.onclick = (e) => { e.stopPropagation(); closeDocument(id); };
    item.append(close);
    item.title = id;
    item.onclick = () => openDocument(id);
    bar.append(item);
  }
}

async function save(doc = current()) {
  if (!doc || state.docs.get(doc.id) !== doc) return false;
  // Snapshot at the user's Save action, even when an earlier save is pending.
  const submitted = doc.text;
  const revision = doc.revision;
  doc.pendingSaves++;
  const pending = doc.saveTail.then(async () => {
    // Discard can close a document while its next save is still queued.
    if (state.docs.get(doc.id) !== doc) return false;
    let reply, persisted;
    try {
      reply = await api("PUT", "/api/document", { id: doc.id }, submitted);
      if (!reply || reply.id !== doc.id || typeof reply.digest !== "string" ||
          typeof reply.final_line_feed_added !== "boolean") {
        throw new Error("The server did not acknowledge this document's save.");
      }
      persisted = submitted + (reply.final_line_feed_added ? "\n" : "");
      if (reply.bytes !== buildIndex(persisted).byteLength) {
        throw new Error("The server did not acknowledge the submitted content length.");
      }
    } catch (e) {
      toast(`Not saved. ${e.message}`, "bad");
      return false;
    }
    // Only acknowledged bytes form the saved baseline. Later input belongs to
    // the next revision and must never be overwritten or marked as saved here.
    doc.saved = persisted;
    doc.digest = reply.digest;
    if (doc.revision === revision && doc.text === submitted) {
      doc.text = persisted;
      doc.index = buildIndex(persisted);
      if (current() === doc) code.value = persisted;
    }
    if (reply.final_line_feed_added) {
      toast("A final line feed was added, which 02_LEXICAL/01 requires.", "warn");
    }
    renderTabs(); renderTree(); render();
    toast(`Saved ${doc.id}`, "good");
    // A refresh failure cannot turn an acknowledged write into a failed save.
    try {
      await loadTree();
      if (current() === doc) await refreshTokens();
    } catch (e) {
      toast(`Saved ${doc.id}; could not refresh the project. ${e.message}`, "warn");
    }
    return true;
  }).finally(() => { doc.pendingSaves--; });
  // A rejected UI task must not poison this document's future save queue.
  doc.saveTail = pending.catch(() => false);
  return pending;
}

async function reload() {
  const doc = current();
  if (!doc) return;
  const go = async () => {
    // What this reload is answering for. Anything typed after this point was
    // never shown to the person who asked for it, and a discard they confirmed
    // covered the edits in front of them, not edits that did not exist yet.
    const revision = doc.revision;
    const reply = await api("GET", "/api/document", { id: doc.id });
    // The tab may have been closed and reopened while the request was out; that
    // path is a different document object with its own edits.
    if (state.docs.get(doc.id) !== doc) return;
    // The response is authoritative about what is on disk either way.
    doc.saved = reply.text;
    doc.digest = reply.digest;
    if (doc.revision !== revision) {
      renderTabs(); renderTree(); render();
      toast(`${doc.id} was edited while it reloaded; your edits are kept.`, "warn");
      return;
    }
    doc.text = reply.text;
    doc.revision++;
    doc.index = buildIndex(doc.text);
    if (current() === doc) code.value = doc.text;
    renderTabs(); renderTree(); render();
    await refreshTokens();
    toast(`Reloaded ${doc.id}`);
  };
  if (dirty(doc)) {
    modal("Discard edits?", (body) => {
      body.append(el("p", "", `Reloading ${doc.id} throws away your unsaved edits.`));
    }, [
      ["Cancel", "", (close) => close()],
      ["Discard and reload", "primary", (close) => { close(); go(); }],
    ]);
    return;
  }
  await go();
}

async function newDocument() {
  modal("New document", (body) => {
    body.append(el("p", "",
      "A path inside the project. New documents are created as .lcl.txt, " +
      "so they open anywhere plain text does. Existing .lcl documents keep " +
      "their name."));
    const input = el("input", "field");
    input.id = "new-path";
    input.value = "untitled.lcl.txt";
    body.append(input);
  }, [
    ["Cancel", "", (close) => close()],
    ["Create", "primary", async (close) => {
      const id = $("#new-path").value.trim();
      close();
      if (!id) return;
      try {
        /* A new document starts as the smallest thing the grammar accepts.
         * 04_GRAMMAR/01: "Every document starts with LCL then SPECIFICATION."
         * The values are placeholders; the engine judges them like any other. */
        const seed =
          'LCL:\n    VERSION: "0.1.0"\n\n' +
          'SPECIFICATION:\n    ID: example.new\n    NAME: "New document"\n' +
          '    VERSION: "1.0.0"\n    KIND: kind.task\n    DOMAIN: "general"\n';
        /* Creating is its own route: it applies the .lcl.txt default and
         * refuses to overwrite. Saving stays exact, so an open document is
         * never renamed under the person editing it. The server decides the
         * final name, and the reply says what it chose. */
        const created = await api("POST", "/api/document", { id }, seed);
        await loadTree();
        await openDocument(created.id);
        toast(
          created.id === created.requested
            ? `Created ${created.id}`
            : `Created ${created.id}, the default name for ${created.requested}`,
          "good",
        );
      } catch (e) {
        toast(`Not created. ${e.message}`, "bad");
      }
    }],
  ]);
}

/* --------------------------------------------------------------- render */

/* Paint the document.
 *
 * Phase C replaces the plain text with engine-supplied token spans; until a
 * document has tokens, it renders as plain text rather than as a guess. */
function render() {
  const doc = current();
  if (!doc) {
    paint.replaceChildren();
    gutter.replaceChildren();
    $("#doc-state").textContent = "";
    return;
  }
  paintTokens(doc);
  renderGutter(doc);
  $("#doc-state").textContent = dirty(doc) ? "modified" : "saved";
  syncScroll();
}

function renderGutter(doc) {
  const lines = doc.text.split("\n").length;
  const marks = diagnosticLines(doc);
  const frag = document.createDocumentFragment();
  for (let n = 1; n <= lines; n++) {
    const line = el("span", "ln", String(n));
    const mark = marks.get(n);
    if (mark) line.classList.add(`has-${mark}`);
    if (doc.breakpoints.has(n)) line.classList.add("has-break");
    if (doc.stepAt === n) line.classList.add("at-step");
    line.onclick = () => toggleBreakpoint(doc, n);
    frag.append(line);
  }
  gutter.replaceChildren(frag);
}

function syncScroll() {
  paint.scrollTop = code.scrollTop;
  paint.scrollLeft = code.scrollLeft;
  gutter.scrollTop = code.scrollTop;
}

function revealByte(byte) {
  const doc = current();
  if (!doc) return;
  const at = doc.index.char(byte);
  code.focus();
  code.setSelectionRange(at, at);
  const { line } = doc.index.position(byte);
  const row = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--row")) || 20;
  code.scrollTop = Math.max(0, (line - 6) * row);
  syncScroll();
  updateCursor();
}

function updateCursor() {
  const doc = current();
  if (!doc) return;
  const byte = doc.index.byte(code.selectionStart);
  const { line, column } = doc.index.position(byte);
  $("#cursor").textContent = `${line}:${column} (byte ${byte})`;
}

/* ------------------------------------------------- painting (phase C) */

/* Above this size the document renders as plain text.
 * Decoration is per byte, and a document this large in an editor is a
 * generated artefact rather than something a person is reading. */
const PAINT_LIMIT = 400000;

/* Paint one document.
 *
 * Three layers, all of them engine-supplied: token classes from the lexer,
 * squiggles from the diagnostics each stage emitted, and reference marks from
 * the resolver's bindings. The frontend decides none of the three; it decides
 * only which CSS class expresses each.
 *
 * Built with DOM nodes rather than innerHTML. The content is the user's
 * document, and assembling markup out of it would make every `<` in a string
 * literal a way into this page. */
function paintTokens(doc) {
  const text = doc.text;
  const total = doc.index.byteLength;

  if (!doc.tokens || total > PAINT_LIMIT) {
    paint.replaceChildren(document.createTextNode(text));
    return;
  }

  /* Per-byte decoration. Documents here are kilobytes; three typed arrays
   * are cheaper and far simpler than interval arithmetic. */
  const cls = new Uint8Array(total);      // 0 none, else CLASS_NAMES index
  const squiggle = new Uint8Array(total); // 0 none, else SEVERITY index
  const isRef = new Uint8Array(total);

  const CLASS_NAMES = ["", "keyword", "block", "type", "literal", "string", "symbol", "ident"];
  const SEVERITY = ["", "bad", "warn", "info"];

  for (const token of doc.tokens) {
    const id = CLASS_NAMES.indexOf(token.class);
    if (id <= 0) continue;
    for (let b = token.start; b < token.end && b < total; b++) cls[b] = id;
  }

  if (doc.report && doc.report.diagnostics) {
    for (const d of doc.report.diagnostics) {
      if (d.source !== doc.id) continue;
      const id = SEVERITY.indexOf(severityOf(d));
      if (id <= 0) continue;
      const end = Math.max(d.span.end, d.span.start + 1);
      for (let b = d.span.start; b < end && b < total; b++) {
        if (squiggle[b] < id || squiggle[b] === 0) squiggle[b] = id;
      }
    }
  }

  if (doc.navigation) {
    for (const r of doc.navigation.references) {
      if (r.source !== doc.id) continue;
      for (let b = r.span.start; b < r.span.end && b < total; b++) isRef[b] = 1;
    }
  }

  /* Walk the bytes, cutting a new span wherever any layer changes. */
  const frag = document.createDocumentFragment();
  let segmentStart = 0;
  const emit = (fromByte, toByte) => {
    if (toByte <= fromByte) return;
    const slice = text.slice(doc.index.char(fromByte), doc.index.char(toByte));
    const c = cls[fromByte], s = squiggle[fromByte], r = isRef[fromByte];
    if (!c && !s && !r) {
      frag.append(document.createTextNode(slice));
      return;
    }
    const node = document.createElement("span");
    const names = [];
    if (r) names.push("t-ref");
    else if (c) names.push(`t-${CLASS_NAMES[c]}`);
    if (s) names.push("sq", `sq-${SEVERITY[s]}`);
    node.className = names.join(" ");
    node.textContent = slice;
    frag.append(node);
  };

  for (let b = 1; b <= total; b++) {
    const changed =
      b === total ||
      cls[b] !== cls[segmentStart] ||
      squiggle[b] !== squiggle[segmentStart] ||
      isRef[b] !== isRef[segmentStart];
    if (changed) {
      emit(segmentStart, b);
      segmentStart = b;
    }
  }
  paint.replaceChildren(frag);
}

/* How a diagnostic is drawn.
 *
 * A closed map over the registered `default_status` the emitting layer
 * resolved. `statuses_and_errors_v0.1.0.json` gives every error one of exactly
 * four, so this switch is total over the registry rather than a guess at what
 * a status name might contain. An unregistered value is drawn as `info` and
 * shows its status text, which makes a registry that has moved visible instead
 * of silently mis-coloured.
 *
 * Choosing a colour is presentation. Deciding the status is not, and that
 * decision arrived with the diagnostic. */
const SEVERITY_OF_STATUS = {
  "status.failed": "bad",
  "status.invalid": "bad",
  "status.blocked": "warn",
  "status.cancelled": "warn",
};
function severityOf(d) {
  return SEVERITY_OF_STATUS[d.default_status] || "info";
}

function diagnosticLines(doc) {
  const marks = new Map();
  if (!doc.report || !doc.report.diagnostics) return marks;
  for (const d of doc.report.diagnostics) {
    if (d.source !== doc.id) continue;
    /* The engine's own derived line, not one counted here. */
    const line = d.position.line;
    const severity = severityOf(d) === "bad" ? "bad" : "warn";
    if (marks.get(line) !== "bad") marks.set(line, severity);
  }
  return marks;
}

/* ------------------------------------------------- analysis (phase C) */

async function refreshTokens() {
  const doc = current();
  if (!doc) return;
  try {
    const reply = await api("POST", "/api/tokens", { id: doc.id }, doc.text);
    doc.tokens = reply.tokens;
    render();
  } catch (_) {
    /* A failed token request leaves the document painted as plain text,
     * which is honest: no engine answer, no highlighting. */
  }
}

async function runAnalysis() {
  const doc = current();
  if (!doc) return;
  await refreshTokens();
  try {
    /* Inspect rather than check: it reaches the same diagnostics and also
     * carries the resolver's bindings, which is what navigation needs. A
     * document that fails before resolution still gets its diagnostics. */
    const report = await api("POST", "/api/inspect", { id: doc.id }, doc.text);
    doc.report = report;
    doc.navigation = report.navigation || null;
    /* Kept separately from the report: a run replaces `report`, and stepping
     * needs the plan to turn an invocation's node index into a span. */
    if (report.structure) doc.plan = report.structure.plan;
    renderDiagnostics(doc);
    renderStructure(doc);
    render();
  } catch (e) {
    toast(`Analysis failed: ${e.message}`, "bad");
  }
}

function view(name) {
  return document.querySelector(`.view[data-view="${name}"]`);
}

function renderDiagnostics(doc) {
  const panel = view("diagnostics");
  const report = doc.report;
  panel.replaceChildren();

  if (!report) { panel.append(el("div", "empty", "Nothing checked yet.")); return; }

  const head = el("div", "group");
  const outcome = report.outcome;
  const status = el("div", `terminal-status ${outcome === "accepted" ? "good" : "bad"}`);
  status.append(document.createTextNode(
    outcome === "accepted"
      ? `Accepted through ${report.reached}`
      : `${outcome} at ${report.reached}`));
  /* Contract 5.6: a stage verdict is never a claim that the document ran. */
  status.append(el("span", "why",
    outcome === "accepted"
      ? "Every stage up to here produced no unhandled diagnostic. This is not a claim that the document ran or succeeded."
      : "Processing stopped at this stage."));
  head.append(status);
  panel.append(head);

  if (!report.diagnostics.length) {
    panel.append(el("div", "empty", "No diagnostics."));
    return;
  }

  for (const d of report.diagnostics) {
    const item = el("div", `diag ${severityOf(d)}${d.primary ? " primary" : ""}`);
    const id = el("div", "id");
    if (d.primary) id.append(el("span", "badge bad", "primary"));
    id.append(document.createTextNode(d.id));
    item.append(id);

    const meta = el("div", "meta",
      `${d.source}:${d.position.line}:${d.position.column} · byte ${d.span.start}–${d.span.end} · ` +
      `${d.stage} · ${d.default_status}`);
    item.append(meta);
    item.append(el("div", "meaning", d.meaning));
    if (d.detail) item.append(el("div", "detail", d.detail));

    item.onclick = async () => {
      if (d.source !== doc.id && state.docs.has(d.source) === false) {
        try { await openDocument(d.source, { focusByte: d.span.start }); return; } catch (_) {}
      }
      if (d.source !== doc.id) { await openDocument(d.source, { focusByte: d.span.start }); return; }
      revealByte(d.span.start);
    };
    panel.append(item);
  }
}

function renderStructure(doc) {
  const panel = view("structure");
  panel.replaceChildren();
  const s = doc.report && doc.report.structure;
  if (!s) {
    panel.append(el("div", "empty",
      "The document did not reach the structural view. Fix the diagnostics first."));
    return;
  }

  if (s.imports.length) {
    const group = el("div", "group");
    group.append(el("h3", "", "Imports"));
    const rows = el("div", "rows");
    for (const i of s.imports) {
      const row = el("div", "row");
      row.append(el("span", "k", i.kind));
      row.append(el("span", "v", i.id));
      row.append(el("span", "tail", i.outcome));
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }

  if (doc.navigation) {
    const group = el("div", "group");
    group.append(el("h3", "", `Declarations (${doc.navigation.declarations.length})`));
    const rows = el("div", "rows");
    for (const d of doc.navigation.declarations) {
      const row = el("div", "row clickable");
      row.append(el("span", "k", d.block));
      row.append(el("span", "v", d.id));
      const uses = doc.navigation.references.filter((r) => r.declaration === d.index).length;
      row.append(el("span", "tail", uses ? `${uses} ref${uses === 1 ? "" : "s"}` : "unused"));
      row.onclick = async () => {
        if (d.source !== doc.id) await openDocument(d.source, { focusByte: d.id_span.start });
        else revealByte(d.id_span.start);
      };
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }

  if (s.plan.length) {
    const group = el("div", "group");
    group.append(el("h3", "", `Execution plan (${s.plan.length} nodes, ${s.candidates} candidates)`));
    const rows = el("div", "rows");
    for (const node of s.plan) {
      const row = el("div", "row clickable plan-node");
      row.style.setProperty("--depth", depthOf(s.plan, node));
      row.append(el("span", "k", node.block));
      row.append(el("span", "v", node.id || ""));
      if (node.operation) row.append(el("span", "tail", node.operation));
      else if (node.order !== null) row.append(el("span", "tail", `#${node.order}`));
      row.onclick = () => revealByte(node.span.start);
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }

  if (s.unused_inputs.length) {
    const group = el("div", "group");
    group.append(el("h3", "", "Supplied but never declared"));
    const rows = el("div", "rows");
    for (const id of s.unused_inputs) {
      const row = el("div", "row");
      row.append(el("span", "v", id));
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }
}

function depthOf(plan, node) {
  let depth = 0, at = node;
  while (at && at.parent !== null && depth < 12) {
    at = plan[at.parent];
    depth++;
  }
  return depth;
}

/* -------------------------------------------------- navigation (phase C) */

/* Go to definition.
 *
 * Follows `declaration` on the resolver's binding. There is no search, no
 * fuzzy match and no fallback to "find the word somewhere else": if the
 * resolver did not bind it, this says so rather than guessing a target. */
function goToDefinition() {
  const doc = current();
  if (!doc || !doc.navigation) { toast("No resolver data yet.", "warn"); return; }
  const byte = doc.index.byte(code.selectionStart);

  const reference = doc.navigation.references.find(
    (r) => r.source === doc.id && r.span.start <= byte && byte < r.span.end);
  if (!reference) { toast("The cursor is not on a reference.", "warn"); return; }

  if (reference.target === "unresolved") {
    toast(`${reference.text} does not resolve to a declaration.`, "bad");
    return;
  }
  if (reference.target === "loop_local") {
    revealByte(reference.binding_span.start);
    toast(`${reference.text} is a FOR EACH binding, not a declaration.`);
    return;
  }
  const target = doc.navigation.declarations[reference.declaration];
  if (target.source !== doc.id) openDocument(target.source, { focusByte: target.id_span.start });
  else revealByte(target.id_span.start);
}

/* Find references to the declaration or reference under the cursor. */
function findReferences() {
  const doc = current();
  if (!doc || !doc.navigation) { toast("No resolver data yet.", "warn"); return; }
  const byte = doc.index.byte(code.selectionStart);
  const nav = doc.navigation;

  let index = null;
  const declaration = nav.declarations.find(
    (d) => d.source === doc.id && d.id_span.start <= byte && byte < d.id_span.end);
  if (declaration) index = declaration.index;
  else {
    const reference = nav.references.find(
      (r) => r.source === doc.id && r.span.start <= byte && byte < r.span.end);
    if (reference && reference.declaration !== null) index = reference.declaration;
  }
  if (index === null) { toast("Put the cursor on a declaration or a reference.", "warn"); return; }

  const target = nav.declarations[index];
  const uses = nav.references.filter((r) => r.declaration === index);
  modal(`References to ${target.id}`, (body) => {
    if (!uses.length) { body.append(el("p", "", "Nothing references this declaration.")); return; }
    const rows = el("div", "rows");
    for (const use of uses) {
      const row = el("div", "row clickable");
      row.append(el("span", "k", `${use.source}:${use.position.line}:${use.position.column}`));
      row.append(el("span", "v", use.slot ? `${use.slot.block}.${use.slot.field}` : use.text));
      row.onclick = async () => {
        closeModal();
        if (use.source !== doc.id) await openDocument(use.source, { focusByte: use.span.start });
        else revealByte(use.span.start);
      };
      rows.append(row);
    }
    body.append(rows);
  }, [["Close", "primary", (close) => close()]]);
}

/* ----------------------------------------------------- running (phase D) */

/* Start a run and follow its event stream.
 *
 * The run happens on a thread in the server; this reads what it observed. The
 * report that ends the stream is the engine's own record, and every view below
 * renders fields out of it rather than deriving anything. */
async function startRun() {
  const doc = current();
  if (!doc) return;
  if (state.run && !state.run.finished) {
    toast("A run is already going. Stop it first.", "warn");
    return;
  }

  const params = { id: doc.id };
  if (state.breakOnEffects) params.break_effects = "1";
  if (state.breakOnOperations) params.break_operations = "1";
  for (const [key, values] of Object.entries(state.grants)) {
    if (values.length) params[key] = values.join("\n");
  }
  if (state.inputs.length) params.input = state.inputs.join("\n");

  let started;
  try {
    started = await api("POST", "/api/run", params, doc.text);
  } catch (e) {
    toast(`Run refused: ${e.message}`, "bad");
    return;
  }

  state.run = {
    id: started.run, finished: false, paused: null,
    operations: [], effects: [], report: null, document: doc.id,
  };
  doc.stepAt = null;
  showView("execution");
  renderExecution();
  follow(state.run);
}

/* Read one run's event stream.
 *
 * EventSource cannot send a header, and the token has to travel somehow, so it
 * goes in the query string exactly as it does for the first page load. The
 * connection is same-origin and loopback; there is no third party to leak to. */
function follow(run) {
  const url = new URL("/api/events", location.origin);
  url.searchParams.set("run", run.id);
  url.searchParams.set("t", TOKEN);
  const source = new EventSource(url);
  run.source = source;

  source.addEventListener("operation", (e) => {
    run.operations.push(JSON.parse(e.data));
    renderExecution();
  });
  source.addEventListener("permission", (e) => {
    run.effects.push({ ...JSON.parse(e.data), stage: "permission" });
    renderExecution();
  });
  source.addEventListener("effect", (e) => {
    run.effects.push({ ...JSON.parse(e.data), stage: "effect" });
    renderExecution();
  });
  source.addEventListener("paused", (e) => {
    run.paused = JSON.parse(e.data);
    onPaused(run);
  });
  source.addEventListener("resumed", () => {
    run.paused = null;
    renderExecution();
  });
  source.addEventListener("report", (e) => {
    run.report = JSON.parse(e.data);
    const doc = state.docs.get(run.document);
    if (doc) { doc.report = run.report; doc.stepAt = null; }
    renderDiagnostics(doc || {});
    renderExecution();
    renderCompletion(run.report);
    render();
  });
  source.addEventListener("failed", (e) => {
    toast(`Run failed: ${JSON.parse(e.data).error}`, "bad");
  });
  source.addEventListener("end", () => {
    run.finished = true;
    run.paused = null;
    source.close();
    renderExecution();
    if (run.report && run.report.completion) showView("completion");
  });
  source.onerror = () => {
    if (!run.finished) { run.finished = true; source.close(); renderExecution(); }
  };
}

async function stopRun() {
  if (!state.run || state.run.finished) return;
  await api("POST", "/api/answer",
    { run: state.run.id, sequence: (state.run.paused || {}).sequence || 0, answer: "cancel" });
  toast("Stopping the run. Every remaining effect is refused.", "warn");
}

/* ------------------------------------------------------- execution view */

function renderExecution() {
  const panel = view("execution");
  panel.replaceChildren();
  const run = state.run;
  if (!run) {
    panel.append(el("div", "empty", "Run the document to see invocations and events."));
    return;
  }

  panel.append(debugBar(run));

  const report = run.report;
  if (report && report.execution) {
    const x = report.execution;

    const group = el("div", "group");
    group.append(el("h3", "", `Invocations (${x.invocations.length}, ${x.steps} steps)`));
    const rows = el("div", "rows");
    x.invocations.forEach((inv, i) => {
      const row = el("div", "row clickable");
      if (run.stepIndex === i) row.classList.add("current");
      row.append(el("span", "k", inv.block));
      row.append(el("span", "v", inv.declaration || ""));
      row.append(el("span", "tail", inv.status));
      row.onclick = () => stepTo(i);
      rows.append(row);
    });
    group.append(rows);
    panel.append(group);

    if (x.events.length) {
      const events = el("div", "group");
      events.append(el("h3", "", `Raised events (${x.events.length})`));
      const rows = el("div", "rows");
      for (const e of x.events) {
        const row = el("div", "row");
        row.append(el("span", "k", e.event));
        row.append(el("span", "v", e.producer));
        row.append(el("span", "tail", e.disposition));
        rows.append(row);
      }
      events.append(rows);
      panel.append(events);
    }
  }

  if (run.effects.length) {
    const group = el("div", "group");
    group.append(el("h3", "", "Capability boundary"));
    const rows = el("div", "rows");
    for (const e of run.effects) {
      const row = el("div", "row");
      row.append(el("span", "k", e.stage));
      row.append(el("span", "v", e.operation));
      const verdict = e.permission || e.outcome;
      const badge = el("span", `badge ${verdict === "granted" || verdict === "completed" ? "good" : "bad"}`, verdict);
      const tail = el("span", "tail");
      tail.append(badge);
      row.append(tail);
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }

  if (!report && !run.finished) {
    panel.append(el("div", "empty", "Running…"));
  }
}

/* ------------------------------------------------------ completion view */

function renderCompletion(report) {
  const panel = view("completion");
  panel.replaceChildren();
  const c = report && report.completion;
  if (!c) {
    panel.append(el("div", "empty",
      "The run did not reach completion. The Diagnostics view says where it stopped."));
    return;
  }

  /* One terminal status, and the completion layer's own stated reason for it.
   * 05_SEMANTICS/10 keeps producer completion and domain outcome apart, so
   * this shows the status the engine chose and does not restate it as a
   * verdict of its own. */
  const succeeded = c.terminal_status === "status.succeeded";
  const status = el("div", `terminal-status ${succeeded ? "good" : "bad"}`);
  status.append(document.createTextNode(c.terminal_status));
  status.append(el("span", "why", c.reason));
  panel.append(status);

  if (c.checks.length) {
    const group = el("div", "group");
    group.append(el("h3", "", `Checks (${c.checks.length})`));
    const rows = el("div", "rows");
    for (const check of c.checks) {
      const row = el("div", "row clickable");
      row.append(el("span", "k", check.kind));
      row.append(el("span", "v", check.id));
      const tail = el("span", "tail");
      if (check.skipped) tail.append(el("span", "badge warn", `skipped: ${check.skipped}`));
      else tail.append(el("span", `badge ${check.outcome === "TRUE" ? "good" : "bad"}`, check.outcome));
      row.append(tail);
      row.onclick = () => revealByte(check.span.start);
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }

  if (c.evidence.length) {
    const group = el("div", "group");
    group.append(el("h3", "", `Evidence (${c.evidence.length})`));
    const rows = el("div", "rows");
    for (const ev of c.evidence) {
      const row = el("div", "row clickable");
      row.append(el("span", "k", ev.provision));
      row.append(el("span", "v", ev.id));
      row.append(el("span", "tail", ev.satisfied ? "satisfied" : "missing"));
      row.onclick = () => revealByte(ev.span.start);
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }

  const verdict = c.verdict;
  if (verdict && (verdict.success || verdict.failure)) {
    const group = el("div", "group");
    group.append(el("h3", "", "Verdict"));
    const list = el("dl", "kv");
    const pair = (k, v) => { list.append(el("dt", "", k)); list.append(el("dd", "", v)); };
    if (verdict.success) pair("SUCCESS", verdict.success);
    if (verdict.quantifier) pair("quantifier", verdict.quantifier);
    if (verdict.value) pair("value", verdict.value);
    for (const m of verdict.members || []) pair(m.id, m.value);
    if (verdict.failure) pair("FAILURE", verdict.failure);
    if (verdict.failure_status) pair("status", verdict.failure_status);
    group.append(list);
    panel.append(group);
  }

  if (c.outputs.length) {
    const group = el("div", "group");
    group.append(el("h3", "", `Outputs (${c.outputs.length})`));
    const rows = el("div", "rows");
    for (const out of c.outputs) {
      const row = el("div", "row");
      row.append(el("span", "k", out.id));
      row.append(el("span", "v", out.value === null ? "—" : out.value));
      row.append(el("span", "tail", out.publication));
      rows.append(row);
    }
    group.append(rows);
    panel.append(group);
  }
}

/* ---------------------------------------------------- debugging (phase E) */

/* What this debugger can honestly do, and what it cannot.
 *
 * It stops at the capability boundary: before the standard library dispatches
 * an operation, and before an effect crosses into the host. That is where an
 * effect leaves the language, so it is both the most useful place to stop and
 * the only place the engine already exposes a seam for it.
 *
 * It does not single-step plan nodes. The runtime executes an accepted plan in
 * one call over its own deterministic queue, and pausing between nodes would
 * mean changing that closed milestone. Rather than offer a Step button that
 * silently means something weaker, there is none: after a run, the recorded
 * invocations are steppable, and that is described as what it is.
 *
 * A breakpoint on a line is a filter over effect pauses, not a new stopping
 * place. With breakpoints set, a pause whose span is not on a marked line is
 * answered immediately and the run carries on. */

function debugBar(run) {
  const bar = el("div", `debugbar${run.paused ? " paused" : ""}`);

  const state_ = el("span", "state",
    run.paused ? `paused · ${run.paused.kind}` : run.finished ? "finished" : "running");
  bar.append(state_);
  bar.append(el("span", "spacer"));

  if (run.paused) {
    const go = el("button", "primary", "Continue");
    go.onclick = () => answerPause(run, "continue");
    const deny = el("button", "", "Deny this effect");
    deny.onclick = () => answerPause(run, "deny");
    bar.append(go, deny);
  }
  if (!run.finished) {
    const stop = el("button", "", "Stop");
    stop.onclick = stopRun;
    bar.append(stop);
  }

  if (run.finished && run.report && run.report.execution) {
    const count = run.report.execution.invocations.length;
    const back = el("button", "", "◀");
    back.title = "Step back through the recorded invocations";
    back.onclick = () => stepTo((run.stepIndex === undefined ? count : run.stepIndex) - 1);
    const fwd = el("button", "", "▶");
    fwd.title = "Step forward through the recorded invocations";
    fwd.onclick = () => stepTo((run.stepIndex === undefined ? -1 : run.stepIndex) + 1);
    const where = el("span", "state",
      run.stepIndex === undefined ? `${count} recorded` : `${run.stepIndex + 1} / ${count}`);
    bar.append(back, fwd, where);
  }
  return bar;
}

/* Step to one recorded invocation and show where it was written.
 *
 * The span comes from the execution plan the engine produced, by the node
 * index the invocation record carries. Nothing is searched for. */
function stepTo(index) {
  const run = state.run;
  if (!run || !run.report || !run.report.execution) return;
  const invocations = run.report.execution.invocations;
  if (!invocations.length) return;

  index = Math.max(0, Math.min(index, invocations.length - 1));
  run.stepIndex = index;
  const invocation = invocations[index];

  const doc = state.docs.get(run.document);
  if (doc && doc.plan && doc.plan[invocation.node]) {
    const node = doc.plan[invocation.node];
    if (node.source === doc.id) {
      doc.stepAt = doc.index.position(node.span.start).line;
      revealByte(node.span.start);
    }
  }
  renderExecution();
  render();
}

/* A line breakpoint filters effect pauses; it is not a new stopping place. */
function toggleBreakpoint(doc, line) {
  if (doc.breakpoints.has(line)) doc.breakpoints.delete(line);
  else doc.breakpoints.add(line);
  renderGutter(doc);
}

/* The run stopped. Decide whether this is a pause the operator asked for. */
function onPaused(run) {
  const doc = state.docs.get(run.document);
  const pause = run.paused;

  /* With breakpoints set, only a pause on a marked line is shown. Everything
   * else continues immediately, which is what makes a breakpoint a filter
   * rather than a second kind of stop. */
  if (doc && doc.breakpoints.size && pause.source === doc.id) {
    const line = doc.index.position(pause.span.start).line;
    if (!doc.breakpoints.has(line)) { answerPause(run, "continue"); return; }
  }

  if (doc && pause.source === doc.id) {
    doc.stepAt = doc.index.position(pause.span.start).line;
    revealByte(pause.span.start);
  }
  renderExecution();
  showView("execution");
  showConsent(run, pause);
}

/* Ask before the effect.
 *
 * Everything the capability contract says an operator needs in order to judge
 * a request: the operation, its target, its parameters, what the language
 * authorized and which rule did it, what effects it may have, and exactly
 * where it is written.
 *
 * Contract 5.7: saying yes here adds a host permission. It does not add a
 * language authorization, and it cannot: this pause is only ever reached for
 * a request the engine already authorized, and an unauthorized one is refused
 * before any of this is drawn. */
function showConsent(run, pause) {
  modal(
    pause.kind === "effect" ? "Allow this effect?" : "Run this operation?",
    (body) => {
      const box = el("div", "request");
      const line = (k, v) => {
        const row = el("div");
        row.append(el("span", "k", k));
        row.append(document.createTextNode(v));
        box.append(row);
      };
      line("operation", pause.operation);
      if (pause.target) line("target", pause.target);
      for (const p of pause.parameters) line(p.name, p.value);
      line("category", pause.category);
      if (pause.possible_effects.length) line("effects", pause.possible_effects.join(", "));
      line("written at", `${pause.source} byte ${pause.span.start}`);
      body.append(box);

      const auth = el("div", "group");
      auth.append(el("h3", "", "What LCL authorized"));
      const list = el("dl", "kv");
      const pair = (k, v) => { list.append(el("dt", "", k)); list.append(el("dd", "", v)); };
      pair("operation", pause.authorization.operation);
      if (pause.authorization.scope) pair("scope", pause.authorization.scope);
      pair("permitted by", pause.authorization.permitted_by.join(", ") || "—");
      if (pause.authorization.overridden.length) {
        pair("overridden", pause.authorization.overridden.join(", "));
      }
      auth.append(list);
      body.append(auth);

      const note = el("p", "empty",
        "Allowing this grants host permission for this one request. " +
        "It does not change what the document authorizes.");
      body.append(note);
    },
    [
      ["Stop the run", "", (close) => { close(); answerPause(run, "cancel"); }],
      ["Deny", "", (close) => { close(); answerPause(run, "deny"); }],
      ["Allow", "primary", (close) => { close(); answerPause(run, "continue"); }],
    ]
  );
}

async function answerPause(run, answer) {
  const pause = run.paused;
  if (!pause) return;
  try {
    await api("POST", "/api/answer",
      { run: run.id, sequence: String(pause.sequence), answer });
    run.paused = null;
    renderExecution();
  } catch (e) {
    /* 409 means the pause moved on. Saying so beats silently doing nothing. */
    toast(`That pause is no longer current: ${e.message}`, "warn");
  }
}

/* --------------------------------------------------- capabilities (phase E) */

function renderCapabilities() {
  const panel = view("capabilities");
  panel.replaceChildren();

  const intro = el("div", "empty",
    "A run grants the host nothing unless you name it here. " +
    "Both gates must pass for an effect: the document must authorize it, and " +
    "you must permit it.");
  panel.append(intro);

  const group = el("div", "group");
  group.append(el("h3", "", "Host grants"));
  for (const [key, label, hint] of [
    ["allow_read", "Read paths", "One path per line"],
    ["allow_write", "Write paths", "One path per line"],
    ["allow_program", "Programs", "One program per line"],
    ["allow_host", "Network hosts", "One host per line"],
  ]) {
    const field = el("div");
    field.append(el("h3", "", label));
    const area = document.createElement("textarea");
    area.className = "field";
    area.rows = 2;
    area.placeholder = hint;
    area.value = state.grants[key].join("\n");
    area.oninput = () => {
      state.grants[key] = area.value.split("\n").map((s) => s.trim()).filter(Boolean);
    };
    field.append(area);
    group.append(field);
  }
  panel.append(group);

  const data = el("div", "group");
  data.append(el("h3", "", "Supplied data"));
  const area = document.createElement("textarea");
  area.className = "field";
  area.rows = 3;
  area.placeholder = "one per line, as id=expression";
  area.value = state.inputs.join("\n");
  area.oninput = () => {
    state.inputs = area.value.split("\n").map((s) => s.trim()).filter(Boolean);
  };
  data.append(area);
  data.append(el("div", "empty",
    "The engine turns each expression into a value with its own evaluation. " +
    "This workspace does not read literals."));
  panel.append(data);

  const stops = el("div", "group");
  stops.append(el("h3", "", "Where a run stops"));
  for (const [key, label, hint] of [
    ["breakOnEffects", "Ask before every effect",
     "Pause at the host boundary, where an effect leaves the language."],
    ["breakOnOperations", "Ask before every operation",
     "Pause at the standard library's dispatch, before the host is reached."],
  ]) {
    const row = el("label", "row clickable");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = state[key];
    box.onchange = () => { state[key] = box.checked; };
    row.append(box);
    row.append(el("span", "v", label));
    stops.append(row);
    stops.append(el("div", "empty", hint));
  }
  const note = el("div", "empty",
    "Stepping between plan nodes is not offered. The runtime executes an " +
    "accepted plan in one call, and a control that claimed otherwise would be " +
    "a control that does not work. After a run, the recorded invocations step " +
    "forward and back in the Execution view.");
  stops.append(note);
  panel.append(stops);
}

/* ---------------------------------------------------------------- wiring */

code.addEventListener("input", () => {
  const doc = current();
  if (!doc) return;
  doc.text = code.value;
  doc.revision++;
  doc.index = buildIndex(doc.text);
  doc.stepAt = null;
  render();
  renderTabs();
  scheduleAnalysis();
});
code.addEventListener("scroll", syncScroll);
code.addEventListener("keyup", updateCursor);
code.addEventListener("click", updateCursor);
code.addEventListener("keydown", (e) => {
  if (e.key === "Tab") {
    e.preventDefault();
    const at = code.selectionStart;
    code.setRangeText("    ", at, code.selectionEnd, "end");
    code.dispatchEvent(new Event("input"));
  }
});

document.addEventListener("keydown", (e) => {
  const meta = e.ctrlKey || e.metaKey;
  if (meta && e.key === "s") { e.preventDefault(); save(); }
  else if (meta && e.key === "r") { e.preventDefault(); reload(); }
  else if (e.key === "F12") { e.preventDefault(); goToDefinition(); }
  else if (e.shiftKey && e.key === "F12") { e.preventDefault(); findReferences(); }
  else if (meta && e.shiftKey && e.key === "F") { e.preventDefault(); findReferences(); }
  else if (e.key === "Escape") closeModal();
});

/* Ctrl+click follows a reference, the way every editor does it. */
code.addEventListener("click", (e) => {
  if (e.ctrlKey || e.metaKey) { e.preventDefault(); goToDefinition(); }
});

$("#act-check").onclick = async () => {
  const doc = current();
  if (!doc) return;
  doc.report = await api("POST", "/api/check", { id: doc.id }, doc.text);
  doc.navigation = null;
  renderDiagnostics(doc); renderStructure(doc); render();
  showView("diagnostics");
};
$("#act-inspect").onclick = async () => { await runAnalysis(); showView("structure"); };
$("#act-run").onclick = startRun;
$("#act-save").onclick = () => save();
$("#act-reload").onclick = reload;
$("#act-new").onclick = newDocument;

function showView(name) {
  for (const t of document.querySelectorAll(".tabstrip .tab")) {
    t.classList.toggle("active", t.dataset.view === name);
  }
  for (const v of document.querySelectorAll(".view")) {
    v.classList.toggle("active", v.dataset.view === name);
  }
}

for (const tab of document.querySelectorAll(".tabstrip .tab")) {
  tab.onclick = () => {
    for (const t of document.querySelectorAll(".tabstrip .tab")) t.classList.remove("active");
    for (const v of document.querySelectorAll(".view")) v.classList.remove("active");
    tab.classList.add("active");
    document.querySelector(`.view[data-view="${tab.dataset.view}"]`).classList.add("active");
  };
}

/* Analysis runs after typing stops, not on every keystroke: a document is
 * carried through five stages, and doing that per character would be work
 * nobody sees the result of. */
let analysisTimer = null;
function scheduleAnalysis() {
  clearTimeout(analysisTimer);
  analysisTimer = setTimeout(() => runAnalysis(), 320);
}

/* ----------------------------------------------------------------- boot */

(async function boot() {
  try {
    await loadSession();
    await loadTree();
    /* What to show first, most specific wins. A document this launch was
     * opened for -- a desktop file association passes one -- then the
     * manifest's declared entry, then whatever the project holds. */
    const launched = state.session.open;
    const entry = state.session.entry;
    const first = state.entries.find((e) => !e.directory);
    const open = launched || entry || (first && first.id);
    if (open) { await openDocument(open); await runAnalysis(); }
    renderCapabilities();
    $("#hint").textContent = "Ctrl+S save · F12 definition · Shift+F12 references";
  } catch (e) {
    document.body.innerHTML =
      `<div style="padding:40px;font:14px system-ui;color:#e0645f">` +
      `Could not start: ${e.message}</div>`;
  }
})();
