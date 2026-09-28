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
    const error = new Error(detail);
    error.status = reply.status;
    error.reply = parsed;
    throw error;
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

/* ------------------------------------------------------- carrying spans */

/* One edit between two texts, as the smallest range that changed.
 *
 * Bytes [from, oldTo) of `before` became bytes [from, newTo) of `after`, and
 * the text ahead of `from` and from `oldTo` on is the same in both. `lines` is
 * how many line feeds the edit added, negative when it removed some. Found by
 * comparing the two texts, so it does not matter how the edit was made. */
function editBetween(before, beforeIndex, after, afterIndex) {
  const shorter = Math.min(before.length, after.length);
  let head = 0;
  while (head < shorter && before.charCodeAt(head) === after.charCodeAt(head)) head++;
  let tail = 0;
  while (tail < shorter - head &&
         before.charCodeAt(before.length - 1 - tail) === after.charCodeAt(after.length - 1 - tail)) {
    tail++;
  }
  /* A character outside the Basic Multilingual Plane is two UTF-16 units. A
   * byte offset taken inside a pair is the pair's first byte, so the head is
   * safe; a tail that starts on the second unit of a changed pair would leave
   * that pair out of the edit, so it gives the unit back. */
  const second = after.charCodeAt(after.length - tail);
  if (tail > 0 && second >= 0xdc00 && second <= 0xdfff) tail--;
  const feeds = (text, from, to) => {
    let n = 0;
    for (let i = from; i < to; i++) if (text.charCodeAt(i) === 10) n++;
    return n;
  };
  return {
    from: beforeIndex.byte(head),
    oldTo: beforeIndex.byte(before.length - tail),
    newTo: afterIndex.byte(after.length - tail),
    lines: feeds(after, head, after.length - tail) - feeds(before, head, before.length - tail),
  };
}

/* Carry byte spans across one edit, deciding nothing about them.
 *
 * A span wholly ahead of the edit stays; a span wholly after it moves with
 * its text, and a line it names moves by the line feeds the edit added or
 * removed; a span the edit touched is dropped, and what it covered is drawn
 * plain until the engine describes the new text. Every span that survives
 * still covers exactly the characters the engine described. */
function carry(spans, edit) {
  if (!spans) return spans;
  const shift = edit.newTo - edit.oldTo;
  const kept = [];
  for (const span of spans) {
    if (span.start < edit.from && span.end <= edit.from) kept.push(span);
    else if (span.start >= edit.oldTo) {
      const moved = { ...span, start: span.start + shift, end: span.end + shift };
      if (typeof span.line === "number") moved.line = span.line + edit.lines;
      kept.push(moved);
    }
  }
  return kept;
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
  settings: null,         // workspace preferences; see `applySettings`
  /* The settings that belong to the computer rather than this browser: the
   * default file type and the default workspace. The server keeps them in the
   * user's configuration directory; see `loadFileSettings`. */
  files: { available: false, default_extension: ".lcl", default_workspace: null },
  /* The file roles this workspace's Core 0.3.0 engine defines; see loadRoles. */
  roles: { available: false, roles: [], modes: [] },
  /* The project whose readiness the sidebar shows; see refreshReadiness. */
  readiness: { entry: null, status: null, report: null, generation: 0 },
};

/* How a document came to be open, which is a different question from whether
 * it has unsaved edits (`dirty`).
 *
 *   "opened"   read from a file that was already there;
 *   "created"  made by + in this page, and not saved by anyone since, so the
 *              file on disk is only this page's placeholder for it;
 *   "saved"    saved explicitly at least once in this page.
 *
 * Only a "created" document's file is deleted when its edits are discarded:
 * nobody chose to keep it. Every other discard leaves the file exactly as it
 * is on disk. The state is never inferred from a name or from `dirty`. */
function Doc(id, text, digest, lifecycle = "opened") {
  return {
    id, text, digest, lifecycle,
    saved: text,
    revision: 0,
    pendingSaves: 0,
    saveTail: Promise.resolve(),
    index: buildIndex(text),
    /* Per-kind request generations. `revision` says whether the root text
     * changed; it cannot say which of two requests made at one revision is
     * the later, and an analysis reads more than the root — `inspect`
     * resolves imports from disk — so two answers at one revision can
     * describe different inputs. `issued` counts requests out, `accepted`
     * remembers the newest answer applied, and an older one is not. */
    issued: { tokens: 0, analysis: 0 },
    accepted: { tokens: 0, analysis: 0 },
    tokens: null,        // token spans from the engine, carried across edits
    report: null,        // the last engine report for this document
    navigation: null,
    /* What the editor draws from `report` and `navigation`: squiggles and
     * reference marks, as byte spans. Kept apart from the report, which is
     * the engine's record and is shown as it arrived, because these move with
     * the text when it is edited (see `carry`). */
    marks: { squiggles: [], refs: [] },
    breakpoints: new Set(),
    stepAt: null,
    /* The engine's marks for a file of a project role: its empty slots, its
     * guidance and its generated IDs (see refreshSlots). Editor metadata only;
     * the text itself is never changed by them. */
    slots: [],
  };
}

const current = () => (state.active ? state.docs.get(state.active) : null);
const dirty = (doc) => doc && doc.text !== doc.saved;

/* Replace a document's text, as one edit.
 *
 * How typing and a reload change a document: the index is rebuilt, the
 * revision moves on, and the spans already drawn move with the text they
 * describe instead of staying at byte offsets that now hold other characters.
 * No answer about the old text is applied to the new one; its spans only
 * move, and the next answer replaces them. */
function replaceText(doc, text) {
  const index = buildIndex(text);
  const edit = editBetween(doc.text, doc.index, text, index);
  doc.tokens = carry(doc.tokens, edit);
  doc.marks = { squiggles: carry(doc.marks.squiggles, edit), refs: carry(doc.marks.refs, edit) };
  doc.text = text;
  doc.index = index;
  doc.revision++;
}

/* Take the editor's marks from the report and resolver data `doc` now holds. */
function markReport(doc) {
  const squiggles = [];
  for (const d of (doc.report && doc.report.diagnostics) || []) {
    if (d.source !== doc.id) continue;
    squiggles.push({
      start: d.span.start, end: d.span.end, severity: severityOf(d), line: d.position.line,
    });
  }
  const refs = [];
  for (const r of (doc.navigation && doc.navigation.references) || []) {
    if (r.source === doc.id) refs.push({ start: r.span.start, end: r.span.end });
  }
  doc.marks = { squiggles, refs };
}

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
const emptyState = $("#empty-state");

/* Every control that acts on the open document. With no document they are
 * disabled together, by `syncDocumentUI`, and by nothing else. */
const DOCUMENT_ACTIONS = ["#act-check", "#act-validate", "#act-inspect", "#act-run", "#act-save", "#act-reload"];

function toast(message, kind = "") {
  const node = el("div", `toast ${kind}`, message);
  $("#toasts").append(node);
  setTimeout(() => node.remove(), kind === "bad" ? 8000 : 4000);
}

/* Where focus returns when the open modal closes. */
let modalOpener = null;

function modal(title, build, actions) {
  /* Recorded once: a modal that replaces an open one keeps the first one's. */
  if ($("#modal-backdrop").hidden) modalOpener = document.activeElement;
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
  /* The page behind takes no focus and no clicks while a modal is open, so
   * Tab stays in the dialog and nothing reaches a document behind it. */
  $("#shell").inert = true;
  const first = body.querySelector("input, select, textarea") || bar.querySelector("button");
  if (first) first.focus();
}
function closeModal() {
  if ($("#modal-backdrop").hidden) return;
  $("#modal-backdrop").hidden = true;
  $("#shell").inert = false;
  /* Back to whatever opened it, if that can still take focus. */
  const opener = modalOpener;
  modalOpener = null;
  if (opener && opener.isConnected && !opener.disabled && typeof opener.focus === "function") {
    opener.focus();
  }
}

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

/* The open documents holding anything not yet kept on disk: edits, a save
 * still on its way, or a new file nobody has saved. */
function unsavedDocuments() {
  return [...state.docs.values()].filter((d) => dirty(d) || d.pendingSaves || d.lifecycle === "created");
}

/* After the server opened another folder in this window: every tab named a
 * document of the folder before, so all of them close (the caller made sure
 * none was unsaved), and the header and tree show the folder now open. */
async function showOpenedFolder() {
  for (const doc of [...state.docs.values()]) dropDocument(doc);
  await loadSession();
  await loadTree();
}

function renderTree() {
  const list = $("#tree");
  list.replaceChildren();
  /* One document in the tree takes Tab focus, the open one or else the first;
   * the arrow keys move between the rest. */
  const files = state.entries.filter((e) => !e.directory);
  const focusable = (files.find((e) => e.id === state.active) || files[0] || {}).id;
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
    if (entry.kind) {
      const role = el("span", `role${entry.kind === "kind.project" ? " entry" : ""}`, roleLabel(entry.kind));
      role.title = `This file declares SPECIFICATION KIND ${entry.kind}`;
      item.append(role);
    }
    item.title = entry.id;
    if (entry.id === state.active) item.classList.add("open");
    if (!entry.directory) {
      item.dataset.id = entry.id;
      item.tabIndex = entry.id === focusable ? 0 : -1;
      item.onclick = () => openDocument(entry.id);
      item.oncontextmenu = (e) => {
        e.preventDefault();
        openMenu(entry.id, e.clientX, e.clientY, item);
      };
      item.onkeydown = (e) => treeKey(e, entry.id, item);
    }
    list.append(item);
  }
}

/* ---------------------------------------------------------- tree actions */

/* The keys a focused document in the tree answers to. Delete only ever asks. */
function treeKey(e, id, item) {
  if (e.key === "Enter") { e.preventDefault(); openDocument(id); }
  else if (e.key === "Delete") { e.preventDefault(); deleteDocument(id); }
  else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
    e.preventDefault();
    const r = item.getBoundingClientRect();
    openMenu(id, r.left + 16, r.bottom, item);
  } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const items = [...document.querySelectorAll("#tree li[data-id]")];
    const next = items[items.indexOf(item) + (e.key === "ArrowDown" ? 1 : -1)];
    if (next) {
      item.tabIndex = -1;
      next.tabIndex = 0;
      next.focus();
    }
  }
}

/* The menu a document in the tree opens: one at a time, closed by choosing,
 * by Escape, by clicking elsewhere or by focus leaving it. */
let treeMenu = null;

function closeMenu() {
  if (!treeMenu) return;
  const menu = treeMenu;
  treeMenu = null;
  menu.remove();
}

