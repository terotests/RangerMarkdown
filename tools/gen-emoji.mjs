/**
 * gen-emoji.mjs — write src/MdEmoji.rgr from markdown-it-emoji's tables.
 *
 *   npm --prefix gallery/markdown/harness install
 *   node gallery/markdown/tools/gen-emoji.mjs
 *
 * `:wink:` and `;)` are markdown-it-emoji's syntax, so its tables are the
 * oracle: the "full" set of GitHub shortcodes (`lib/data/full.mjs`) and its
 * emoticon shortcuts (`lib/data/shortcuts.mjs`), both MIT, © Vitaly Puzrin.
 *
 * Packed the way MdEntities is, for the same reason: some 1900 names as
 * `set` statements would be that many statements to type-check and run before
 * the first document is parsed. Names are `name:hex[,hex]` pairs, space
 * separated, split on first lookup. The shortcuts are few and full of the
 * characters a packed string would need escaping for (`:")`, `</3`), so they
 * are written out one per line, longest first — the order the matcher tries
 * them in.
 *
 * The generated file is checked in, so building the parser never needs the
 * registry.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.join(HERE, "..", "src", "MdEmoji.rgr");
const PKG = path.join(HERE, "..", "harness", "node_modules", "markdown-it-emoji");

let full;
let shortcuts;
let version = "";
try {
  full = (await import(pathToFileURL(path.join(PKG, "lib", "data", "full.mjs")).href)).default;
  shortcuts = (await import(pathToFileURL(path.join(PKG, "lib", "data", "shortcuts.mjs")).href)).default;
  version = JSON.parse(fs.readFileSync(path.join(PKG, "package.json"), "utf8")).version;
} catch {
  console.error("markdown-it-emoji is not installed.");
  console.error("  npm --prefix gallery/markdown/harness install");
  console.error("The checked-in gallery/markdown/src/MdEmoji.rgr is unchanged.");
  process.exit(1);
}

const names = Object.keys(full).sort();
const pairs = names.map(
  (n) => n + ":" + [...full[n]].map((ch) => ch.codePointAt(0).toString(16)).join(",")
);
const chunks = [];
let cur = "";
for (const p of pairs) {
  if (cur.length + p.length + 1 > 900) {
    chunks.push(cur);
    cur = p;
  } else {
    cur = cur ? cur + " " + p : p;
  }
}
if (cur) chunks.push(cur);

// alias -> name, for names the full table has; longest alias first, ties in
// a fixed order so the file does not churn
const aliases = [];
for (const name of Object.keys(shortcuts)) {
  if (!full[name]) continue;
  const list = Array.isArray(shortcuts[name]) ? shortcuts[name] : [shortcuts[name]];
  for (const a of list) aliases.push([a, name]);
}
aliases.sort((x, y) => y[0].length - x[0].length || (x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0));

const lit = (s) => '"' + s.replace(/\\/g, "\\\\").replace(/"/g, '\\"') + '"';

const L = [];
const w = (s) => L.push(s);
w("; SPDX-License-Identifier: AGPL-3.0-or-later");
w("");
w("; ============================================================================");
w("; MdEmoji.rgr — emoji shortcodes and emoticons. GENERATED.");
w("; ============================================================================");
w(";");
w(";     node gallery/markdown/tools/gen-emoji.mjs");
w(";");
w("; From markdown-it-emoji " + version + " (MIT, (c) Vitaly Puzrin): its \"full\" table of");
w("; " + names.length + " GitHub shortcodes (`:wink:`) and its " + aliases.length + " emoticon shortcuts (`;)`).");
w("; The names are packed as `name:hex[,hex]` pairs, split on first lookup, the");
w("; way MdEntities packs the HTML5 references; the shortcuts are written out,");
w("; longest first, which is the order `MdTypography` tries them in.");
w("; ============================================================================");
w("");
w('Import "MdChar.rgr"');
w('Import "MdEntities.rgr"');
w("");
w("; One per process (`__singleton`): the table is loaded once, not once per parse.");
w("class MdEmoji @singleton(true) {");
w("    def table:[string:string]");
w("    def loaded:boolean false");
w("    ; the emoticons, longest first, and the name each one stands for");
w("    def shortText:[string]");
w("    def shortName:[string]");
w("");
w("    Constructor () {");
w("    }");
w("");
w("    ; The emoji a shortcode name stands for, or \"\" when there is none.");
w("    fn lookup:string (name:string) {");
w("        if! loaded {");
w("            this.load()");
w("        }");
w("        if (has table name) {");
w('            return (?? (get table name) "")');
w("        }");
w('        return ""');
w("    }");
w("");
w("    fn shortcutCount:int () {");
w("        if! loaded {");
w("            this.load()");
w("        }");
w("        return (array_length shortText)");
w("    }");
w("");
w("    fn shortcutAt:string (i:int) {");
w("        return (itemAt shortText i)");
w("    }");
w("");
w("    fn shortcutName:string (i:int) {");
w("        return (itemAt shortName i)");
w("    }");
w("");
w("    fn load:void () {");
w("        loaded = true");
for (const [a, n] of aliases) {
  w("        push shortText " + lit(a));
  w("        push shortName " + lit(n));
}
w("        def packed:[string]");
for (const c of chunks) w('        push packed "' + c + '"');
w("        def ci:int 0");
w("        def cn:int (array_length packed)");
w("        while (ci < cn) {");
w("            def chunk:string (itemAt packed ci)");
w('            def pairs:[string] (strsplit chunk " ")');
w("            def pi:int 0");
w("            def pn:int (array_length pairs)");
w("            while (pi < pn) {");
w("                def pair:string (itemAt pairs pi)");
w("                def cut:int (MdEntities.colonAt(pair))");
w("                if (cut > 0) {");
w("                    def name:string (substring pair 0 cut)");
w("                    def plen:int (strlen pair)");
w("                    def codes:string (substring pair (cut + 1) plen)");
w("                    set table name (MdEntities.decodeCodes(codes))");
w("                }");
w("                pi = (pi + 1)");
w("            }");
w("            ci = (ci + 1)");
w("        }");
w("    }");
w("}");
w("");

fs.writeFileSync(OUT, L.join("\n"));
console.log("wrote " + OUT + " — " + names.length + " names in " + chunks.length + " chunks, " + aliases.length + " shortcuts");
