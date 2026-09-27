"use strict";
/* The LCL Users Manual viewer.
 *
 * One viewer for both frontends: the desktop workspace serves it in its own
 * manual window, and the Android app packages it for its Manual tab. The
 * manual is documentation, not language authority: nothing here talks to the
 * engine, a project or a document, and nothing is written anywhere except the
 * reader's own position.
 *
 * The content is the packaged snapshot of users_manual/ (Markdown). It is
 * rendered by building DOM nodes with textContent only, so no manual text can
 * become markup or script. Only snapshot files are reachable; any other link
 * target is shown as text, not followed.
 */

/* ------------------------------------------------------------- Markdown */

const LIST = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;
const indentOf = (line) => line.length - line.trimStart().length;
const blank = (line) => /^\s*$/.test(line);

/* Inline Markdown to a tree: strings and {t, c, href}. */
function inline(text) {
  const out = [];
  let buf = "";
  const flush = () => { if (buf) { out.push(buf); buf = ""; } };
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === "\\" && i + 1 < text.length && /[\\`*_[\]()#+\-.!|<>{}]/.test(text[i + 1])) {
      buf += text[i + 1]; i += 2; continue;
    }
    if (c === "`") {
      let n = 1;
      while (text[i + n] === "`") n++;
      const end = text.indexOf("`".repeat(n), i + n);
      if (end > 0) {
        flush();
        let code = text.slice(i + n, end);
        if (code.length > 1 && code.startsWith(" ") && code.endsWith(" ")) code = code.slice(1, -1);
        out.push({ t: "code", c: [code] });
        i = end + n; continue;
      }
    }
    if ((c === "!" && text[i + 1] === "[") || c === "[") {
      const at = c === "!" ? i + 1 : i;
      const link = linkAt(text, at);
      if (link) {
        flush();
        out.push(c === "!" ? { t: "span", c: [link.label] } : { t: "a", href: link.href, c: inline(link.label) });
        i = link.end; continue;
      }
    }
    if (c === "<") {
      const auto = /^<(https?:\/\/[^>\s]+)>/.exec(text.slice(i));
      if (auto) { flush(); out.push({ t: "a", href: auto[1], c: [auto[1]] }); i += auto[0].length; continue; }
    }
    if (c === "*" || c === "_") {
      const mark = text[i + 1] === c ? c + c : c;
      const open = i + mark.length;
      const wordBefore = c === "_" && /[\p{L}\p{N}]/u.test(text[i - 1] || "");
      const end = text.indexOf(mark, open);
      if (!wordBefore && end > open && !/\s/.test(text[open]) && !/\s/.test(text[end - 1])) {
        flush();
        out.push({ t: mark.length === 2 ? "strong" : "em", c: inline(text.slice(open, end)) });
        i = end + mark.length; continue;
      }
    }
    buf += c; i++;
  }
  flush();
  return out;
}

function linkAt(text, i) {
  let depth = 0, j = i;
  for (; j < text.length; j++) {
    if (text[j] === "[") depth++;
    else if (text[j] === "]" && --depth === 0) break;
  }
  if (j >= text.length || text[j + 1] !== "(") return null;
  const close = text.indexOf(")", j + 2);
  if (close < 0) return null;
  return { label: text.slice(i + 1, j), href: text.slice(j + 2, close).trim().split(/\s+/)[0], end: close + 1 };
}

/* The text an inline tree shows. */
function plain(text) {
  const walk = (nodes) => nodes.map((n) => (typeof n === "string" ? n : walk(n.c || []))).join("");
  return walk(inline(text));
}