function openMenu(id, x, y, returnTo) {
  closeMenu();
  const menu = el("div", "context-menu");
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", id);
  const actions = [["Open", () => openDocument(id)], ["Delete…", () => deleteDocument(id)]];
  const listed = state.entries.find((e) => e.id === id);
  if (listed && listed.kind === "kind.task" && state.roles.available) {
    actions.splice(1, 0, ["Convert to multi-file project…", () => convertDocument(id)]);
  }
  for (const [label, act] of actions) {
    const item = el("button", "", label);
    item.setAttribute("role", "menuitem");
    item.onclick = () => { closeMenu(); act(); };
    menu.append(item);
  }
  menu.addEventListener("keydown", (e) => {
    const items = [...menu.querySelectorAll("button")];
    const at = items.indexOf(document.activeElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : items.length - 1;
      items[(at + step) % items.length].focus();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeMenu();
      if (returnTo) returnTo.focus();
    }
  });
  menu.addEventListener("focusout", (e) => {
    if (!menu.contains(e.relatedTarget)) closeMenu();
  });
  document.body.append(menu);
  // Kept inside the window, wherever the click was.
  const box = menu.getBoundingClientRect();
  const view = document.documentElement;
  menu.style.left = `${Math.max(0, Math.min(x, view.clientWidth - box.width - 4))}px`;
  menu.style.top = `${Math.max(0, Math.min(y, view.clientHeight - box.height - 4))}px`;
  treeMenu = menu;
  menu.querySelector("button").focus();
}

document.addEventListener("mousedown", (e) => {
  if (treeMenu && !treeMenu.contains(e.target)) closeMenu();
});

/* Delete one document from the project, after asking.
 *
 * The file is read first, so the question is about what is on disk now, and
 * the deletion names that content's digest: if the file changes or is
 * replaced while the question is open, the server leaves it alone and says
 * so. An open document goes with it, unsaved edits and all, and the dialog
 * says that too. */
async function deleteDocument(id) {
  let shown;
  try {
    shown = await api("GET", "/api/document", { id });
  } catch (e) {
    toast(`${id} could not be read, so it was not deleted. ${e.message}`, "bad");
    try { await loadTree(); } catch (_) { /* the listing says what is there */ }
    return;
  }
  const doc = state.docs.get(id);
  const name = id.split("/").pop();
  modal(`Delete "${name}"?`, (body) => {
    body.append(el("p", "", "This permanently removes the file from the project."));
    if (doc && (dirty(doc) || doc.pendingSaves)) {
      body.append(el("p", "warning", "It is open with unsaved edits, and they are lost too."));
    }
  }, [
    ["Cancel", "", (close) => close()],
    ["Delete", "danger", async (close) => {
      close();
      try {
        await api("DELETE", "/api/document", { id, digest: shown.digest });
      } catch (e) {
        toast(`${id} was not deleted. ${e.message}`, "bad");
        try { await loadTree(); } catch (_) { /* as above */ }
        return;
      }
      const open = state.docs.get(id);
      if (open) dropDocument(open);
      try { await loadTree(); } catch (_) { /* as above */ }
      toast(`Deleted ${id}`, "good");
    }],
  ]);
}

/* ------------------------------------------------------------ documents */

/* Open one document. `created` is the digest the create route reported, when
 * this page has just created it: the document counts as newly created only if
 * the file still holds exactly those bytes when it is read. */
