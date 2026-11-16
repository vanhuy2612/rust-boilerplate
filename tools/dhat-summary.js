#!/usr/bin/env node
// Summarize a dhat heap report (dhat-heap/*.json) as readable tables.
//
// Usage: node tools/dhat-summary.js <file.json> [top=15] [--app-only]
//
// dhat JSON uses short keys. Each entry in "pps" (program point) is ONE allocation
// call stack ("fs" = frame indices into "ftbl"):
//   tb / tbk  total bytes / blocks allocated there during the window (freed or not)
//   mb / mbk  max bytes / blocks live at once at that call stack
//   gb / gbk  bytes / blocks live at the global peak ("t-gmax")
//   eb / ebk  bytes / blocks still live at the end of the window ("t-end") -> leak candidates
//   tl        total lifetime of all blocks (µs, see "tu")
// "tg" = time of global peak, "te" = time at end of window.

const fs = require("fs");

const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith("--") && !/^\d+$/.test(a));
const top = Number(args.find((a) => /^\d+$/.test(a)) || 15);
const appOnly = args.includes("--app-only");
if (!file) {
  console.error("usage: node tools/dhat-summary.js <dhat-heap.json> [top] [--app-only]");
  process.exit(1);
}

const data = JSON.parse(fs.readFileSync(file, "utf8"));
const APP_CRATE = "rust_boilerplate";

// Frames from allocator / std plumbing are noise when looking for "who allocated".
const NOISE = /^(\[root\]|alloc::|core::|std::|dhat::|hashbrown::|__rust|<?alloc::|<?core::|<?std::)/;

function frameName(index) {
  // "0x7ff6...: crate::path::fn (file.rs:12:0)" -> "crate::path::fn (file.rs:12)"
  return data.ftbl[index]
    .replace(/^0x[0-9a-f]+: /i, "")
    .replace(/:0\)$/, ")");
}

function shortFn(name, max = 110) {
  const s = name.replace(/<[^<>]*(<[^<>]*>[^<>]*)*>/g, "<..>");
  return s.length > max ? s.slice(0, max - 1) + "…" : s;
}

// Most useful frame of a stack: first frame from this app, else first non-noise frame.
function site(pp) {
  const names = pp.fs.map(frameName);
  // Only frames that ARE app functions (not library frames with app types in generics).
  const app = names.find((n) => n.startsWith(APP_CRATE + "::") || n.startsWith("<" + APP_CRATE + "::"));
  const first = names.find((n) => !NOISE.test(n)) || names[names.length - 1];
  return { site: app || first, isApp: Boolean(app), stack: names };
}

// dhat records no variable names, only "fn (file.rs:line)". Read that source line so the
// report shows the statement (and therefore the variable) that owns the allocation.
const path = require("path");
const PROJECT_ROOT = path.resolve(__dirname, "..");
const sourceCache = new Map();
function sourceLine(name) {
  const match = /\(([^()]+\.rs):(\d+)\)$/.exec(name);
  if (!match) return null;
  const rel = match[1].replace(/\\/g, "/");
  const candidates = [rel, rel.replace(/^[^/]+\/(?=src\/)/, "")] // "rust/src/main.rs" -> "src/main.rs"
    .map((p) => path.resolve(PROJECT_ROOT, p));
  for (const file of candidates) {
    if (!sourceCache.has(file)) {
      sourceCache.set(file, fs.existsSync(file) ? fs.readFileSync(file, "utf8").split(/\r?\n/) : null);
    }
    const lines = sourceCache.get(file);
    if (lines) return (lines[Number(match[2]) - 1] || "").trim();
  }
  return null;
}

function bytes(n) {
  if (n >= 1 << 30) return (n / (1 << 30)).toFixed(2) + " GB";
  if (n >= 1 << 20) return (n / (1 << 20)).toFixed(2) + " MB";
  if (n >= 1 << 10) return (n / (1 << 10)).toFixed(1) + " KB";
  return n + " B";
}

// Group program points by allocation site so one line of code = one row.
const groups = new Map();
for (const pp of data.pps) {
  const s = site(pp);
  if (appOnly && !s.isApp) continue;
  const g = groups.get(s.site) || { site: s.site, tb: 0, tbk: 0, gb: 0, eb: 0, ebk: 0, mb: 0, stacks: 0 };
  g.tb += pp.tb;
  g.tbk += pp.tbk;
  g.gb += pp.gb;
  g.eb += pp.eb;
  g.ebk += pp.ebk;
  g.mb = Math.max(g.mb, pp.mb);
  g.stacks += 1;
  groups.set(s.site, g);
}
const rows = [...groups.values()];

const sum = (key) => data.pps.reduce((acc, pp) => acc + pp[key], 0);
console.log(`File: ${file}`);
console.log(`Program: ${data.cmd} (pid ${data.pid})`);
console.log(`Window length: ${(data.te / 1e6).toFixed(1)} s, global peak at ${(data.tg / 1e6).toFixed(1)} s`);
console.log("");
console.log(`Total allocated in window : ${bytes(sum("tb"))} in ${sum("tbk")} blocks`);
console.log(`Live at global peak       : ${bytes(sum("gb"))} in ${sum("gbk")} blocks`);
console.log(`Still live at end (t-end) : ${bytes(sum("eb"))} in ${sum("ebk")} blocks`);
if (appOnly) console.log("(tables below: only stacks that pass through this app's code)");

function table(title, key, extra) {
  const list = rows.filter((r) => r[key] > 0).sort((a, b) => b[key] - a[key]).slice(0, top);
  console.log(`\n=== ${title} ===`);
  if (list.length === 0) return console.log("(none)");
  for (const r of list) {
    console.log(`${bytes(r[key]).padStart(10)}  ${extra(r).padEnd(22)}  ${shortFn(r.site)}`);
    const code = sourceLine(r.site);
    if (code) console.log(`${" ".repeat(36)}code: ${code}`);
  }
}

table("Still live at end of window (leak candidates)", "eb", (r) => `${r.ebk} blocks`);
table("Live at the global peak (who uses RAM at peak)", "gb", (r) => `max ${bytes(r.mb)}`);
table("Total allocated (allocation churn)", "tb", (r) => `${r.tbk} blocks`);