/* Block Markdown to a list of blocks. */
function parse(md) {
  const lines = md.replace(/\r\n?/g, "\n").split("\n");
  const blocks = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (blank(line)) { i++; continue; }
    let m;
    if ((m = /^(\s*)(`{3,}|~{3,})\s*([\w+-]*)/.exec(line))) {
      const fence = m[2], indent = m[1].length, body = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith(fence)) {
        body.push(lines[i].slice(Math.min(indent, indentOf(lines[i]))));
        i++;
      }
      i++;
      blocks.push({ t: "pre", lang: m[3], text: body.join("\n") });
      continue;
    }
    if ((m = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line))) {
      blocks.push({ t: "h", level: m[1].length, text: m[2] }); i++; continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) { blocks.push({ t: "hr" }); i++; continue; }
    if (/^\s*>/.test(line)) {
      const body = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ""));
      blocks.push({ t: "blockquote", children: parse(body.join("\n")) });
      continue;
    }
    if (/^\s*\|/.test(line) && i + 1 < lines.length && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1])) {
      const cells = (l) => l.trim().replace(/^\|/, "").replace(/\|\s*$/, "")
        .split(/(?<!\\)\|/).map((cell) => cell.trim().replace(/\\\|/g, "|"));
      const head = cells(line);
      const rows = [];
      i += 2;
      while (i < lines.length && /^\s*\|/.test(lines[i])) rows.push(cells(lines[i++]));
      blocks.push({ t: "table", head, rows });
      continue;
    }
    if (LIST.test(line)) {
      const [list, next] = parseList(lines, i);
      blocks.push(list); i = next; continue;
    }
    const para = [line.trim()];
    i++;
    while (i < lines.length && !blank(lines[i]) && !/^\s*(#{1,6}\s|`{3,}|~{3,}|>|\|)/.test(lines[i]) &&
           !LIST.test(lines[i])) {
      para.push(lines[i++].trim());
    }
    blocks.push({ t: "p", text: para.join(" ") });
  }
  return blocks;
}