async function openDocument(id, { focusByte, created } = {}) {
  if (!state.docs.has(id)) {
    const reply = await api("GET", "/api/document", { id });
    const lifecycle = created !== undefined && reply.digest === created ? "created" : "opened";
    state.docs.set(id, Doc(reply.id, reply.text, reply.digest, lifecycle));
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
  refreshReadiness();
  refreshSlots(doc, doc.revision).then(() => { if (current() === doc) render(); });
}

/* Take one document out of the editor: its tab and its buffer. When it was
 * the one on screen, the last remaining tab takes its place, or the empty
 * state does. Nothing on disk is touched here. */
function dropDocument(doc) {
  if (state.docs.get(doc.id) !== doc) return;
  state.docs.delete(doc.id);
  state.order = state.order.filter((x) => x !== doc.id);
  if (state.active === doc.id) {
    state.active = state.order[state.order.length - 1] || null;
    if (state.active) code.value = state.docs.get(state.active).text;
  }
  renderTabs(); renderTree(); render();
}

/* Discard a document this page created and nobody has saved: its file goes
 * too, because nobody chose to keep it.
 *
 * A save already on its way decides first. If one lands, the document is an
 * ordinary saved one, and discarding only drops the edits made since. The
 * deletion names the bytes this page created, so a file that anything else
 * has written since is kept, and the reply says so. */
async function discardNew(doc) {
  await doc.saveTail;
  if (state.docs.get(doc.id) !== doc) return;
  if (doc.lifecycle === "created") {
    try {
      await api("DELETE", "/api/document", { id: doc.id, digest: doc.digest });
      toast(`Discarded ${doc.id}. Its file was removed.`);
    } catch (e) {
      toast(`${doc.id} was kept on disk. ${e.message}`, "warn");
    }
  }
  dropDocument(doc);
  try { await loadTree(); } catch (_) { /* the tree refreshes on the next listing */ }
}

function closeDocument(id) {
  const doc = state.docs.get(id);
  if (!doc) return;
  const drop = () => dropDocument(doc);
  if (doc.lifecycle === "created") {
    /* Whether or not it was edited: until someone saves it, a new document's
     * file is only a placeholder, and closing it is the moment to decide. */
    modal("Unsaved new document", (body) => {
      body.append(el("p", "", `${id.split("/").pop()} has not been saved.`));
      body.append(el("p", "note",
        "Discard removes the file that was created for it. Save keeps it as an ordinary document."));
    }, [
      ["Cancel", "", (close) => close()],
      ["Discard", "", (close) => { close(); return discardNew(doc); }],
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
    /* A save queued behind a conflict waits for the person's choice rather
     * than raising the same conflict again. */
    if (doc.conflict) {
      toast(`Not saved. ${doc.id} changed on disk; choose Reload or Keep mine first.`, "warn");
      return false;
    }
    let reply, persisted;
    try {
      /* The revision this edit started from: the digest the last load or the
       * last acknowledged save gave. A queued save runs after the one before
       * it was acknowledged, so it bases itself on that save, never on a
       * digest its own queue already replaced. */
      reply = await api("PUT", "/api/document", { id: doc.id, base: doc.digest }, submitted);
      if (!reply || reply.id !== doc.id || typeof reply.digest !== "string" ||
          typeof reply.final_line_feed_added !== "boolean") {
        throw new Error("The server did not acknowledge this document's save.");
      }
      persisted = submitted + (reply.final_line_feed_added ? "\n" : "");
      if (reply.bytes !== buildIndex(persisted).byteLength) {
        throw new Error("The server did not acknowledge the submitted content length.");
      }
    } catch (e) {
      if (e.status === 409 && e.reply && e.reply.conflict) {
        doc.conflict = { digest: e.reply.digest, exists: e.reply.exists };
        showConflict(doc);
        return false;
      }
      toast(`Not saved. ${e.message}`, "bad");
      return false;
    }
    // Only acknowledged bytes form the saved baseline. Later input belongs to
    // the next revision and must never be overwritten or marked as saved here.
    doc.saved = persisted;
    doc.digest = reply.digest;
    // Saved on purpose: from now on it is an ordinary document, and a discard
    // drops only edits made after this.
    doc.lifecycle = "saved";
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
      refreshReadiness();
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
    replaceText(doc, reply.text);
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

/* New document: blank, or a file of one role.
 *
 * A role's text is the engine's scaffold or a Master (see Settings →
 * Templates), chosen and written by the server; this dialog only shows the
 * exact text first. Nothing is created until Create is pressed. */
function newDocument() {
  const ending = state.files.default_extension;
  /* The roles were read at start; the Masters are read while the dialog is
   * already open, and fill in its choices when they arrive. */
  const kinds = [["", "Blank LCL file"]].concat(state.roles.roles.map((r) => [r.role, r.label]));
  let masters = [];
  let mastersArrived = () => {};
  /* The digest of the exact text shown in the preview. A file of a role is
   * created only from that text: the server refuses (409) when the template,
   * default or name changed since, and (428) when nothing was previewed. */
  let previewed = null;
  api("GET", "/api/masters").then((reply) => {
    masters = reply.masters || [];
    mastersArrived();
  }, () => { /* no Masters: the canonical scaffolds remain */ });
  modal("New document", (body) => {
    body.append(el("p", "",
      `A path inside the project. A name without an ending is created as ${ending}, ` +
      "the default file type in Settings. Type .lcl or .lcl.txt yourself to choose " +
      "either one: the ending you type is kept, and existing documents keep their name."));
    const input = el("input", "field");
    input.id = "new-path";
    input.value = `untitled${ending}`;
    const kindLabel = el("label", "", "Kind of file");
    kindLabel.htmlFor = "new-kind";
    const kind = el("select", "field");
    kind.id = "new-kind";
    for (const [value, label] of kinds) {
      const option = el("option", "", label);
      option.value = value;
      kind.append(option);
    }
    const fromLabel = el("label", "", "Start from");
    fromLabel.htmlFor = "new-from";
    const from = el("select", "field");
    from.id = "new-from";
    const preview = el("pre", "scaffold-preview");
    preview.id = "new-preview";
    const note = el("p", "note");
    const refill = () => {
      from.replaceChildren();
      const role = kind.value;
      from.disabled = !role;
      if (!role) return;
      const options = [
        ["default:guided", "Default (Guided)"], ["default:minimal", "Default (Minimal)"],
        ["canonical:guided", "Canonical scaffold — Guided"], ["canonical:minimal", "Canonical scaffold — Minimal"],
      ].concat(masters.filter((m) => (m.type || m.role) === role && m.valid).map((m) => [`master:${m.id}`, `Master: ${m.name}`]));
      for (const [value, label] of options) {
        const option = el("option", "", label);
        option.value = value;
        from.append(option);
      }
    };
    let shown = 0;
    const show = async () => {
      const mine = ++shown;
      previewed = null;
      preview.replaceChildren();
      note.textContent = "";
      if (!kind.value) {
        note.textContent = "A blank file holds only the LCL and SPECIFICATION headers.";
        return;
      }
      const name = input.value.trim() || "untitled";
      const path = /\.lcl(\.txt)?$/.test(name) ? name : name + ending;
      try {
        const scaffold = await api("GET", "/api/scaffold",
          { path, role: kind.value, ...selectionParams(kind.value, from.value) });
        if (mine !== shown) return;
        previewed = scaffold.digest;
        showScaffold(preview, scaffold);
        note.textContent = "Exactly this text will be written. Marked lines: ! required slot, " +
          "? optional slot, # guidance, ⚙ generated identifier.";
      } catch (e) {
        if (mine === shown) note.textContent = `No preview: ${e.message}`;
      }
    };
    mastersArrived = () => {
      if (!$("#new-from") || $("#new-from") !== from) return;
      const chosen = from.value;
      refill();
      if ([...from.children].some((o) => o.value === chosen)) from.value = chosen;
    };
    kind.onchange = () => { refill(); show(); };
    from.onchange = show;
    input.oninput = () => { clearTimeout(input.timer); input.timer = setTimeout(show, 250); };
    body.append(input, kindLabel, kind, fromLabel, from, note, preview);
    refill();
    show();
  }, [
    ["Cancel", "", (close) => close()],
    ["Create", "primary", async (close) => {
      const id = $("#new-path").value.trim();
      const role = $("#new-kind").value;
      const from = $("#new-from").value;
      close();
      if (!id) return;
      try {
        let created;
        if (role) {
          if (!previewed) throw new Error("wait for the preview: a file of a role is created only from the text shown.");
          created = await api("POST", "/api/document",
            { id, role, ...selectionParams(role, from), scaffold_digest: previewed });
        } else {
          /* A blank document is the smallest thing the grammar accepts.
           * 04_GRAMMAR/01: "Every document starts with LCL then SPECIFICATION."
           * The values are placeholders; the engine judges them like any other. */
          const seed =
            'LCL:\n    VERSION: "0.1.0"\n\n' +
            'SPECIFICATION:\n    ID: example.new\n    NAME: "New document"\n' +
            '    VERSION: "1.0.0"\n    KIND: kind.task\n    DOMAIN: "general"\n';
          created = await api("POST", "/api/document", { id }, seed);
        }
        /* Creating is its own route: it applies the .lcl default and
         * refuses to overwrite. Saving stays exact, so an open document is
         * never renamed under the person editing it. The server decides the
         * final name, and the reply says what it chose. */
        await loadTree();
        await openDocument(created.id, { created: created.digest });
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

/* ------------------------------------------------------------- settings */

/* Workspace preferences.
 *
 * Presentation only, and kept in this browser's local storage. They never
 * enter a document, a project manifest or a request to the engine, so no
 * setting here can change what a document means or how it runs.
 *
 * The stored object carries a version, so a later release can migrate it.
 * Anything unreadable, from another version, or out of range falls back to
 * the default for that field rather than failing the page. */
const SETTINGS_KEY = "lcl.workspace.settings";
const SETTINGS_VERSION = 1;
const THEMES = ["system", "dark", "light"];
const FONT_MIN = 11;
const FONT_MAX = 20;
const DEFAULT_SETTINGS = Object.freeze({
  version: SETTINGS_VERSION, theme: "system", fontSize: 13, lineNumbers: true,
});

function storage() {
  /* Absent, or refused by the browser's privacy settings: either way the
   * page works with defaults and simply remembers nothing. */
  try { return globalThis.localStorage || null; } catch (_) { return null; }
}

function validSettings(raw) {
  const s = { ...DEFAULT_SETTINGS };
  if (!raw || typeof raw !== "object" || raw.version !== SETTINGS_VERSION) return s;
  if (THEMES.includes(raw.theme)) s.theme = raw.theme;
  if (Number.isInteger(raw.fontSize) && raw.fontSize >= FONT_MIN && raw.fontSize <= FONT_MAX) {
    s.fontSize = raw.fontSize;
  }
  if (typeof raw.lineNumbers === "boolean") s.lineNumbers = raw.lineNumbers;
  return s;
}

function loadSettings() {
  const store = storage();
  if (!store) return { ...DEFAULT_SETTINGS };
  try {
    const text = store.getItem(SETTINGS_KEY);
    return validSettings(text ? JSON.parse(text) : null);
  } catch (_) {
    return { ...DEFAULT_SETTINGS };
  }
}

function saveSettings(settings) {
  const store = storage();
  if (!store) return false;
  try {
    store.setItem(SETTINGS_KEY, JSON.stringify(settings));
    return true;
  } catch (_) {
    return false;
  }
}

/* Apply preferences to the page.
 *
 * Theme narrows `color-scheme` through `data-theme` ("system" removes it and
 * the operating system decides). Font size sets the editor's type size and its
 * row height together, which the source, the paint layer and the gutter all
 * read, so their lines stay aligned at every size. */
function applySettings(settings) {
  state.settings = settings;
  const root = document.documentElement;
  if (settings.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = settings.theme;
  root.style.setProperty("--editor-font", `${settings.fontSize}px`);
  root.style.setProperty("--row", `${Math.round(settings.fontSize * 20 / 13)}px`);
  root.classList.toggle("no-gutter", !settings.lineNumbers);
  syncScroll();
}

/* The computer's settings, as the server holds them. A page that cannot read
 * them keeps the defaults, and Settings says they are unavailable. */
async function loadFileSettings() {
  try {
    state.files = await api("GET", "/api/settings");
  } catch (_) {
    state.files = { available: false, default_extension: ".lcl", default_workspace: null };
  }
}

/* What one folder path is, for the Settings dialog: said in `status`, with a
 * button to create it when it does not exist. Creating happens only when
 * that button, which names the exact path, is pressed. */
async function describeFolder(path, status) {
  if (!path) {
    status.replaceChildren(el("span", "", "Empty: new projects go in, and launches open, the built-in folder" +
      (state.files.builtin_default_workspace ? ` ${state.files.builtin_default_workspace}.` : ".")));
    return true;
  }
  let folder;
  try {
    folder = await api("GET", "/api/folder", { path });
  } catch (e) {
    status.replaceChildren(el("span", "bad", e.message));
    return false;
  }
  if (!folder.absolute) {
    status.replaceChildren(el("span", "bad",
      `${path} is not an absolute path. Write it from /, for example /home/you/LCL.`));
    return false;
  }
  if (folder.exists && !folder.directory) {
    status.replaceChildren(el("span", "bad", `${path} is a file, not a folder.`));
    return false;
  }
  if (folder.directory && folder.writable === false) {
    status.replaceChildren(el("span", "bad",
      `${path} is a folder, but new projects cannot be written to it.`));
    return false;
  }
  if (!folder.exists) {
    const create = el("button", "", "Create this folder");
    create.type = "button";
    create.onclick = async () => {
      try {
        await api("POST", "/api/folder", { path });
      } catch (e) {
        status.replaceChildren(el("span", "bad", `Not created. ${e.message}`));
        return;
      }
      status.replaceChildren(el("span", "good", `Created ${path}. Save to use it.`));
    };
    status.replaceChildren(el("span", "bad", `${path} does not exist. `), create);
    return false;
  }
  status.replaceChildren(el("span", "good", `${path} is a folder new projects can be created in.`));
  return true;
}

function openSettings() {
  const now = state.settings || { ...DEFAULT_SETTINGS };
  const files = state.files;
  modal("Settings", (body) => {
    const form = el("div", "settings");

    form.append(el("h3", "", "Appearance"));
    const themeLabel = el("label", "", "Theme");
    themeLabel.htmlFor = "setting-theme";
    const theme = el("select");
    theme.id = "setting-theme";
    for (const [value, label] of [["system", "System"], ["dark", "Dark"], ["light", "Light"]]) {
      const option = el("option", "", label);
      option.value = value;
      theme.append(option);
    }
    theme.value = now.theme;
    form.append(themeLabel, theme);

    form.append(el("h3", "", "Editor"));
    const sizeLabel = el("label", "", "Font size (px)");
    sizeLabel.htmlFor = "setting-font-size";
    const size = el("input");
    size.id = "setting-font-size";
    size.type = "number";
    size.min = String(FONT_MIN);
    size.max = String(FONT_MAX);
    size.step = "1";
    size.value = String(now.fontSize);
    form.append(sizeLabel, size);

    const numbersLabel = el("label", "", "Show line numbers");
    numbersLabel.htmlFor = "setting-line-numbers";
    const numbers = el("input");
    numbers.id = "setting-line-numbers";
    numbers.type = "checkbox";
    numbers.checked = now.lineNumbers;
    form.append(numbersLabel, numbers);

    form.append(el("p", "note",
      `Appearance and Editor are saved in this browser only. Font size is ${FONT_MIN} to ` +
      `${FONT_MAX} px. Documents are always indented with spaces; Tab inserts four.`));

    /* Files: kept by the workspace for this computer, not by the browser. */
    form.append(el("h3", "", "Files"));
    const typeLabel = el("label", "", "Default file type");
    typeLabel.htmlFor = "setting-file-type";
    const type = el("select");
    type.id = "setting-file-type";
    for (const [value, label] of [[".lcl", "LCL (.lcl)"], [".lcl.txt", "LCL Text (.lcl.txt)"]]) {
      const option = el("option", "", label);
      option.value = value;
      type.append(option);
    }
    type.value = files.default_extension;
    form.append(typeLabel, type);

    const whereLabel = el("label", "", "Projects folder");
    whereLabel.htmlFor = "setting-workspace";
    const whereRow = el("div", "path-field");
    const where = el("input");
    where.id = "setting-workspace";
    where.type = "text";
    where.spellcheck = false;
    where.value = files.default_workspace || "";
    where.placeholder = files.builtin_default_workspace
      ? `Empty: the built-in folder, ${files.builtin_default_workspace}`
      : "Empty: the built-in folder";
    const check = el("button", "", "Check");
    check.type = "button";
    const openHere = el("button", "", "Open folder");
    openHere.type = "button";
    openHere.id = "setting-workspace-open";
    whereRow.append(where, check, openHere);
    form.append(whereLabel, whereRow);
    const status = el("p", "note folder-status");
    status.id = "setting-workspace-status";
    check.onclick = () => describeFolder(where.value.trim(), status);
    /* Shows the saved Projects folder in this window, in place of the folder
     * shown now; every tab closes, so none may hold unsaved work. */
    openHere.onclick = async () => {
      if (where.value.trim() !== (files.default_workspace || "")) {
        status.replaceChildren(el("span", "bad", "Save first: Open folder opens the saved Projects folder."));
        return;
      }
      if (unsavedDocuments().length) {
        status.replaceChildren(el("span", "bad",
          "Save or close the open documents first: this window will show the Projects folder instead."));
        return;
      }
      try {
        await api("POST", "/api/projects/open");
      } catch (e) {
        status.replaceChildren(el("span", "bad", e.message));
        return;
      }
      closeModal();
      await showOpenedFolder();
      toast(`This window now shows ${state.session.root}.`, "good");
    };
    if (files.default_workspace && files.default_workspace_exists === false) {
      status.append(el("span", "bad",
        "This folder does not exist now: New Project refuses to create projects until it does, " +
        "and a launch opens the built-in folder instead."));
    }
    form.append(status);
    form.append(el("p", "note",
      `This window shows ${files.current_workspace || (state.session && state.session.root) || ""}. ` +
      "New Project creates every new project in the Projects folder, each in a folder of its " +
      "own, and LCL Workspace opens the Projects folder when it starts from the desktop menu; " +
      "a folder or document opened explicitly still wins. Leave it empty for the built-in " +
      "folder. The default file type is the ending of a new document named without one and " +
      "of every file a new project starts with."));
    if (files.problem) form.append(el("p", "note warning", files.problem));
    if (!files.available) {
      type.disabled = where.disabled = check.disabled = true;
      form.append(el("p", "note warning",
        "These cannot be saved: this workspace has no configuration folder (HOME or " +
        "XDG_CONFIG_HOME is not set)."));
    }
    form.append(el("h3", "", "Templates"));
    form.append(el("p", "note",
      "Master templates: your own starting text for each kind of file and for new projects."));
    const templates = el("button", "", "Templates…");
    templates.type = "button";
    templates.id = "open-templates";
    templates.onclick = () => openTemplates();
    form.append(templates);
    updatesSection(form);
    androidDevices(form);
    body.append(form);
  }, [
    ["Cancel", "", (close) => close()],
    /* Fills in the defaults and leaves the choice to Save or Cancel: one of
     * these sections decides what every future launch opens. */
    ["Reset to defaults", "", () => {
      $("#setting-theme").value = DEFAULT_SETTINGS.theme;
      $("#setting-font-size").value = String(DEFAULT_SETTINGS.fontSize);
      $("#setting-line-numbers").checked = DEFAULT_SETTINGS.lineNumbers;
      if (files.available) {
        $("#setting-file-type").value = ".lcl";
        $("#setting-workspace").value = "";
      }
    }],
    ["Save", "primary", async (close) => {
      /* A number field reports text it cannot read as "", which Number()
       * would take for 0 and clamp to the minimum. Unreadable is unreadable. */
      const typed = $("#setting-font-size").value.trim();
      const requested = typed === "" ? NaN : Number(typed);
      const clamped = Math.min(FONT_MAX, Math.max(FONT_MIN,
        Number.isFinite(requested) ? Math.round(requested) : DEFAULT_SETTINGS.fontSize));
      const next = validSettings({
        version: SETTINGS_VERSION,
        theme: $("#setting-theme").value,
        fontSize: clamped,
        lineNumbers: Boolean($("#setting-line-numbers").checked),
      });

      /* The computer's settings first: a folder that is not there keeps the
       * dialog open with the reason and, where it fits, a way to create it. */
      const wanted = {
        default_extension: $("#setting-file-type").value,
        default_workspace: $("#setting-workspace").value.trim() || null,
      };
      const typeChanged = wanted.default_extension !== files.default_extension;
      const whereChanged = wanted.default_workspace !== (files.default_workspace || null);
      if (files.available && (typeChanged || whereChanged)) {
        const status = $("#setting-workspace-status");
        if (whereChanged && !(await describeFolder(wanted.default_workspace, status))) return;
        try {
          state.files = await api("PUT", "/api/settings", {}, JSON.stringify(wanted));
        } catch (e) {
          status.replaceChildren(el("span", "bad", `Not saved. ${e.message}`));
          return;
        }
      }

      close();
      applySettings(next);
      if (!saveSettings(next)) toast("Settings apply now but could not be stored in this browser.", "warn");
      if (files.available && whereChanged) {
        toast("Projects folder updated. New projects are created there, and LCL Workspace opens it " +
          "the next time it starts from the menu.", "good");
      }
      if (files.available && typeChanged) {
        toast(`New documents named without an ending are now created as ${wanted.default_extension}.`, "good");
      }
    }],
  ]);
}

/* ---------------------------------------------------------------- updates */

/* What the installed updater last found, or null when this workspace has no
 * updater beside it. Reading it never uses the network. */
async function loadUpdate() {
  try {
    state.update = await api("GET", "/api/update");
  } catch (_) {
    state.update = null;
  }
  markUpdate();
  return state.update;
}

/* A quiet mark on the Settings button while an update waits. It never opens
 * anything and never takes focus from the editor. */
function markUpdate() {
  const s = state.update && state.update.state;
  const waiting = Boolean(s && s.available &&
    (s.state === "update_available" || s.state === "ready_to_install"));
  const button = $("#act-settings");
  button.classList.toggle("has-update", waiting);
  button.title = waiting ? `Settings — LCL ${s.available.version} is available` : "Settings";
}

/* At start, a check only when the last one is a day old. Offline or not, the
 * workspace works exactly the same. */
async function checkUpdatesWhenDue() {
  const u = await loadUpdate();
  if (!u || !u.configured || !u.check_due) return;
  try {
    state.update = await api("POST", "/api/update/check");
  } catch (_) {
    return;
  }
  markUpdate();
}

const UPDATE_STATES = {
  up_to_date: "Up to date",
  checking: "Checking",
  update_available: "Update available",
  downloading: "Downloading",
  ready_to_install: "Ready to install (restart required)",
  installing: "Installing",
  failed: "Failed",
  offline: "Could not check: offline",
  not_configured: "Updates are not set up in this build",
};

const UPDATE_PROBLEMS = {
  offline: "No network",
  invalid: "Invalid update",
  verification: "Verification failed",
  download: "Download failed",
  install: "Installation failed",
  unsupported: "Cannot update automatically",
  not_configured: "Not set up",
  busy: "Busy",
};

function formatSize(bytes) {
  return bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.ceil(bytes / 1024)} KB`;
}

function updatesSection(form) {
  form.append(el("h3", "", "Updates"));
  const box = el("div", "updates");
  box.id = "updates";
  form.append(box);
  renderUpdates(box);
  loadUpdate().then(() => { if (box.isConnected) renderUpdates(box); });
}

function renderUpdates(box) {
  box.replaceChildren();
  const u = state.update;
  if (!u) {
    box.append(el("p", "note", "Updates are not available here: lcl-update is not installed beside this workspace."));
    return;
  }
  const s = u.state || {};
  const when = (secs) => (secs ? new Date(secs * 1000).toLocaleString() : "never");
  box.append(el("p", "", `Installed version: LCL ${u.installed}`));
  box.append(el("p", "", `Last update check: ${when(s.checked_at)}`));
  const shown = el("p", "update-state", `State: ${UPDATE_STATES[s.state] || "Not checked yet"}`);
  shown.id = "update-state";
  box.append(shown);
  if (s.error) {
    box.append(el("p", "note warning", `${UPDATE_PROBLEMS[s.error.kind] || "Problem"}: ${s.error.message}`));
  }
  if (s.available) {
    const a = s.available;
    box.append(el("p", "", `New version: LCL ${a.version} · released ${a.published_at.slice(0, 10)} · ${formatSize(a.size)}`));
    /* Release notes are text to read, never markup. */
    const notes = el("pre", "release-notes");
    notes.textContent = a.release_notes;
    box.append(notes);
  }
  if (s.state === "downloading" && s.progress) {
    const bar = el("progress");
    bar.max = s.progress.total;
    bar.value = s.progress.done;
    box.append(bar);
  }
  const row = el("div", "path-field");
  const check = el("button", "", "Check for updates");
  check.type = "button";
  check.id = "update-check";
  check.disabled = !u.configured || s.state === "downloading" || s.state === "installing";
  check.onclick = async () => {
    check.disabled = true;
    shown.textContent = "State: Checking";
    try {
      state.update = await api("POST", "/api/update/check");
    } catch (e) {
      toast(`Could not check for updates. ${e.message}`, "bad");
    }
    markUpdate();
    renderUpdates(box);
  };
  row.append(check);
  if (s.state === "update_available") {
    const update = el("button", "primary", "Update");
    update.type = "button";
    update.id = "update-download";
    update.onclick = async () => {
      update.disabled = true;
      try {
        await api("POST", "/api/update/download");
      } catch (e) {
        toast(`The update could not be downloaded. ${e.message}`, "bad");
        return;
      }
      followDownload(box);
    };
    row.append(update);
  }
  if (s.state === "ready_to_install") {
    const install = el("button", "primary", "Install and restart");
    install.type = "button";
    install.id = "update-install";
    install.onclick = () => installUpdate(s.available ? s.available.version : "");
    row.append(install);
  }
  box.append(row);
}

/* Follow a download until it is staged or has failed. Only a verified,
 * staged update is ever called ready. */
async function followDownload(box) {
  const started = Date.now();
  for (;;) {
    await new Promise((resolve) => setTimeout(resolve, 700));
    await loadUpdate();
    if (box.isConnected) renderUpdates(box);
    const s = state.update && state.update.state;
    const going = s && (s.state === "downloading" ||
      (s.state === "update_available" && Date.now() - started < 10000));
    if (!going) return;
  }
}

/* Installing closes this window's workspace, so it waits for unsaved work to
 * be saved or closed: nothing is discarded to restart. */
function installUpdate(version) {
  const unsaved = unsavedDocuments();
  modal("Install update", (body) => {
    body.append(el("p", "",
      `LCL Workspace closes, LCL ${version} is installed and checked, and LCL Workspace opens again. ` +
      "If anything fails, the version you have now is put back. Your projects, settings, templates " +
      "and paired devices are not touched."));
    if (unsaved.length) {
      body.append(el("p", "note warning",
        `Save or close ${unsaved.map((d) => d.id).join(", ")} first: installing restarts LCL Workspace.`));
    }
  }, [
    ["Cancel", "", (close) => close()],
    ["Install and restart", "primary", async (close) => {
      if (unsavedDocuments().length) return;
      try {
        await api("POST", "/api/update/install");
      } catch (e) {
        toast(`Not installed. ${e.message}`, "bad");
        return;
      }
      close();
      const note = el("div", null,
        "Installing the update. LCL Workspace opens again in a new window when it is done; this page can be closed.");
      note.style.cssText = "padding:40px;font:14px system-ui";
      document.body.replaceChildren(note);
    }],
  ]);
  if (unsaved.length) [...$("#modal-actions").querySelectorAll("button")].pop().disabled = true;
}

/* ------------------------------------------------------- android devices */

/* LCL for Android pairs with this PC through `lcl-remote`, which keeps its own
 * identity and list of trusted devices. This section only shows what it
 * reports and asks it to pair, approve, deny or revoke; it changes nothing
 * that is saved.
 *
 * A pairing code only lets a phone ask. Its request waits under Pending
 * pairing requests until the person approves it here — after comparing the
 * verification code the phone shows — and approving takes a second,
 * explicit step. Trusted devices are listed apart from requests. */
function androidDevices(form) {
  form.append(el("h3", "", "Android devices"));
  const box = el("div", "remote");
  box.id = "remote-devices";
  const status = el("p", "note", "Looking for lcl-remote…");
  const list = el("div", "remote-list");
  const requests = el("div", "remote-requests");
  requests.id = "remote-pending";
  const pair = el("button", "", "Pair Android device");
  pair.type = "button";
  pair.id = "remote-pair";
  pair.hidden = true;
  const code = el("div", "remote-code");
  box.append(status, list, requests, pair, code);
  form.append(box);

  let devices = [];
  let waiting = [];
  /* The pairing code on screen, and the devices known before it was shown. */
  let shown = null;
  /* The request whose approval is being confirmed, kept across refreshes. */
  let confirming = null;
  let timer = null;
  let watch = null;
  const stop = () => {
    if (timer) { clearInterval(timer); timer = null; }
    if (watch) { clearInterval(watch); watch = null; }
  };
  /* Closed, or replaced by another dialog: nothing more to show or poll. */
  const showing = () => box.isConnected && !$("#modal-backdrop").hidden;
  const when = (seconds) => seconds ? new Date(seconds * 1000).toLocaleString() : "never";
  const remaining = (expires) => {
    const seconds = Math.max(0, Math.round(expires - Date.now() / 1000));
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  };
  const button = (label, onclick) => {
    const node = el("button", "", label);
    node.type = "button";
    node.onclick = onclick;
    return node;
  };

  async function decide(r, decision) {
    confirming = null;
    try {
      await api("POST", `/api/remote/${decision}`, { id: r.request });
      toast(decision === "approve"
        ? `${r.name} is approved. The phone finishes pairing by itself within a few seconds.`
        : `${r.name}'s request is denied. It is not trusted.`, "good");
    } catch (e) {
      toast(`${decision === "approve" ? "Not approved" : "Not denied"}. ${e.message}`, "bad");
    }
    await refresh();
  }

  /* The requests last reported, as rows. Every name and code is text. */
  function showRequests() {
    requests.replaceChildren(el("h4", "", "Pending pairing requests"));
    if (waiting.length === 0) {
      requests.append(el("p", "note", "No pairing request is waiting."));
      return;
    }
    requests.append(el("p", "note",
      "A phone that scanned a pairing code asks to be trusted. Approve only the request whose " +
      "verification code your phone shows; deny any you do not recognise."));
    if (!waiting.some((r) => r.request === confirming)) confirming = null;
    for (const r of waiting) {
      const row = el("div", "remote-request");
      row.dataset.request = r.request;
      row.append(
        el("span", "remote-name", r.name),
        el("span", "remote-verification mono", r.verification),
        el("span", "note mono", `fingerprint ${r.fingerprint}`),
        el("span", "note", r.status === "approved"
          ? "approved; the phone finishes pairing by itself"
          : `asked ${when(r.created)} · expires in ${remaining(r.expires)}`),
      );
      if (r.status === "pending" && confirming === r.request) {
        const confirm = el("div", "remote-confirm");
        confirm.append(
          el("p", "", "Approve this Android device?"),
          el("p", "", `Verification code: ${r.verification}`),
          el("p", "mono", `Fingerprint: ${r.fingerprint}`),
          el("p", "note", "Approve only if the phone shows this same verification code."),
          button("Approve device", () => decide(r, "approve")),
          button("Cancel", () => { confirming = null; showRequests(); }),
        );
        row.append(confirm);
      } else if (r.status === "pending") {
        row.append(
          button("Approve…", () => { confirming = r.request; showRequests(); }),
          button("Deny", () => decide(r, "deny")),
        );
      }
      requests.append(row);
    }
  }

  async function refreshRequests() {
    let reply;
    try {
      reply = await api("GET", "/api/remote/pending");
    } catch (e) {
      requests.replaceChildren(el("h4", "", "Pending pairing requests"),
        el("p", "bad", `Pairing requests are unavailable: ${e.message}`));
      return;
    }
    if (reply.installed === false) { requests.replaceChildren(); return; }
    waiting = reply.requests || [];
    showRequests();
    /* Requests change from the phone's side too; follow them while any wait. */
    if (waiting.length > 0 && !timer && !watch) watch = setInterval(refresh, 2000);
    if (watch && waiting.length === 0) { clearInterval(watch); watch = null; }
  }

  async function refresh() {
    if (!showing()) { stop(); return null; }
    let reply;
    try {
      reply = await api("GET", "/api/remote/devices");
    } catch (e) {
      status.replaceChildren(el("span", "bad", `Android devices are unavailable: ${e.message}`));
      return null;
    }
    if (reply.installed === false) {
      status.replaceChildren(el("span", "", "LCL for Android pairs with this PC through lcl-remote, which is not installed here."));
      return null;
    }
    status.replaceChildren(reply.service_running
      ? el("span", "good", "The Android service is running; paired devices can connect.")
      : el("span", "bad",
        "The Android service is not running, so paired devices cannot connect. Start it with " +
        "`lcl-remote serve`, or `systemctl --user start lcl-remote`."));
    devices = reply.devices || [];
    list.replaceChildren();
    if (devices.length === 0) list.append(el("p", "note", "No Android device is paired."));
    for (const d of devices) {
      const row = el("div", "remote-device");
      row.dataset.id = d.id;
      const state = d.revoked_at ? `revoked ${when(d.revoked_at)}` : (d.online ? "online" : "offline");
      row.append(
        el("span", "remote-name", d.name),
        el("span", d.revoked_at ? "bad" : (d.online ? "good" : "note"), state),
        el("span", "note", `last connected ${when(d.last_seen)} · paired ${when(d.paired_at)}`),
      );
      if (!d.revoked_at) {
        const revoke = el("button", "", "Revoke");
        revoke.type = "button";
        /* Two steps: revoking ends the device's trust until it pairs again. */
        revoke.onclick = async () => {
          if (revoke.dataset.confirm !== "1") {
            revoke.dataset.confirm = "1";
            revoke.textContent = "Revoke — click again to confirm";
            setTimeout(() => { revoke.dataset.confirm = ""; revoke.textContent = "Revoke"; }, 5000);
            return;
          }
          revoke.disabled = true;
          try {
            await api("POST", "/api/remote/revoke", { id: d.id });
            code.replaceChildren(); // an earlier "is paired" would now be wrong
            toast(`${d.name} is revoked. It is disconnected and must pair again with a new QR code.`, "good");
          } catch (e) {
            toast(`Not revoked. ${e.message}`, "bad");
          }
          refresh();
        };
        row.append(revoke);
      }
      list.append(row);
    }
    await refreshRequests();
    pair.hidden = false;
    /* Noticed on any refresh — the one after an approval, or a later one. */
    const fresh = shown && devices.find((d) => !shown.known.has(d.id));
    if (fresh) {
      shown = null;
      if (timer) { clearInterval(timer); timer = null; }
      code.replaceChildren(el("p", "good", `${fresh.name} is paired. It reconnects by itself from now on.`));
      pair.disabled = false;
    }
    return reply;
  }

  pair.onclick = async () => {
    pair.disabled = true;
    stop();
    let issued;
    try {
      issued = await api("POST", "/api/remote/pair");
    } catch (e) {
      code.replaceChildren(el("p", "note warning", `No pairing code: ${e.message}`));
      pair.disabled = false;
      return;
    }
    shown = { known: new Set(devices.map((d) => d.id)), expires: issued.expires };
    /* An image, not markup: nothing lcl-remote printed becomes part of the page. */
    const qr = el("img", "qr");
    qr.alt = "Pairing QR code";
    qr.src = "data:image/svg+xml;base64," + btoa(issued.svg);
    const fingerprint = (issued.fingerprint.match(/.{4}/g) || []).slice(0, 8).join(" ");
    const left = el("p", "note");
    const text = el("input");
    text.id = "remote-link";
    text.type = "text";
    text.readOnly = true;
    text.value = issued.payload;
    const textLabel = el("label", "note", "Or paste this pairing text into the app:");
    textLabel.htmlFor = "remote-link";
    code.replaceChildren(
      qr,
      el("p", "", "On the phone: LCL → Pair a PC → Scan QR code. The phone shows this fingerprint before it asks to pair:"),
      el("p", "mono", `${fingerprint} …`),
      el("p", "", "Scanning does not trust the phone. After Pair is pressed on the phone, its request appears under " +
        "Pending pairing requests: approve it only if its verification code is the one the phone shows."),
      el("p", "note", `The code pairs one device. Addresses in it: ${issued.addresses.join(", ")}.`),
      left,
      textLabel,
      text,
    );
    const tick = async () => {
      const seconds = Math.round(issued.expires - Date.now() / 1000);
      await refresh();
      if (!shown) return; // paired: nothing left to count down
      if (seconds <= 0) {
        stop();
        shown = null;
        code.replaceChildren(el("p", "note", "The code expired. Pair Android device makes a new one."));
        pair.disabled = false;
      } else if (waiting.some((r) => r.status === "pending")) {
        left.textContent = `A request is waiting below: compare its verification code with the phone. The code expires in ${remaining(issued.expires)}.`;
      } else {
        left.textContent = `Waiting for the phone… the code expires in ${remaining(issued.expires)}.`;
      }
    };
    timer = setInterval(tick, 2000);
    tick();
  };

  /* After the dialog is on screen: this runs while it is still being built. */
  Promise.resolve().then(refresh);
}

/* --------------------------------------------------------------- render */

/* Paint the document.
 *
 * Phase C replaces the plain text with engine-supplied token spans; until a
 * document has tokens, it renders as plain text rather than as a guess. */
function render() {
  const doc = current();
  syncDocumentUI(doc);
  if (!doc) {
    paint.replaceChildren();
    gutter.replaceChildren();
    $("#doc-state").textContent = "";
    $("#cursor").textContent = "";
    return;
  }
  paintTokens(doc);
  renderGutter(doc);
  const required = (doc.slots || []).filter((m) => m.kind === "required_slot").length;
  $("#doc-state").textContent = (dirty(doc) ? "modified" : "saved") +
    (required ? ` · ${required} required slot${required === 1 ? "" : "s"} to fill` : "");
  updateCursor();
  syncScroll();
}

/* The one transition between "no document" and "a document is open".
 *
 * The source textarea is transparent by design: what is seen is the paint
 * layer under it. With no document there is nothing to paint, so an editable
 * textarea would take keystrokes and show none of them. With no document it
 * is therefore disabled and emptied, the empty-state panel says why, and every
 * action that needs a document is disabled with it. Called from `render`,
 * which every open, close, edit, save and reload already goes through. */
function syncDocumentUI(doc) {
  const open = Boolean(doc);
  code.disabled = !open;
  if (!open && code.value !== "") code.value = "";
  emptyState.hidden = open;
  for (const selector of DOCUMENT_ACTIONS) $(selector).disabled = !open;
  /* A report describes one document. With none open, the analysis panels go
   * back to their empty state instead of describing a closed tab. */
  if (!open) {
    view("diagnostics").replaceChildren(el("div", "empty", "Nothing checked yet."));
    view("structure").replaceChildren(el("div", "empty",
      "Run Inspect to see imports, declarations and the execution plan."));
  }
}

/* The slot marks of the current text, from the engine, for a file whose tree
 * entry declares a project part role. Anything else has none. */
async function refreshSlots(doc, revision) {
  const listed = state.entries.find((e) => e.id === doc.id);
  if (!listed || !listed.kind || !listed.kind.startsWith("kind.part.")) { doc.slots = []; return; }
  try {
    const reply = await api("POST", "/api/slots", { id: doc.id }, doc.text);
    if (doc.revision === revision) doc.slots = reply.marks || [];
  } catch (_) {
    doc.slots = [];
  }
}

const SLOT_TITLES = {
  required_slot: "Required slot: fill this in",
  optional_slot: "Optional slot: fill it in or delete the line",
  guidance: "Guidance for filling this file in",
  generated_id: "Generated identifier",
};

function renderGutter(doc) {
  const lines = doc.text.split("\n").length;
  const marks = diagnosticLines(doc);
  const slots = new Map((doc.slots || []).map((m) => [m.line, m.kind]));
  const frag = document.createDocumentFragment();
  for (let n = 1; n <= lines; n++) {
    const line = el("span", "ln", String(n));
    const mark = marks.get(n);
    if (mark) line.classList.add(`has-${mark}`);
    const slot = slots.get(n);
    if (slot) { line.classList.add(`slot-${slot}`); line.title = SLOT_TITLES[slot] || slot; }
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
 * only which CSS class expresses each. Between an edit and the engine's next
 * answer, each layer's spans have moved with the text they describe, and the
 * characters the edit touched are plain (see `carry`).
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

  for (const mark of doc.marks.squiggles) {
    const id = SEVERITY.indexOf(mark.severity);
    if (id <= 0) continue;
    const end = Math.max(mark.end, mark.start + 1);
    for (let b = mark.start; b < end && b < total; b++) {
      if (squiggle[b] < id || squiggle[b] === 0) squiggle[b] = id;
    }
  }

  for (const mark of doc.marks.refs) {
    for (let b = mark.start; b < mark.end && b < total; b++) isRef[b] = 1;
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
  for (const mark of doc.marks.squiggles) {
    /* The engine's own derived line, not one counted here; an edit ahead of
     * it moves it by the line feeds that edit added or removed. */
    const severity = mark.severity === "bad" ? "bad" : "warn";
    if (marks.get(mark.line) !== "bad") marks.set(mark.line, severity);
  }
  return marks;
}

/* ------------------------------------------------- analysis (phase C) */

/* Whether an answer about `doc` at `revision` still describes `doc`.
 *
 * These answers are functions of the bytes that were submitted, so the
 * document's own revision is the whole of the question: an answer for any
 * other revision describes text that is no longer there, and two answers for
 * one revision describe the same text and are interchangeable. The document
 * must also still be the one the map holds under its id — closing a tab and
 * opening it again makes a new document, and an answer about the old one is
 * not about the new one.
 *
 * `save` and `reload` have always tested this; it is the same test. */
function describes(doc, revision) {
  return state.docs.get(doc.id) === doc && doc.revision === revision;
}

/* Take the next generation for one kind of request on one document. */
function issue(doc, kind) {
  doc.issued[kind] += 1;
  return doc.issued[kind];
}

/* Whether an answer may be applied, and record it if so.
 *
 * Two questions, and both have to hold. `describes` asks whether the document
 * and its root text are still the ones that were asked about. The generation
 * asks whether this answer is newer than the newest already applied, which is
 * what orders two answers the revision cannot tell apart.
 *
 * `analysis` and the explicit check share one generation because they share
 * one `report`: an answer to either is stale once the other has been applied
 * on top of it. */
function accepts(doc, revision, kind, generation) {
  if (!describes(doc, revision)) return false;
  if (generation <= doc.accepted[kind]) return false;
  doc.accepted[kind] = generation;
  return true;
}

async function refreshTokens() {
  const doc = current();
  if (!doc) return;
  const revision = doc.revision;
  const generation = issue(doc, "tokens");
  try {
    const reply = await api("POST", "/api/tokens", { id: doc.id }, doc.text);
    if (!accepts(doc, revision, "tokens", generation)) return;
    doc.tokens = reply.tokens;
    /* Painting is the active tab's business, and storing is the document's.
     * An answer for a tab nobody is looking at belongs in that tab's cache
     * and nowhere on screen. */
    if (current() === doc) render();
  } catch (_) {
    /* No engine answer, no highlighting: a failed request about the text on
     * screen leaves it painted as plain text. A failure about text that has
     * since changed, or older than an answer already applied, says nothing
     * about what is there now. */
    if (describes(doc, revision) && generation > doc.accepted.tokens) {
      doc.tokens = null;
      if (current() === doc) render();
    }
  }
}

async function runAnalysis() {
  const doc = current();
  if (!doc) return;
  const revision = doc.revision;
  const generation = issue(doc, "analysis");
  await refreshTokens();
  /* The tokens step awaited, so the document may have moved on. Asking the
   * engine about text that is already gone would only produce another answer
   * to throw away, and pairing a report with tokens from a different revision
   * is the thing being prevented. */
  if (!describes(doc, revision)) return;
  try {
    /* Inspect rather than check: it reaches the same diagnostics and also
     * carries the resolver's bindings, which is what navigation needs. A
     * document that fails before resolution still gets its diagnostics. */
    const report = await api("POST", "/api/inspect", { id: doc.id }, doc.text);
    if (!accepts(doc, revision, "analysis", generation)) return;
    doc.report = report;
    doc.navigation = report.navigation || null;
    markReport(doc);
    /* Kept separately from the report: a run replaces `report`, and stepping
     * needs the plan to turn an invocation's node index into a span. */
    if (report.structure) doc.plan = report.structure.plan;
    await refreshSlots(doc, revision);
    if (current() !== doc) return;
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
    given: doc, revision: doc.revision,   // the document and text it was given
  };
  doc.stepAt = null;
  showView("execution");
  renderExecution();
  renderReadiness();
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
    if (doc) {
      doc.report = run.report;
      doc.stepAt = null;
      /* Its spans are drawn only on the text the run was given. Edited since,
       * the marks already drawn stay: they have moved with the text. */
      if (describes(run.given, run.revision)) markReport(doc);
    }
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
    renderReadiness();
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
  /* The textarea is disabled with no document, so this should not happen;
   * if anything does reach it, re-rendering the empty state discards it
   * rather than keeping text nobody can see. */
  if (!doc) { render(); return; }
  const wasDirty = dirty(doc);
  replaceText(doc, code.value);
  doc.stepAt = null;
  render();
  renderTabs();
  if (dirty(doc) !== wasDirty) renderTree();
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
  /* Shift+F12 first, and plain F12 says it is plain: `e.key === "F12"` is true
     with Shift held too, so a bare test for it ahead of this one answered
     Shift+F12 with goToDefinition and left findReferences unreachable. */
  else if (e.shiftKey && e.key === "F12") { e.preventDefault(); findReferences(); }
  else if (!e.shiftKey && e.key === "F12") { e.preventDefault(); goToDefinition(); }
  else if (meta && e.shiftKey && e.key === "F") { e.preventDefault(); findReferences(); }
  else if (e.key === "Escape") closeModal();
});

/* Ctrl+click follows a reference, the way every editor does it. */
code.addEventListener("click", (e) => {
  if (e.ctrlKey || e.metaKey) { e.preventDefault(); goToDefinition(); }
});

/* Check (steps 1 to 5) or Validate (steps 1 to 9) over the buffer. A
 * validated project entry's report is also the project's readiness. */
async function engineCommand(route) {
  const doc = current();
  if (!doc) return;
  const revision = doc.revision;
  const generation = issue(doc, "analysis");
  let report;
  try {
    report = await api("POST", route, { id: doc.id }, doc.text);
  } catch (e) {
    toast(`Not checked. ${e.message}`, "bad");
    return;
  }
  if (!accepts(doc, revision, "analysis", generation)) return;
  doc.report = report;
  doc.navigation = null;
  markReport(doc);
  if (route === "/api/validate" && report.project && report.project.entry === doc.id) {
    showReadiness(doc.id, report);
  }
  if (current() !== doc) return;
  renderDiagnostics(doc); renderStructure(doc); render();
  showView("diagnostics");
}
$("#act-check").onclick = () => engineCommand("/api/check");
$("#act-validate").onclick = () => engineCommand("/api/validate");
$("#act-inspect").onclick = async () => { await runAnalysis(); showView("structure"); };
$("#act-run").onclick = startRun;
$("#act-save").onclick = () => save();
$("#act-reload").onclick = reload;
$("#act-new").onclick = newDocument;
$("#act-new-project").onclick = newProject;
$("#act-settings").onclick = openSettings;
$("#act-manual").onclick = openManual;
$("#readiness-refresh").onclick = () => refreshReadiness();

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

/* ------------------------------------------------- Core 0.3 projects */

/* What a person reads for a role: the last segment of its canonical kind,
 * capitalised, as the server's own label is. Presentation only. */
function roleLabel(kind) {
  const last = String(kind).split(".").pop();
  return last.charAt(0).toUpperCase() + last.slice(1);
}

/* The roles the Core 0.3.0 engine defines, and whether it is here at all. */
async function loadRoles() {
  try {
    state.roles = await api("GET", "/api/roles");
  } catch (_) {
    state.roles = { available: false, roles: [], modes: [] };
  }
  $("#act-new-project").disabled = !state.roles.available;
}

/* "default:guided", "canonical:minimal" or "master:<id>" as request params. */
function selectionParams(role, choice) {
  const [source, value] = String(choice || "default:guided").split(/:(.*)/s);
  if (source === "master") return { master: value };
  return { source, mode: value };
}

/* A scaffold's exact text, each marked line tagged with what it is. The
 * marks are the engine's editor metadata; nothing here reads the text. */
function showScaffold(pre, scaffold) {
  const marks = new Map();
  for (const mark of scaffold.marks || []) marks.set(mark.line, mark.kind);
  const symbol = { required_slot: "!", optional_slot: "?", guidance: "#", generated_id: "⚙" };
  const lines = scaffold.text.replace(/\n$/, "").split("\n");
  lines.forEach((line, i) => {
    const kind = marks.get(i + 1);
    const row = el("span", kind ? `mark ${kind}` : "", `${kind ? symbol[kind] : " "} ${line}\n`);
    if (kind) row.title = kind.replace("_", " ");
    pre.append(row);
  });
}

/* ------------------------------------------------------ save conflict */

/* A save was refused because the file changed on disk after this revision
 * was loaded — on the phone, in another window or in another program. The
 * person chooses; nothing is merged and nothing is overwritten unasked. */
function showConflict(doc) {
  const where = doc.conflict.exists ? "changed on disk" : "deleted from disk";
  modal("Changed on disk", (body) => {
    body.append(el("p", "",
      `${doc.id} was ${where} after you loaded it — by LCL for Android, another window ` +
      "or another program. Your edits were not saved, and nothing was merged."));
    body.append(el("p", "note",
      "Reload disk version replaces your edits with what is on disk. Keep mine writes " +
      "your version over it, after one more confirmation."));
  }, [
    ["Cancel", "", (close) => close()],
    ["Reload disk version", "", async (close) => {
      close();
      await resolveConflict(doc, "reload");
    }],
    ["Keep mine…", "primary", (close) => {
      close();
      modal("Overwrite the disk version?", (body) => {
        body.append(el("p", "",
          `The version of ${doc.id} on disk will be replaced by yours. The other ` +
          "change is lost unless it was saved elsewhere."));
      }, [
        ["Cancel", "", (c) => c()],
        ["Overwrite with mine", "primary", async (c) => {
          c();
          await resolveConflict(doc, "keep");
        }],
      ]);
    }],
  ]);
}

async function resolveConflict(doc, choice) {
  if (state.docs.get(doc.id) !== doc || !doc.conflict) return;
  const seen = doc.conflict;
  if (choice === "reload") {
    try {
      const reply = await api("GET", "/api/document", { id: doc.id });
      doc.conflict = null;
      doc.saved = reply.text;
      doc.digest = reply.digest;
      replaceText(doc, reply.text);
      if (current() === doc) code.value = doc.text;
      renderTabs(); renderTree(); render();
      await refreshTokens();
      toast(`Reloaded ${doc.id} from disk.`);
    } catch (e) {
      toast(`Not reloaded. ${e.message}`, "bad");
    }
    return;
  }
  /* Keep mine: a save over exactly the revision the conflict reported. If the
   * file moved again since, that is a new conflict, reported the same way. */
  doc.conflict = null;
  if (seen.exists) {
    doc.digest = seen.digest;
    await save(doc);
    return;
  }
  try {
    const created = await api("POST", "/api/document", { id: doc.id }, doc.text);
    doc.digest = created.digest;
    doc.saved = doc.text;
    doc.lifecycle = "saved";
    renderTabs(); await loadTree(); render();
    toast(`Saved ${doc.id} again.`, "good");
  } catch (e) {
    toast(`Not saved. ${e.message}`, "bad");
  }
}

/* ------------------------------------------------------- users manual */

/* The Users Manual opens in its own window, beside the editor: this page,
 * its documents and its unsaved edits stay exactly as they are. The manual
 * is packaged into the workspace, so it needs no network. */
function openManual() {
  const url = new URL("/manual/", location.origin);
  url.searchParams.set("t", TOKEN);
  const opened = window.open(url.toString(), "lcl-users-manual", "popup=yes,width=1000,height=820");
  if (opened) opened.focus();
  else toast("The browser blocked the Users Manual window. Allow pop-ups for this page and try again.", "warn");
}

/* -------------------------------------------------------- new project */

/* New Project: a folder, the Core version, a scaffold or project Master, an
 * exact preview of every file, and only then Create. The server recomputes
 * the plan and refuses it if it is not the one previewed. */
async function newProject() {
  await loadRoles();
  if (!state.roles.available) {
    toast("Core 0.3.0 is not available in this workspace, and multi-file projects need it.", "warn");
    return;
  }
  let masters = [];
  try { masters = (await api("GET", "/api/masters")).masters || []; } catch (_) { masters = []; }
  let plan = null;
  modal("New project", (body) => {
    body.append(el("p", "",
      "A new folder named after the project, in the Projects folder chosen in Settings. " +
      "Its files are listed below before anything is written."));
    const nameLabel = el("label", "", "Project name");
    nameLabel.htmlFor = "project-name";
    const name = el("input", "field");
    name.id = "project-name";
    name.value = "New_Project";
    const where = el("p", "note");
    where.id = "project-where";
    const coreLabel = el("label", "", "LCL Core version");
    coreLabel.htmlFor = "project-core";
    const core = el("select", "field");
    core.id = "project-core";
    const only = el("option", "", `LCL Core ${state.roles.core}`);
    only.value = state.roles.core;
    core.append(only);
    const fromLabel = el("label", "", "Start from");
    fromLabel.htmlFor = "project-from";
    const from = el("select", "field");
    from.id = "project-from";
    for (const [value, label] of [
      ["default:guided", "Default (Guided)"], ["default:minimal", "Default (Minimal)"],
      ["canonical:guided", "Canonical — Guided"], ["canonical:minimal", "Canonical — Minimal"],
    ].concat(masters.filter((m) => m.role === "kind.project" && m.valid)
      .map((m) => [`master:${m.id}`, `Master: ${m.name}`]))) {
      const option = el("option", "", label);
      option.value = value;
      from.append(option);
    }
    const files = el("div", "plan");
    files.id = "project-plan";
    let asked = 0;
    const show = async () => {
      const mine = ++asked;
      plan = null;
      $("#project-create").disabled = true;
      files.replaceChildren(el("p", "note", "Preparing the preview…"));
      where.replaceChildren();
      try {
        const reply = await api("GET", "/api/project/plan",
          { name: name.value.trim(), ...selectionParams("kind.project", from.value) });
        if (mine !== asked) return;
        files.replaceChildren();
        where.append(el("span", "", `Will be created as ${reply.path}`));
        for (const file of reply.files) {
          const box = el("details");
          const origin = file.origin.kind === "master" ? `Master ${file.origin.master}` : `canonical, ${file.origin.mode}`;
          box.append(el("summary", "", `${file.path} — ${file.label} (${origin})`));
          const pre = el("pre", "scaffold-preview");
          showScaffold(pre, file);
          box.append(pre);
          files.append(box);
        }
        if (reply.exists) {
          where.replaceChildren(el("span", "bad",
            `${reply.path} already exists. An existing project is never written into: choose another name.`));
          return;
        }
        if (reply.opens_projects_folder) {
          const unsaved = unsavedDocuments();
          if (unsaved.length) {
            where.append(el("span", "bad",
              `. This window will show the Projects folder ${reply.projects} instead of ` +
              `${state.session.root}: save or close ${unsaved.map((d) => d.id).join(", ")} first.`));
            return;
          }
          where.append(el("span", "",
            `. This window will then show the Projects folder ${reply.projects}.`));
        }
        plan = reply;
        $("#project-create").disabled = false;
      } catch (e) {
        if (mine === asked) files.replaceChildren(el("p", "note warning", e.message));
      }
    };
    name.oninput = () => { clearTimeout(name.timer); name.timer = setTimeout(show, 250); };
    from.onchange = show;
    body.append(nameLabel, name, where, coreLabel, core, fromLabel, from, files);
    setTimeout(show, 0);
  }, [
    ["Cancel", "", (close) => close()],
    ["Create", "primary", async (close) => {
      if (!plan) return;
      const chosen = plan;
      close();
      let made;
      try {
        made = await api("POST", "/api/project", {
          name: chosen.name,
          ...selectionParams("kind.project", $("#project-from") ? $("#project-from").value : ""),
          plan_digest: chosen.plan_digest,
        });
      } catch (e) {
        toast(`Not created. ${e.message}`, "bad");
        return;
      }
      try {
        if (chosen.opens_projects_folder) await showOpenedFolder();
        else await loadTree();
        await openDocument(made.entry);
      } catch (e) {
        toast(`Created ${made.path}, but it could not be shown. ${e.message}`, "warn");
        return;
      }
      toast(`Created project ${made.name}: ${made.files.length} files in ${made.path}.`, "good");
    }],
  ]);
  const create = [...$("#modal-actions").querySelectorAll("button")].pop();
  create.id = "project-create";
  create.disabled = true;
}

/* ---------------------------------------------------------- readiness */

/* Project readiness is the engine's: `validate` over the entry, with every
 * part read from disk. Which project is shown is never guessed: the open
 * document when it is an entry (it declares kind.project), else the entry
 * last shown when it lists the open document as a part. */
function entryFor(id) {
  const listed = state.entries.find((e) => e.id === id);
  if (listed && listed.kind === "kind.project") return id;
  const shown = state.readiness;
  if (shown.entry && shown.report && shown.report.project &&
      shown.report.project.parts.some((p) => p.unit === id)) return shown.entry;
  return null;
}

async function refreshReadiness() {
  const doc = current();
  const entry = doc ? entryFor(doc.id) : state.readiness.entry;
  if (!entry) { renderReadiness(); return; }
  const generation = ++state.readiness.generation;
  state.readiness.entry = entry;
  state.readiness.status = "loading";
  renderReadiness();
  const open = state.docs.get(entry);
  try {
    const report = open
      ? await api("POST", "/api/validate", { id: entry }, open.text)
      : await api("GET", "/api/project/status", { entry });
    if (generation !== state.readiness.generation) return;
    showReadiness(entry, report);
  } catch (e) {
    if (generation !== state.readiness.generation) return;
    state.readiness.status = "invalid";
    state.readiness.report = null;
    state.readiness.problem = e.message;
    renderReadiness();
  }
}

function showReadiness(entry, report) {
  const project = report.project;
  state.readiness.entry = entry;
  state.readiness.report = report;
  state.readiness.problem = null;
  state.readiness.status = !project ? "invalid"
    : project.admission === "admitted" ? "ready"
    : !project.complete ? "incomplete" : "invalid";
  renderReadiness();
}

function readinessStatus() {
  const r = state.readiness;
  if (state.run && !state.run.finished && state.run.document === r.entry) return "running";
  return r.status;
}

function renderReadiness() {
  const box = $("#readiness");
  box.replaceChildren();
  const r = state.readiness;
  const doc = current();
  const shown = doc && entryFor(doc.id);
  box.hidden = !shown;
  if (!shown) return;
  const status = readinessStatus();
  const head = el("div", `readiness-state ${status}`, `${r.entry} — ${status}`);
  head.id = "readiness-state";
  box.append(head);
  if (r.problem) box.append(el("p", "note warning", r.problem));
  const project = r.report && r.report.project;
  if (!project) return;
  const list = el("ul", "readiness-files");
  const row = (label, status, unit, role, required) => {
    const li = el("li", `file ${status}`);
    li.append(el("span", "name", label), el("span", "state", status));
    if (role) li.append(el("span", "role", `${roleLabel(role)}${required === false ? ", optional" : ""}`));
    if (unit) { li.tabIndex = 0; li.onclick = () => openDocument(unit); li.onkeydown = (e) => { if (e.key === "Enter") openDocument(unit); }; }
    list.append(li);
  };
  row(project.entry, project.entry_status, project.entry, "kind.project");
  for (const part of project.parts) {
    row(part.unit || part.source, part.status, part.unit && state.entries.some((e) => e.id === part.unit) ? part.unit : null,
      part.kind, part.required);
  }
  box.append(list);
  const diagnostics = (r.report.diagnostics || []);
  if (diagnostics.length) {
    const d = el("ul", "readiness-diagnostics");
    for (const item of diagnostics.slice(0, 20)) {
      const li = el("li", "", `${item.id} — ${item.source}:${item.position ? item.position.line : "?"}`);
      li.title = item.meaning || "";
      if (state.entries.some((e) => e.id === item.source)) {
        li.tabIndex = 0;
        li.onclick = () => openDocument(item.source, { focusByte: item.span.start });
      }
      d.append(li);
    }
    box.append(d);
  }
  /* The engine refuses to run a project that is not admitted; the button
   * says so first. */
  if (doc && doc.id === r.entry) {
    $("#act-run").disabled = status !== "ready" && status !== "running";
    $("#act-run").title = status === "ready" ? "Steps 1 to 13." : "The project is not ready: see Project readiness.";
  }
}

/* ---------------------------------------------------------- templates */

/* Settings → Templates. The server stores Masters and the engine validates
 * them; this dialog lists them and edits their JSON. A Master is copied into a
 * file when the file is created, so nothing here changes an existing file. */
async function openTemplates() {
  let listing, roles;
  try {
    [listing, roles] = await Promise.all([api("GET", "/api/masters"), api("GET", "/api/roles")]);
  } catch (e) {
    toast(`Templates could not be read. ${e.message}`, "bad");
    return;
  }
  const all = roles.available ? [roles.project].concat(roles.roles) : [];
  modal("Templates", (body) => {
    if (!listing.available) {
      body.append(el("p", "note warning", "Templates need a configuration folder (HOME or XDG_CONFIG_HOME)."));
      return;
    }
    body.append(el("p", "note",
      `Stored in ${listing.location}. A default is used for new files of its kind unless ` +
      "another template is chosen; Canonical uses the scaffold the LCL Core defines."));
    if (listing.defaults_problem) body.append(el("p", "note warning", listing.defaults_problem));
    const table = el("div", "templates-defaults");
    for (const role of all) {
      const label = el("label", "", `${role.label} default`);
      const pick = el("select", "field");
      pick.id = `default-${role.role}`;
      label.htmlFor = pick.id;
      const canonical = el("option", "", "Canonical scaffold");
      canonical.value = "";
      pick.append(canonical);
      for (const m of listing.masters.filter((m) => (m.type || m.role) === role.role && m.valid)) {
        const option = el("option", "", m.name);
        option.value = m.id;
        pick.append(option);
      }
      pick.value = (listing.defaults || {})[role.role] || "";
      pick.onchange = async () => {
        try {
          await api("PUT", "/api/masters/default", { role: role.role, id: pick.value });
          toast(pick.value ? `New ${role.label} files now start from ${pick.value}.` : `New ${role.label} files now start from the canonical scaffold.`, "good");
        } catch (e) {
          toast(`Default not changed. ${e.message}`, "bad");
          openTemplates();
        }
      };
      table.append(label, pick);
    }
    body.append(table);
    const list = el("ul", "templates");
    for (const m of listing.masters) {
      const li = el("li", m.valid ? "" : "invalid");
      li.append(el("span", "name", `${m.name || m.id} (${m.id})`),
        el("span", "role", m.role ? (m.label || roleLabel(m.role)) : "unreadable"));
      if (!m.valid) li.append(el("span", "note warning", m.problem || "invalid"));
      for (const [label, act] of [
        ["Edit", () => editTemplate(m.id, "replace")],
        ["Duplicate", () => editTemplate(m.id, "duplicate")],
        ["Delete", () => deleteTemplate(m.id)],
      ]) {
        const b = el("button", "", label);
        b.type = "button";
        b.onclick = act;
        li.append(b);
      }
      list.append(li);
    }
    if (!listing.masters.length) list.append(el("li", "note", "No templates yet."));
    body.append(list);
    const newLabel = el("label", "", "New template for");
    const newRole = el("select", "field");
    newRole.id = "template-new-role";
    newLabel.htmlFor = newRole.id;
    for (const role of all) {
      const option = el("option", "", role.label);
      option.value = role.role;
      newRole.append(option);
    }
    const create = el("button", "", "New template…");
    create.type = "button";
    create.onclick = () => editTemplate(null, "create", newRole.value);
    body.append(newLabel, newRole, create);
  }, [["Close", "primary", (close) => close()]]);
}

/* The editor for one Master's JSON. "create" starts from the canonical
 * scaffold of `role`; "duplicate" from a copy under a new id. */
async function editTemplate(id, how, role) {
  let json;
  try {
    if (how === "create") json = (await api("GET", "/api/master/starter", { role, mode: "guided" })).json;
    else json = (await api("GET", "/api/master", { id })).json;
  } catch (e) {
    toast(`Template could not be read. ${e.message}`, "bad");
    return;
  }
  if (how === "duplicate") {
    try {
      const copy = JSON.parse(json);
      copy.id = `${copy.id}-copy`;
      copy.name = `${copy.name} (copy)`;
      json = JSON.stringify(copy, null, 2);
    } catch (_) { /* an unreadable Master is shown as it is, to be fixed */ }
  }
  modal(how === "replace" ? `Edit template ${id}` : "New template", (body) => {
    body.append(el("p", "note",
      "The template's file. It is checked by the LCL engine when you save, and a template " +
      "that is not valid is not saved. Files already made from it do not change."));
    const area = el("textarea", "field template-json");
    area.id = "template-json";
    area.spellcheck = false;
    area.value = json;
    body.append(area);
  }, [
    ["Cancel", "", (close) => { close(); openTemplates(); }],
    ["Save", "primary", async (close) => {
      const text = $("#template-json").value;
      try {
        await api("PUT", "/api/master", how === "replace" ? { replace: id } : { create: "1" }, text);
        close();
        toast("Template saved.", "good");
        openTemplates();
      } catch (e) {
        toast(`Not saved. ${e.message}`, "bad");
      }
    }],
  ]);
}

function deleteTemplate(id) {
  modal(`Delete template ${id}?`, (body) => {
    body.append(el("p", "", "Files already made from it are not changed. A default naming it goes back to the canonical scaffold."));
  }, [
    ["Cancel", "", (close) => { close(); openTemplates(); }],
    ["Delete", "danger", async (close) => {
      close();
      try {
        await api("DELETE", "/api/master", { id });
        toast(`Deleted template ${id}.`, "good");
      } catch (e) {
        toast(`Not deleted. ${e.message}`, "bad");
      }
      openTemplates();
    }],
  ]);
}

/* ------------------------------------------------------------ convert */

/* Convert a standalone kind.task document into a project in a new folder: an
 * entry holding its IMPORT, EXTENSION and EXECUTE blocks and one task part
 * holding the rest, byte for byte. The server offers it only when the result
 * is admitted; the original is never changed. */
function convertDocument(id) {
  let plan = null;
  const stem = id.split("/").pop().replace(/\.lcl(\.txt)?$/, "");
  const parent = id.includes("/") ? id.slice(0, id.lastIndexOf("/") + 1) : "";
  modal("Convert to multi-file project", (body) => {
    body.append(el("p", "",
      `${id} stays exactly as it is. The project is written to a new folder, and only ` +
      "after you have seen every file."));
    const label = el("label", "", "New folder");
    label.htmlFor = "convert-folder";
    const folder = el("input", "field");
    folder.id = "convert-folder";
    folder.value = `${parent}${stem}_project`;
    const out = el("div", "plan");
    let asked = 0;
    const show = async () => {
      const mine = ++asked;
      plan = null;
      $("#convert-create").disabled = true;
      out.replaceChildren(el("p", "note", "Preparing the preview…"));
      try {
        const reply = await api("GET", "/api/convert/plan", { id, folder: folder.value.trim() });
        if (mine !== asked) return;
        out.replaceChildren();
        for (const file of reply.files) {
          const box = el("details");
          box.open = true;
          box.append(el("summary", "", file.path));
          box.append(el("pre", "scaffold-preview", file.text));
          out.append(box);
        }
        plan = reply;
        $("#convert-create").disabled = false;
      } catch (e) {
        if (mine === asked) out.replaceChildren(el("p", "note warning", e.message));
      }
    };
    folder.oninput = () => { clearTimeout(folder.timer); folder.timer = setTimeout(show, 250); };
    body.append(label, folder, out);
    setTimeout(show, 0);
  }, [
    ["Cancel", "", (close) => close()],
    ["Create project", "primary", async (close) => {
      if (!plan) return;
      const chosen = plan;
      const folder = $("#convert-folder").value.trim();
      close();
      try {
        await api("POST", "/api/convert", { id, folder, plan_digest: chosen.plan_digest });
        await loadTree();
        await openDocument(chosen.entry);
        toast(`Created ${chosen.entry}. ${id} is unchanged.`, "good");
      } catch (e) {
        toast(`Not converted. ${e.message}`, "bad");
      }
    }],
  ]);
  const create = [...$("#modal-actions").querySelectorAll("button")].pop();
  create.id = "convert-create";
  create.disabled = true;
}

/* ----------------------------------------------------------------- boot */

(async function boot() {
  /* Preferences and the empty state come first, before any request, so the
   * page never shows an editable editor with nothing in it. */
  applySettings(loadSettings());
  render();
  try {
    await loadSession();
    await loadFileSettings();
    /* How this launch chose its folder, when that is worth saying — such as
     * a chosen default workspace that no longer exists. */
    if (state.session.notice) toast(state.session.notice, "warn");
    await loadRoles();
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
    /* In the background, never in the way: only a quiet mark if one waits. */
    checkUpdatesWhenDue();
  } catch (e) {
    /* The message is text from the server or the transport, never markup. */
    const failure = el("div", null, `Could not start: ${e.message}`);
    failure.style.cssText = "padding:40px;font:14px system-ui;color:#e0645f";
    document.body.replaceChildren(failure);
  }
})();
