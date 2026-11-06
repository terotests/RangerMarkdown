// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_node::MdNode;
use crate::rgr::MdText;

/// ============================================================================
/// MdFrontMatter.rgr — the metadata block, read.
/// ============================================================================
///
///   ---
///   title: Design notes
///   page: A4
///   margin: 20mm
///   runningHead: title
///   pageNumbers: bottom-center
///   toc: true
///   ---
///
/// The block parser already keeps this — it has to, or `title: …` is drawn as
/// a paragraph — and until now nothing read it. This does: it is the YAML
/// subset a front-matter block actually uses, which is `key: value` and
/// nothing else. No anchors, no nesting, no flow sequences. A document that
/// wants those wants a configuration file.
///
/// Everything has a default and a file with no front matter renders, so this
/// can only ever make a page more specific — never fail it.
/// ============================================================================
#[derive(Clone)]
pub struct MdMeta {
    pub keys: Vec<String>,
    pub values: Vec<String>,
}

impl MdMeta {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
        }
    }
    pub fn has(&self, key: &str) -> bool {
        self.indexOfKey(key) >= 0
    }
    pub fn indexOfKey(&self, key: &str) -> i64 {
        let want: String = MdChar::lower(key);
        let mut i: i64 = 0;
        let n: i64 = self.keys.len() as i64;
        while i < n {
            if self.keys[i as usize].clone() == want {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn get(&self, key: &str, fallback: &str) -> String {
        let i: i64 = self.indexOfKey(key);
        if i < 0 {
            return fallback.to_string();
        }
        self.values[i as usize].clone()
    }
    pub fn flag(&self, key: &str, fallback: bool) -> bool {
        let v: String = MdChar::lower(&self.get(key, ""));
        if v.is_empty() {
            return fallback;
        }
        if v == "true" {
            return true;
        }
        if v == "yes" {
            return true;
        }
        if v == "on" {
            return true;
        }
        if v == "1" {
            return true;
        }
        false
    }
    pub fn put(&mut self, key: &str, value: &str) {
        self.keys.push(MdChar::lower(key));
        self.values.push(value.to_string());
    }
    pub fn count(&self) -> i64 {
        self.keys.len() as i64
    }
}

#[derive(Clone)]
pub struct MdFrontMatter {
}

impl MdFrontMatter {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The document's front matter, or an empty set. Only the FIRST block
    /// counts: a `---` further down is a thematic break and the parser has
    /// already treated it as one.
    pub fn of(doc: &Rc<RefCell<MdNode>>) -> Rc<RefCell<MdMeta>> {
        let mut meta: Rc<RefCell<MdMeta>> = Rc::new(RefCell::new(MdMeta::new()));
        let n: i64 = doc.borrow().childCount();
        if n == 0 {
            return meta;
        }
        let mut first: Rc<RefCell<MdNode>> = doc.borrow().childAt(0);
        if first.borrow().kind != "front_matter" {
            return meta;
        }
        MdFrontMatter::parseInto(&first.borrow().literal.clone(), &meta);
        meta
    }
    pub fn parseInto(text: &str, meta: &Rc<RefCell<MdMeta>>) {
        let mut lines: Vec<String> = text.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut i: i64 = 0;
        let n: i64 = lines.len() as i64;
        while i < n {
            let raw: String = lines[i as usize].clone();
            let line: String = raw.trim().to_string();
            let __len: i64 = MdText::str_len(&line) as i64;
            if __len > 0 {
                let first: i64 = MdText::char_at(&line, 0);
                if first != 35 {
                    let cut: i64 = MdFrontMatter::colonAt(&line);
                    if cut > 0 {
                        let key: String = MdText::substr(&line, 0, cut).trim().to_string();
                        let rest: String = MdFrontMatter::uncomment(&MdText::substr(&line, cut + 1, __len).trim().to_string());
                        meta.borrow_mut().put(&key, &MdFrontMatter::unquote(&rest));
                    }
                }
            }
            i += 1;
        }
    }
    /// A value without its comment, as YAML has it: `mode: book  # slides |
    /// book` is "book". A comment is a # with a space before it and a space
    /// (or nothing) after it, outside quotes, so `color: #ff8800`, `title:
    /// Osa #2` and `title: "A # B"` keep theirs; `key: # note` is empty.
    pub fn uncomment(v: &str) -> String {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut quote: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else {
                if (c == 34 || c == 39) && (i == 0 || MdText::char_at(&v, i - 1) == 32) {
                    quote = c;
                }
                if c == 35 {
                    let mut space_before: bool = i == 0;
                    if i > 0 {
                        let before: i64 = MdText::char_at(&v, i - 1);
                        space_before = before == 32 || before == 9;
                    }
                    let mut space_after: bool = i + 1 >= n;
                    if i + 1 < n {
                        let after: i64 = MdText::char_at(&v, i + 1);
                        space_after = after == 32 || after == 9;
                    }
                    if space_before && space_after {
                        return MdText::substr(&v, 0, i).trim().to_string();
                    }
                }
            }
            i += 1;
        }
        v.to_string()
    }
    pub fn colonAt(line: &str) -> i64 {
        let n: i64 = MdText::str_len(&line) as i64;
        let mut i: i64 = 0;
        while i < n {
            if MdText::char_at(&line, i) == 58 {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn unquote(v: &str) -> String {
        let n: i64 = MdText::str_len(&v) as i64;
        if n < 2 {
            return v.to_string();
        }
        let a: i64 = MdText::char_at(&v, 0);
        let b: i64 = MdText::char_at(&v, n - 1);
        if a == 34 && b == 34 {
            return MdText::substr(&v, 1, n - 1);
        }
        if a == 39 && b == 39 {
            return MdText::substr(&v, 1, n - 1);
        }
        v.to_string()
    }
    /// ----------------------------------------------------------------------
    /// Lengths
    /// ----------------------------------------------------------------------
    ///
    /// A page is measured in points because that is what a PDF is measured in,
    /// and nobody writes margins in points. `20mm`, `0.75in`, `56pt` and a bare
    /// number all arrive here; the bare number is points, stated rather than
    /// guessed.
    /// `length` is a name the compiler resolves elsewhere; this is `points`.
    pub fn points(v: &str, fallback: f64) -> f64 {
        let t: String = MdChar::lower(&v.trim().to_string());
        let n: i64 = MdText::str_len(&t) as i64;
        if n == 0 {
            return fallback;
        }
        let mut unit: String = String::new();
        let mut digits: String = t.clone();
        if n > 2 {
            let tail: String = MdText::substr(&t, n - 2, n);
            if (((tail == "mm" || tail == "cm") || tail == "in") || tail == "pt") || tail == "px" {
                unit = tail.clone();
                digits = MdText::substr(&t, 0, n - 2);
            }
        }
        let value: f64 = MdFrontMatter::number(&digits);
        if value <= 0.0 {
            return fallback;
        }
        if unit == "mm" {
            return value * 2.8346456692913384;
        }
        if unit == "cm" {
            return value * 28.346456692913385;
        }
        if unit == "in" {
            return value * 72.0;
        }
        value
    }
    /// A decimal, without a locale and without an exception. Anything that is
    /// not a number is 0, and every caller treats 0 as "keep the default".
    pub fn number(v: &str) -> f64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut whole: f64 = 0.0;
        let mut frac: f64 = 0.0;
        let mut scale: f64 = 1.0;
        let mut seen_dot: bool = false;
        let mut any: bool = false;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if MdChar::isDigit(c) {
                any = true;
                let d: f64 = (c - 48) as f64;
                if seen_dot {
                    scale *= 10.0;
                    frac += d / scale;
                } else {
                    whole = whole * 10.0 + d;
                }
            } else {
                if c == 46 {
                    if seen_dot {
                        return 0.0;
                    }
                    seen_dot = true;
                } else {
                    return 0.0;
                }
            }
            i += 1;
        }
        if false == any {
            return 0.0;
        }
        whole + frac
    }
    /// `a4 landscape`, `letter-landscape`, `a4 portrait`. A sheet turned on its
    /// side is the same sheet, so this is one flag over the table below rather
    /// than a second table — and `portrait` is spelled out because a document
    /// that says which way up it is should not have to know the default.
    pub fn isLandscape(name: &str) -> bool {
        let t: String = MdChar::lower(&name.trim().to_string());
        MdFrontMatter::hasSuffix(&t, "landscape")
    }
    /// Not `endsWith`: that name collides with a built-in on this compiler and
    /// the call resolves to nothing, the way `replace` did in `MdEdit`.
    pub fn hasSuffix(s: &str, suffix: &str) -> bool {
        let n: i64 = MdText::str_len(&s) as i64;
        let m: i64 = MdText::str_len(&suffix) as i64;
        if m > n {
            return false;
        }
        MdText::substr(&s, n - m, n) == suffix
    }
    /// The size name with any orientation word taken off it.
    pub fn sizeName(name: &str) -> String {
        let t: String = MdChar::lower(&name.trim().to_string());
        let mut out: String = t;
        if MdFrontMatter::hasSuffix(&out, "landscape") {
            out = MdText::substr(&out, 0, (MdText::str_len(&out) as i64) - 9);
        }
        if MdFrontMatter::hasSuffix(&out, "portrait") {
            out = MdText::substr(&out, 0, (MdText::str_len(&out) as i64) - 8);
        }
        MdFrontMatter::stripTrailingDash(&out).trim().to_string()
    }
    pub fn stripTrailingDash(s: &str) -> String {
        let n: i64 = MdText::str_len(&s) as i64;
        if n == 0 {
            return s.to_string();
        }
        let c: i64 = MdText::char_at(&s, n - 1);
        if c == 45 || c == 95 {
            return MdText::substr(&s, 0, n - 1);
        }
        s.to_string()
    }
    /// The width a document gets, orientation included.
    pub fn pageW(name: &str, fallback: f64, fallback_h: f64) -> f64 {
        if MdFrontMatter::isLandscape(name) {
            return MdFrontMatter::pageHeight(&MdFrontMatter::sizeName(name), fallback_h);
        }
        MdFrontMatter::pageWidth(&MdFrontMatter::sizeName(name), fallback)
    }
    pub fn pageH(name: &str, fallback: f64, fallback_w: f64) -> f64 {
        if MdFrontMatter::isLandscape(name) {
            return MdFrontMatter::pageWidth(&MdFrontMatter::sizeName(name), fallback_w);
        }
        MdFrontMatter::pageHeight(&MdFrontMatter::sizeName(name), fallback)
    }
    /// The named page sizes, in points, plus `210x297mm` for anything else.
    pub fn pageWidth(name: &str, fallback: f64) -> f64 {
        let t: String = MdChar::lower(&name.trim().to_string());
        if t == "a3" {
            return 841.89;
        }
        if t == "a4" {
            return 595.28;
        }
        if t == "a5" {
            return 419.53;
        }
        if t == "letter" {
            return 612.0;
        }
        if t == "legal" {
            return 612.0;
        }
        let x: i64 = MdFrontMatter::xAt(&t);
        if x > 0 {
            let unit: String = MdFrontMatter::unitOf(&t);
            let w: String = format!("{}{}", MdText::substr(&t, 0, x), unit);
            return MdFrontMatter::points(&w, fallback);
        }
        fallback
    }
    pub fn pageHeight(name: &str, fallback: f64) -> f64 {
        let t: String = MdChar::lower(&name.trim().to_string());
        if t == "a3" {
            return 1190.55;
        }
        if t == "a4" {
            return 841.89;
        }
        if t == "a5" {
            return 595.28;
        }
        if t == "letter" {
            return 792.0;
        }
        if t == "legal" {
            return 1008.0;
        }
        let x: i64 = MdFrontMatter::xAt(&t);
        if x > 0 {
            let n: i64 = MdText::str_len(&t) as i64;
            let h: String = MdText::substr(&t, x + 1, n);
            return MdFrontMatter::points(&h, fallback);
        }
        fallback
    }
    pub fn xAt(t: &str) -> i64 {
        let n: i64 = MdText::str_len(&t) as i64;
        let mut i: i64 = 0;
        while i < n {
            if MdText::char_at(&t, i) == 120 {
                return i;
            }
            i += 1;
        }
        -1
    }
    /// `210x297mm` gives its unit once, at the end, and the width has to borrow
    /// it — otherwise the page is 210 points wide and 297 millimetres tall.
    pub fn unitOf(t: &str) -> String {
        let n: i64 = MdText::str_len(&t) as i64;
        if n < 2 {
            return String::new();
        }
        let tail: String = MdText::substr(&t, n - 2, n);
        if tail == "mm" {
            return "mm".to_string();
        }
        if tail == "cm" {
            return "cm".to_string();
        }
        if tail == "in" {
            return "in".to_string();
        }
        if tail == "pt" {
            return "pt".to_string();
        }
        if tail == "px" {
            return "px".to_string();
        }
        String::new()
    }
}