function parseList(lines, i) {
  const first = LIST.exec(lines[i]);
  const base = first[1].length;
  const ordered = /\d/.test(first[2]);
  const list = { t: ordered ? "ol" : "ul", start: ordered ? parseInt(first[2], 10) : 1, items: [] };
  while (i < lines.length) {
    const m = LIST.exec(lines[i]);
    if (!m || m[1].length !== base || /\d/.test(m[2]) !== ordered) break;
    const content = m[1].length + m[2].length + 1;
    const body = [m[3]];
    i++;
    while (i < lines.length) {
      const l = lines[i];
      if (blank(l)) {
        let k = i + 1;
        while (k < lines.length && blank(lines[k])) k++;
        if (k < lines.length && indentOf(lines[k]) > base) { body.push(""); i++; continue; }
        break;
      }
      const ind = indentOf(l);
      if (ind <= base && (LIST.test(l) || /^(#{1,6}\s|`{3,}|~{3,}|\||>)/.test(l.trimStart()))) break;
      body.push(l.slice(Math.min(ind, content)));
      i++;
    }
    list.items.push(parse(body.join("\n")));
  }
  return [list, i];
}

/* GitHub's heading anchor: lower case, punctuation dropped, spaces to "-". */
function slug(text) {
  return plain(text).toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, "").trim().replace(/\s/g, "-");
}

/* Anchors for every heading of one file, duplicates numbered as GitHub does. */
function headings(blocks) {
  const seen = new Map();
  const out = [];
  const walk = (list) => {
    for (const b of list) {
      if (b.t === "h") {
        const base = slug(b.text);
        const n = seen.get(base) || 0;
        seen.set(base, n + 1);
        b.anchor = n ? `${base}-${n}` : base;
        out.push(b);
      } else if (b.t === "blockquote") walk(b.children);
    }
  };
  walk(blocks);
  return out;
}

function titleOf(file) {
  const h1 = /^#\s+(.+?)\s*#*\s*$/m.exec(file.text);
  return h1 ? plain(h1[1]) : file.name.replace(/\.md$/, "").replace(/_/g, " ");
}

/* Where a link goes: a snapshot file (and anchor), an external address, or
 * something the snapshot does not hold, which is not followed. */
function resolve(href, current, names) {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
    return /^https?:/i.test(href) ? { kind: "external", href } : { kind: "unpackaged", href };
  }
  const hash = href.indexOf("#");
  const path = hash < 0 ? href : href.slice(0, hash);
  const anchor = hash < 0 ? "" : href.slice(hash + 1);
  if (!path) return { kind: "internal", file: current, anchor };
  let name;
  try { name = decodeURIComponent(path.replace(/^\.\//, "")); } catch (_) { return { kind: "unpackaged", href }; }
  return names.includes(name) ? { kind: "internal", file: name, anchor } : { kind: "unpackaged", href };
}

/* Every line holding all the query's words, with the heading above it. */
function search(files, query, limit = 100) {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const results = [];
  for (const file of files) {
    let heading = "", anchor = "", fenced = false;
    const seen = new Map();
    for (const line of file.text.split("\n")) {
      if (/^\s*(```|~~~)/.test(line)) fenced = !fenced;
      const h = !fenced && /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
      if (h) {
        heading = plain(h[2]);
        const base = slug(h[2]);
        const n = seen.get(base) || 0;
        seen.set(base, n + 1);
        anchor = n ? `${base}-${n}` : base;
      }
      const lower = line.toLowerCase();
      if (words.every((w) => lower.includes(w))) {
        results.push({ file: file.name, anchor, heading, line: line.trim() });
        if (results.length >= limit) return results;
      }
    }
  }
  return results;
}

if (typeof module === "object" && module.exports) {
  module.exports = { inline, plain, parse, slug, headings, titleOf, resolve, search };
}

/* -------------------------------------------------------------- viewer */

if (typeof document !== "undefined") {
  const $ = (s) => document.querySelector(s);
  const bridge = window.LclManual || null;   // the Android app's, when there
  const token = (document.querySelector('meta[name="lcl-token"]') || {}).content || "";
  const view = { files: [], names: [], version: "", digest: "", parsed: new Map() };

  /* The reader's position: per route scroll offsets and the last route, for
   * this session only. The Android app keeps it for the app's process. */
  const memory = {
    read() {
      try {
        const raw = bridge ? bridge.recall() : sessionStorage.getItem("lcl.manual.position");
        return raw ? JSON.parse(raw) : { last: "", scroll: {} };
      } catch (_) { return { last: "", scroll: {} }; }
    },
    write(value) {
      try {
        const raw = JSON.stringify(value);
        if (bridge) bridge.remember(raw); else sessionStorage.setItem("lcl.manual.position", raw);
      } catch (_) { /* a position that cannot be kept is only a convenience lost */ }
    },
  };
  let position = memory.read();

  const el = (tag, cls, text) => {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text !== undefined) node.textContent = text;
    return node;
  };

  function renderInline(nodes, parent, file) {
    for (const n of nodes) {
      if (typeof n === "string") { parent.append(document.createTextNode(n)); continue; }
      if (n.t === "a") {
        const target = resolve(n.href, file, view.names);
        let a;
        if (target.kind === "internal") {
          a = el("a");
          a.href = route(target.file, target.anchor);
        } else if (target.kind === "external" && !bridge) {
          a = el("a");
          a.href = target.href;
          a.target = "_blank";
          a.rel = "noopener noreferrer";
        } else {
          a = el("span", "unpackaged");
          a.title = `${target.href} is not part of the packaged manual`;
        }
        renderInline(n.c, a, file);
        parent.append(a);
        continue;
      }
      const node = el(n.t);
      renderInline(n.c, node, file);
      parent.append(node);
    }
  }

  function renderBlocks(blocks, parent, file) {
    for (const b of blocks) {
      if (b.t === "h") {
        const h = el(`h${b.level}`);
        h.id = b.anchor;
        renderInline(inline(b.text), h, file);
        parent.append(h);
      } else if (b.t === "p") {
        const p = el("p");
        renderInline(inline(b.text), p, file);
        parent.append(p);
      } else if (b.t === "pre") {
        const pre = el("pre");
        pre.append(el("code", "", b.text));
        parent.append(pre);
      } else if (b.t === "hr") {
        parent.append(el("hr"));
      } else if (b.t === "blockquote") {
        const q = el("blockquote");
        renderBlocks(b.children, q, file);
        parent.append(q);
      } else if (b.t === "table") {
        const table = el("table");
        const head = el("tr");
        for (const cell of b.head) { const th = el("th"); renderInline(inline(cell), th, file); head.append(th); }
        const thead = el("thead"); thead.append(head); table.append(thead);
        const tbody = el("tbody");
        for (const row of b.rows) {
          const tr = el("tr");
          for (const cell of row) { const td = el("td"); renderInline(inline(cell), td, file); tr.append(td); }
          tbody.append(tr);
        }
        table.append(tbody);
        parent.append(table);
      } else if (b.t === "ul" || b.t === "ol") {
        const list = el(b.t);
        if (b.t === "ol" && b.start !== 1) list.start = b.start;
        for (const item of b.items) {
          const li = el("li");
          // A one-paragraph item is shown without a paragraph box.
          if (item.length === 1 && item[0].t === "p") renderInline(inline(item[0].text), li, file);
          else renderBlocks(item, li, file);
          list.append(li);
        }
        parent.append(list);
      }
    }
  }

  function parsedOf(name) {
    if (!view.parsed.has(name)) {
      const file = view.files.find((f) => f.name === name);
      const blocks = parse(file.text);
      view.parsed.set(name, { blocks, heads: headings(blocks) });
    }
    return view.parsed.get(name);
  }

  function route(file, anchor) {
    return `#/${encodeURIComponent(file)}${anchor ? `/${encodeURIComponent(anchor)}` : ""}`;
  }

  function current() {
    const hash = location.hash;
    if (hash.startsWith("#?q=")) return { search: decodeURIComponent(hash.slice(4)) };
    const m = /^#\/([^/]*)(?:\/(.*))?$/.exec(hash);
    const home = view.names.includes("README.md") ? "README.md" : view.names[0];
    if (!m) return { file: home, anchor: "" };
    let file, anchor;
    try { file = decodeURIComponent(m[1]); anchor = m[2] ? decodeURIComponent(m[2]) : ""; } catch (_) { return { file: home, anchor: "" }; }
    return view.names.includes(file) ? { file, anchor } : { missing: file, file: home, anchor: "" };
  }

  function renderToc(file) {
    const toc = $("#toc");
    toc.replaceChildren();
    for (const f of view.files) {
      const a = el("a", f.name === file ? "current" : "", titleOf(f));
      a.href = route(f.name, "");
      toc.append(a);
      if (f.name === file) {
        for (const h of parsedOf(f.name).heads.filter((h) => h.level === 2)) {
          const sub = el("a", "sub", plain(h.text));
          sub.href = route(f.name, h.anchor);
          toc.append(sub);
        }
      }
    }
  }

  function show() {
    const where = current();
    const content = $("#content");
    content.replaceChildren();
    $("#toc").classList.remove("open");
    if (where.search !== undefined) {
      $("#search").value = where.search;
      const hits = search(view.files, where.search);
      content.append(el("h1", "", `Search: ${where.search}`));
      if (!hits.length) content.append(el("p", "empty", "Nothing in the manual matches."));
      const list = el("ul", "results");
      for (const hit of hits) {
        const li = el("li");
        const a = el("a", "", hit.line || hit.heading);
        a.href = route(hit.file, hit.anchor);
        const file = view.files.find((f) => f.name === hit.file);
        li.append(a, el("span", "where", `${titleOf(file)}${hit.heading ? ` — ${hit.heading}` : ""}`));
        list.append(li);
      }
      content.append(list);
      renderToc("");
    } else {
      if (where.missing) content.append(el("p", "note", `${where.missing} is not part of the packaged manual.`));
      renderBlocks(parsedOf(where.file).blocks, content, where.file);
      renderToc(where.file);
      document.title = `${titleOf(view.files.find((f) => f.name === where.file))} — LCL Users Manual`;
    }
    const target = where.anchor && document.getElementById(where.anchor);
    const saved = position.scroll[location.hash];
    if (target) target.scrollIntoView();
    else window.scrollTo(0, typeof saved === "number" ? saved : 0);
    position.last = location.hash;
    memory.write(position);
  }

  let scrollTimer = null;
  window.addEventListener("scroll", () => {
    clearTimeout(scrollTimer);
    scrollTimer = setTimeout(() => {
      position.scroll[location.hash] = Math.round(window.scrollY);
      memory.write(position);
    }, 150);
  });
  window.addEventListener("hashchange", show);
  $("#back").onclick = () => history.back();
  $("#forward").onclick = () => history.forward();
  $("#toc-toggle").onclick = () => {
    const open = $("#toc").classList.toggle("open");
    $("#toc-toggle").setAttribute("aria-expanded", String(open));
  };
  $("#search").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const q = e.target.value.trim();
      if (q) location.hash = `#?q=${encodeURIComponent(q)}`;
    } else if (e.key === "Escape") {
      e.target.value = "";
    }
  });

  async function load() {
    if (bridge) return JSON.parse(bridge.snapshot());
    const reply = await fetch(`/manual/snapshot?t=${encodeURIComponent(token)}`,
      { headers: { "X-LCL-Token": token }, cache: "no-store" });
    if (!reply.ok) throw new Error(`the manual could not be loaded (${reply.status})`);
    return reply.json();
  }

  load().then((snapshot) => {
    view.files = snapshot.files;
    view.names = snapshot.files.map((f) => f.name);
    view.version = snapshot.version;
    view.digest = snapshot.digest;
    $("#version").textContent = `version ${snapshot.version} · ${snapshot.digest.slice(0, 12)}`;
    $("#version").title = `manual digest ${snapshot.digest}`;
    if (!location.hash && position.last) history.replaceState(null, "", position.last);
    show();
    if (bridge && bridge.loaded) bridge.loaded(`${snapshot.version} ${snapshot.digest} ${snapshot.files.length}`);
  }).catch((error) => {
    $("#content").replaceChildren(el("p", "note", error.message));
  });
}
