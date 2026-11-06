// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::MdText;
use crate::rgr::{CssSheet};

/// ============================================================================
/// MdCssSupport.rgr — which selectors and properties a sheet can use, and a
/// note on every line that does nothing.
/// ============================================================================
///
/// A stylesheet that is partly ignored looks exactly like one that works until
/// the slide is drawn: `text-transform: uppercase` on `.kicker`, `th {
/// background-color }`, `#slide h2`, `color: ;`. The only way to find out was
/// to render and compare. This file is the list of what IS read — per element,
/// for a block's own class or id, and under a slide or a section — and `lint`,
/// which reads a sheet against it and says, for each declaration that will not
/// be applied, which one and why:
///
///   .haalea { opacity }: opacity is not supported; it is ignored.
///   li:nth-child(2): :nth-child is not supported; give the block a class.
///   .rikki: "font-size 20pt" has no ":" and is ignored.
///
/// The same list is the guide's table of supported properties (`table`), so
/// the two cannot say different things. A host that reads rules of its own
/// (Sliqtly's `chart`, `diagram`, `figure`) adds them with `addElement`.
///
/// Nothing here applies a sheet: `MdCss` does. This only reads text.
/// ============================================================================
/// One thing `lint` found: where in the sheet (`at`…`end`, offsets into the
/// text it was handed) and what to tell the author.
#[derive(Clone)]
pub struct MdCssNote {
    pub at: i64,
    pub end: i64,
    pub selector: String,
    pub property: String,
    pub message: String,
}

impl MdCssNote {
    pub fn new() -> Self {
        Self {
            at: 0,
            end: 0,
            selector: String::new(),
            property: String::new(),
            message: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct MdCssSupport {
    /// element names and, at the same index, what each one reads (comma list)
    pub elements: Vec<String>,
    pub reads: Vec<String>,
    /// …and a short word on what each element is, for the guide
    pub about: Vec<String>,
    /// elements a host reads whole under a slide or a section too
    /// (Sliqtly's `#anchor chart { font-size }`)
    pub wholeUnderScope: Vec<String>,
}

impl MdCssSupport {
    pub fn new() -> Self {
        let mut me = Self {
            elements: Vec::new(),
            reads: Vec::new(),
            about: Vec::new(),
            wholeUnderScope: Vec::new(),
        };
        me.defaults();
        me
    }
    pub fn TYPO() -> String {
        "font-weight,font-style,text-transform,letter-spacing".to_string()
    }
    pub fn BOX() -> String {
        "background-color,background,padding,padding-top,padding-right,padding-bottom,padding-left,border,border-color,border-width,border-style,border-radius,box-shadow".to_string()
    }
    pub fn BLOCK() -> String {
        format!("font-size,color,font-family,text-align,column-count,column-gap,list-style,{},{}", MdCssSupport::TYPO(), MdCssSupport::BOX())
    }
    /// `name` in the comma list; an entry ending in `*` (`fx-*`) is a prefix
    pub fn listed(csv: &str, name: &str) -> bool {
        let mut parts: Vec<String> = csv.split(",").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut i: i64 = 0;
        while i < (parts.len() as i64) {
            let part: String = parts[i as usize].clone();
            if part == name {
                return true;
            }
            let n: i64 = MdText::str_len(&part) as i64;
            if n > 1 && MdText::char_at(&part, n - 1) == 42 {
                let head: String = MdText::substr(&part, 0, n - 1);
                if (MdText::str_len(&name) as i64) > (MdText::str_len(&head) as i64) && MdText::substr(&name, 0, (MdText::str_len(&head) as i64)) == head {
                    return true;
                }
            }
            i += 1;
        }
        false
    }
    /// What may be said under a slide or a section (`#intro h2`, `.dark
    /// page`): its colours, faces, backgrounds, typography and list-style.
    /// Sizes and spaces are not — a slide shrunk to fit scales those in
    /// place — so a block that needs another size takes a class.
    pub fn scopedOk(prop: &str) -> bool {
        if MdText::index_of(&prop, "color") >= 0 {
            return true;
        }
        if MdText::index_of(&prop, "background") >= 0 {
            return true;
        }
        if MdText::index_of(&prop, "font-family") >= 0 {
            return true;
        }
        if MdCssSupport::listed(&MdCssSupport::TYPO(), prop) {
            return true;
        }
        if prop == "list-style" || (prop == "text-align" || prop == "box-shadow") {
            return true;
        }
        if prop == "cell-tones" || prop == "content" {
            return true;
        }
        false
    }
    pub fn spaced(csv: &str) -> String {
        (csv.split(",").map(|s| s.to_string()).collect::<Vec<String>>())
            .join(&*", ".to_string())
            .clone()
    }
    /// read by some element or by a class at all
    pub fn anywhere(sup: &MdCssSupport, prop: &str) -> bool {
        if MdCssSupport::listed(&MdCssSupport::BLOCK(), prop) {
            return true;
        }
        let __it_r = sup.reads.clone();
        for r in __it_r.iter().cloned() {
            if MdCssSupport::listed(&r, prop) {
                return true;
            }
        }
        false
    }
    /// the element a selector is about ("" for a bare class or id)
    pub fn subjectOf(sel: &str) -> String {
        let mut words: Vec<String> = CssSheet::split_whitespace(sel);
        if words.is_empty() {
            return String::new();
        }
        let mut last: String = words[((words.len() as i64) - 1) as usize].clone();
        let colon: i64 = MdText::index_of(&last, ":");
        if colon >= 0 {
            last = MdText::substr(&last, 0, colon);
        }
        let tag: String = MdCssSupport::tagOf(&last);
        if tag == "*" {
            return String::new();
        }
        tag
    }
    /// under a slide or a section: more than one part, the outer ones not a
    /// gallery's
    pub fn isScoped(sel: &str) -> bool {
        let mut words: Vec<String> = CssSheet::split_whitespace(sel);
        if (words.len() as i64) < 2 {
            return false;
        }
        let first: String = MdCssSupport::tagOf(&words[0].clone());
        first != "gallery" && first != "cell"
    }
    /// `h2.lead#x` → `h2`
    pub fn tagOf(word: &str) -> String {
        let n: i64 = MdText::str_len(&word) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&word, i);
            if (c == 46 || c == 35) || c == 58 {
                return MdText::substr(&word, 0, i);
            }
            i += 1;
        }
        word.to_string()
    }
    /// a rule whose variables every sheet sees
    pub fn rootLike(sel: &str) -> bool {
        let s: String = MdCssSupport::lower(&sel.trim().to_string());
        ((s == ":root" || s == "html") || (s == "body" || s == "document")) || (s == "page" || s == "*")
    }
    /// ----------------------------------------------------------------------
    /// Variables
    /// ----------------------------------------------------------------------
    /// `--name` declared in a :root-like rule of `css`, by name
    pub fn varNames(css: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut pairs: Vec<String> = MdCssSupport::varPairs(css);
        for p in pairs.iter().cloned() {
            let eq: i64 = MdText::index_of(&p, "=");
            if eq > 0 {
                out.push(MdText::substr(&p, 0, eq));
            }
        }
        out
    }
    /// `--name=value` for each variable a :root-like rule of `css` declares,
    /// in order (a later one of the same name wins where they are used)
    pub fn varPairs(css: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let src: String = CssSheet::strip_comments(css);
        let n: i64 = MdText::str_len(&src) as i64;
        let mut i: i64 = 0;
        while i < n {
            let open: i64 = CssSheet::find_char(&src, i, 123);
            if open < 0 {
                return out;
            }
            let close: i64 = CssSheet::matching_brace(&src, open);
            if close < 0 {
                return out;
            }
            let sel: String = MdText::substr(&src, i, open).trim().to_string();
            if !sel.is_empty() && MdText::char_at(&sel, 0) == 64 {
                // inside @media print / screen too
                let mut inner: Vec<String> = MdCssSupport::varPairs(&MdText::substr(&src, open + 1, close));
                for ip in inner.iter().cloned() {
                    out.push(ip.clone());
                }
            } else {
                let mut root: bool = false;
                let __n_si = sel.split(",").map(|s| s.to_string()).collect::<Vec<String>>().len() as i64;
                for si in 0..__n_si {
                    let mut s = sel.split(",")
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>()[si as usize].clone();
                    if MdCssSupport::rootLike(&s) {
                        root = true;
                    }
                }
                if root {
                    let mut decls: Vec<String> = MdText::substr(&src, open + 1, close)
                        .split(";")
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>();
                    for d in decls.iter().cloned() {
                        let c: i64 = MdText::index_of(&d, ":");
                        if c > 0 {
                            let name: String = MdText::substr(&d, 0, c).trim().to_string();
                            if MdText::index_of(&name, "--") == 0 {
                                out.push(format!("{}={}", name, MdText::substr(&d, c + 1, (MdText::str_len(&d) as i64)).trim().to_string()));
                            }
                        }
                    }
                }
            }
            i = close + 1;
        }
        out
    }
    /// `css` with every `var(--name)` / `var(--name, fallback)` replaced by
    /// the variable's value (`vars`: `--name=value`, later wins), or by its
    /// fallback. One that is neither is left as written, which no value
    /// reader takes, so the declaration does nothing (and `lint` says so).
    pub fn substitute(css: &str, vars: &[String]) -> String {
        if MdText::index_of(&css, "var(") < 0 {
            return css.to_string();
        }
        let mut out: String = css.to_string();
        let mut round: i64 = 0;
        while round < 6 && MdText::index_of(&out, "var(") >= 0 {
            out = MdCssSupport::substituteOnce(&out, vars);
            round += 1;
        }
        out
    }
    pub fn substituteOnce(css: &str, vars: &[String]) -> String {
        let mut out: String = String::new();
        let n: i64 = MdText::str_len(&css) as i64;
        let mut i: i64 = 0;
        while i < n {
            let at: i64 = MdCssSupport::findText(css, i, "var(");
            if at < 0 {
                out = format!("{}{}", out, MdText::substr(&css, i, n));
                return out;
            }
            out = format!("{}{}", out, MdText::substr(&css, i, at));
            // the matching ) — a fallback may hold rgb(…) or another var(…)
            let mut depth: i64 = 0;
            let mut j: i64 = at + 3;
            let mut close: i64 = -1;
            while j < n && close < 0 {
                let c: i64 = MdText::char_at(&css, j);
                if c == 40 {
                    depth += 1;
                }
                if c == 41 {
                    depth -= 1;
                    if depth == 0 {
                        close = j;
                    }
                }
                j += 1;
            }
            if close < 0 {
                out = format!("{}{}", out, MdText::substr(&css, at, n));
                return out;
            }
            let inner: String = MdText::substr(&css, at + 4, close);
            let comma: i64 = MdText::index_of(&inner, ",");
            let mut name: String = inner.trim().to_string();
            let mut fallback: String = String::new();
            let mut has_fallback: bool = false;
            if comma >= 0 {
                name = MdText::substr(&inner, 0, comma).trim().to_string();
                fallback = MdText::substr(&inner, comma + 1, (MdText::str_len(&inner) as i64)).trim().to_string();
                has_fallback = true;
            }
            let value: String = MdCssSupport::valueOf(vars, &name);
            if !value.is_empty() {
                out = format!("{}{}", out, value);
            } else {
                if has_fallback {
                    out = format!("{}{}", out, fallback);
                } else {
                    // left as written, marked so the next round skips it
                    out = format!("{}unset-var({})", out, inner);
                }
            }
            i = close + 1;
        }
        out
    }
    pub fn valueOf(vars: &[String], name: &str) -> String {
        let mut found: String = String::new();
        for v in vars.iter().cloned() {
            let eq: i64 = MdText::index_of(&v, "=");
            if eq > 0 && MdText::substr(&v, 0, eq) == name {
                found = MdText::substr(&v, eq + 1, (MdText::str_len(&v) as i64));
            }
        }
        found
    }
    /// ----------------------------------------------------------------------
    /// Text helpers
    /// ----------------------------------------------------------------------
    pub fn note(mut out: &mut Vec<Rc<RefCell<MdCssNote>>>, at: i64, end: i64, selector: String, property: String, message: String) {
        let mut m: Rc<RefCell<MdCssNote>> = Rc::new(RefCell::new(MdCssNote::new()));
        m.borrow_mut().at = at;
        m.borrow_mut().end = end;
        m.borrow_mut().selector = selector.clone();
        m.borrow_mut().property = property.clone();
        m.borrow_mut().message = message.clone();
        out.push(m.clone());
    }
    /// the end of the declaration starting at `at`: the next `;` outside
    /// parentheses and quotes, or the end of the body
    pub fn declEnd(body: &str, at: i64) -> i64 {
        let n: i64 = MdText::str_len(&body) as i64;
        let mut depth: i64 = 0;
        let mut quote: i64 = 0;
        let mut i: i64 = at;
        while i < n {
            let c: i64 = MdText::char_at(&body, i);
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else {
                if c == 34 || c == 39 {
                    quote = c;
                }
                if c == 40 {
                    depth += 1;
                }
                if c == 41 {
                    depth -= 1;
                }
                if c == 59 && depth <= 0 {
                    return i;
                }
            }
            i += 1;
        }
        n
    }
    /// comments turned to spaces (newlines kept), so offsets still point
    /// into the text the author wrote. The text between comments is copied a
    /// run at a time: a character appended per step is quadratic, and on Go
    /// `charAt` is a byte that `strfromcode` would make a character of its own.
    pub fn blankComments(css: &str) -> String {
        if MdText::index_of(&css, "/*") < 0 {
            return css.to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        let n: i64 = MdText::str_len(&css) as i64;
        let mut from: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            if MdText::char_at(&css, i) == 47 && (i + 1 < n && MdText::char_at(&css, i + 1) == 42) {
                parts.push(MdText::substr(&css, from, i));
                let mut j: i64 = i + 2;
                let mut end: i64 = 0 - 1;
                while j < n && end < 0 {
                    if MdText::char_at(&css, j) == 42 && (j + 1 < n && MdText::char_at(&css, j + 1) == 47) {
                        end = j;
                    } else {
                        j += 1;
                    }
                }
                let mut body_end: i64 = end;
                if end < 0 {
                    body_end = n;
                }
                parts.push("  ".to_string().to_string());
                parts.push(MdCssSupport::blank(&MdText::substr(&css, i + 2, body_end)));
                if end < 0 {
                    return parts.join(&*String::new());
                }
                parts.push("  ".to_string().to_string());
                i = end + 2;
                from = i;
            } else {
                i += 1;
            }
        }
        parts.push(MdText::substr(&css, from, n));
        parts.join(&*String::new())
    }
    /// as many spaces as text has, its line breaks kept
    pub fn blank(text: &str) -> String {
        let mut lines: Vec<String> = text.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut out: Vec<String> = Vec::new();
        for ln in lines.iter().cloned() {
            out.push(MdCssSupport::spaces(MdText::str_len(&ln) as i64));
        }
        out.join(&*"\n".to_string())
    }
    pub fn spaces(n: i64) -> String {
        let mut s: String = " ".to_string();
        while (MdText::str_len(&s) as i64) < n {
            s = format!("{}{}", s, s);
        }
        MdText::substr(&s, 0, n)
    }
    pub fn findIn(s: &str, from: i64, to: i64, ch: i64) -> i64 {
        let mut i: i64 = from;
        while i < to {
            if MdText::char_at(&s, i) == ch {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn findText(s: &str, from: i64, want: &str) -> i64 {
        let rest: String = MdText::substr(&s, from, (MdText::str_len(&s) as i64));
        let k: i64 = MdText::index_of(&rest, want);
        if k < 0 {
            return -1;
        }
        from + k
    }
    pub fn trimStart(s: &str) -> String {
        let n: i64 = MdText::str_len(&s) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            if ((c == 32 || c == 9) || c == 10) || c == 13 {
                i += 1;
            } else {
                return MdText::substr(&s, i, n);
            }
        }
        String::new()
    }
    pub fn has(list: &[String], s: &str) -> bool {
        for x in list.iter().cloned() {
            if x == s {
                return true;
            }
        }
        false
    }
    pub fn short(s: &str) -> String {
        let t: String = s.trim().to_string();
        if (MdText::str_len(&t) as i64) > 40 {
            return format!("{}…", MdText::substr(&t, 0, 39));
        }
        t
    }
    pub fn lower(s: &str) -> String {
        CssSheet::lower(s)
    }
    pub fn defaults(&mut self) {
        let t: String = MdCssSupport::TYPO();
        self.addElement("page",
            "width,height,padding,background-color,background-image,bleed,safe-area",
            "the slide or sheet"
        );
        self.addElement("deck",
            "split-level,overflow,aspect-ratio,crop-marks",
            "how a document becomes slides"
        );
        self.addElement("document",
            &format!("font-family,font-size,line-height,color,{}", t),
            "all text"
        );
        self.addElement("body",
            &format!("font-family,font-size,line-height,color,{}", t),
            "the same as document"
        );
        self.addElement("p",
            &format!("margin-bottom,color,text-align,{}", t),
            "paragraphs"
        );
        self.addElement("heading",
            &format!("font-family,font-size,margin-top,margin-bottom,color,text-align,{}", t),
            "every heading level"
        );
        self.addElement("h1",
            &format!("font-size,color,text-align,{}", t),
            "a level-1 heading"
        );
        self.addElement("h2",
            &format!("font-size,color,text-align,{}", t),
            "a level-2 heading"
        );
        self.addElement("h3",
            &format!("font-size,color,text-align,{}", t),
            "a level-3 heading"
        );
        self.addElement("h4",
            &format!("font-size,color,text-align,{}", t),
            "a level-4 heading"
        );
        self.addElement("h5",
            &format!("font-size,color,text-align,{}", t),
            "a level-5 heading"
        );
        self.addElement("h6",
            &format!("font-size,color,text-align,{}", t),
            "a level-6 heading"
        );
        self.addElement("a", "color", "links");
        self.addElement("code",
            "font-family,font-size,line-height,padding,color,background-color,border-color",
            "code blocks and code in text"
        );
        self.addElement("mark", "background-color,color", "==highlighted== text");
        self.addElement("kbd", "background-color,border-color", "<kbd> keys");
        self.addElement("hr", "color", "--- rules");
        self.addElement("blockquote",
            &format!("color,border-color,border-width,padding-left,padding-top,background-color,background,{}", t),
            "> quotes"
        );
        self.addElement("container",
            "background-color,background,border-radius,box-shadow,padding,padding-top,padding-right,padding-bottom,padding-left,border,border-color,border-width,border-style",
            "{container=box|bubble} plates and ::: blocks"
        );
        self.addElement("list",
            "padding-left,margin-left,color,list-style",
            "lists (color = the bullets)"
        );
        self.addElement("ul", "list-style", "bulleted lists");
        self.addElement("ol", "list-style", "numbered lists");
        self.addElement("li", &format!("margin-bottom,{}", t), "list items");
        self.addElement("table",
            "border-color,background-color,padding-left,padding-top,font-size,cell-tones",
            "tables (background-color = the header row)"
        );
        self.addElement("th", "background-color,color", "a table's header row");
        self.addElement("img",
            "justify-content,text-align,max-height",
            "pictures"
        );
        self.addElement("header",
            "content,content-left,content-center,content-right,background-image,background-color,border-color,font-weight,text-align,font-size,color,height",
            "the band at the top of each page"
        );
        self.addElement("footer",
            "content,content-left,content-center,content-right,background-image,background-color,border-color,font-weight,text-align,font-size,color,height",
            "the band at the foot of each page"
        );
        self.addElement("gallery",
            "gap,column-gap,columns,column-count,background-color",
            "```gallery albums"
        );
        self.addElement("cell",
            "padding,padding-top,padding-right,padding-bottom,padding-left,border-radius,border-width,border-color,background-color,font-size,color,font-family,text-align,font-weight",
            "a gallery's cells (cell:nth(n) for one)"
        );
        self.addElement("caption",
            "padding,padding-top,padding-right,padding-bottom,padding-left,border-radius,border-width,border-color,background-color,font-size,color,font-family,text-align,font-weight",
            "a gallery cell's caption"
        );
    }
    /// An element and what it reads, or more for one already listed.
    pub fn addElement(&mut self, name: &str, props: &str, what: &str) {
        let i: i64 = self.index_of(name);
        if i >= 0 {
            let mut have: String = self.reads[i as usize].clone();
            let mut more: Vec<String> = props.split(",").map(|s| s.to_string()).collect::<Vec<String>>();
            for p in more.iter().cloned() {
                if !MdCssSupport::listed(&have, &p) {
                    have = format!("{},{}", have, p);
                }
            }
            self.reads[i as usize] = have.clone();
            if !what.is_empty() {
                self.about[i as usize] = what.to_string();
            }
            return;
        }
        self.elements.push(name.to_string());
        self.reads.push(props.to_string());
        self.about.push(what.to_string());
    }
    pub fn index_of(&self, name: &str) -> i64 {
        let mut i: i64 = 0;
        while i < (self.elements.len() as i64) {
            if self.elements[i as usize].clone() == name {
                return i;
            }
            i += 1;
        }
        -1
    }
    /// Everything `name` reads is read under a slide or a section as well.
    pub fn readsUnderScope(&mut self, name: &str) {
        if !MdCssSupport::has(&self.wholeUnderScope.clone(), name) {
            self.wholeUnderScope.push(name.to_string());
        }
    }
    pub fn isElement(&self, name: &str) -> bool {
        self.index_of(name) >= 0
    }
    /// what `name` reads, "" for an element that is not one
    pub fn propsOf(&self, name: &str) -> String {
        let i: i64 = self.index_of(name);
        if i < 0 {
            return String::new();
        }
        self.reads[i as usize].clone()
    }
    /// ----------------------------------------------------------------------
    /// The guide's table
    /// ----------------------------------------------------------------------
    /// One line per element: `h2 — a level-2 heading: font-size, color, …`,
    /// then what a class or id reads and what a slide's rules read.
    pub fn table(&self) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < (self.elements.len() as i64) {
            let name: String = self.elements[i as usize].clone();
            let props: String = MdCssSupport::spaced(&self.reads[i as usize].clone());
            let what: String = self.about[i as usize].clone();
            out = format!("{}- `{}`", out, name);
            if !what.is_empty() {
                out = format!("{} ({})", out, what);
            }
            out = format!("{}: {}\n", out, props);
            i += 1;
        }
        out = format!("{}{}{}\n", out, "- `.class` / `#id` on a block (`{.lead}` under a paragraph or heading): ".to_string(), MdCssSupport::spaced(&MdCssSupport::BLOCK()));
        out = format!("{}{}{}, list-style, text-align. Sizes and spacing there are not read: give the block a class.\n", out, "- A slide or section as the first part (`#id h2`, `.dark page`, `slide.dark p`; the id or class is the heading's `{#id .class}`, or its anchor): colours, backgrounds, font-family, ".to_string(), MdCssSupport::spaced(&MdCssSupport::TYPO()));
        out = format!("{}{}", out, "- `:root { --brand: #ff3d7f }` and `var(--brand)` (or `var(--brand, #000)`) anywhere a value goes.\n".to_string());
        out
    }
    /// ----------------------------------------------------------------------
    /// The lint
    /// ----------------------------------------------------------------------
    /// Every declaration and selector in `css` that will not be applied, in
    /// the order they are written. `vars` is the variables the sheets before
    /// this one defined (`name=value` lines), so `var(--brand)` from the
    /// theme is known in the author's own sheet.
    pub fn lint(&self, css: &str, vars: &[String]) -> Vec<Rc<RefCell<MdCssNote>>> {
        let mut out: Vec<Rc<RefCell<MdCssNote>>> = Vec::new();
        let src: String = MdCssSupport::blankComments(css);
        let mut names: Vec<String> = MdCssSupport::varNames(&src);
        for v in vars.iter().cloned() {
            let eq: i64 = MdText::index_of(&v, "=");
            if eq > 0 {
                names.push(MdText::substr(&v, 0, eq));
            }
        }
        self.lintBlock(&src, 0, (MdText::str_len(&src) as i64), &names, &mut out);
        out
    }
    /// the rules between `from` and `to`
    pub fn lintBlock(&self, src: &str, from: i64, to: i64, names: &[String], mut out: &mut Vec<Rc<RefCell<MdCssNote>>>) {
        let mut i: i64 = from;
        while i < to {
            let open: i64 = MdCssSupport::findIn(src, i, to, 123);
            if open < 0 {
                let tail: String = MdText::substr(&src, i, to).trim().to_string();
                if !tail.is_empty() {
                    MdCssSupport::note(&mut out, i, to, tail.clone(), String::new(), format!("\"{}{}", MdCssSupport::short(&tail), "\" has no { … } block and is ignored.".to_string()));
                }
                return;
            }
            let sel_text: String = MdText::substr(&src, i, open).trim().to_string();
            let sel_at: i64 = i + ((MdText::str_len(&MdText::substr(&src, i, open)) as i64) - (MdText::str_len(&MdCssSupport::trimStart(&MdText::substr(&src, i, open))) as i64));
            let close: i64 = CssSheet::matching_brace(src, open);
            if close < 0 || close > to {
                MdCssSupport::note(
                    &mut out,
                    sel_at,
                    open,
                    sel_text.clone(),
                    String::new(),
                    format!("{}{}", sel_text, ": the { is never closed; the rest of the sheet is ignored.".to_string())
                );
                return;
            }
            if !sel_text.is_empty() {
                if MdText::char_at(&sel_text, 0) == 64 {
                    let head: String = MdCssSupport::lower(&sel_text);
                    if head == "@media print" || head == "@media screen" {
                        self.lintBlock(src, open + 1, close, names, &mut out);
                    } else {
                        MdCssSupport::note(
                            &mut out,
                            sel_at,
                            open,
                            sel_text.clone(),
                            String::new(),
                            format!("{}: at-rules other than @media print / @media screen are not read; the block is ignored.", sel_text)
                        );
                    }
                } else {
                    self.lintRule(src,
                        &sel_text,
                        sel_at,
                        open + 1,
                        close,
                        names,
                        &mut out
                    );
                }
            } else {
                MdCssSupport::note(
                    &mut out,
                    open,
                    open + 1,
                    String::new(),
                    String::new(),
                    "A { … } block with no selector is ignored.".to_string()
                );
            }
            i = close + 1;
        }
    }
    pub fn lintRule(&self, src: &str, sel_text: &str, sel_at: i64, body_from: i64, body_to: i64, names: &[String], mut out: &mut Vec<Rc<RefCell<MdCssNote>>>) {
        // each selector of the list: what it can read, "" when nothing
        let mut sels: Vec<String> = sel_text.split(",").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut subjects: Vec<String> = Vec::new();
        let mut scoped_flags: Vec<bool> = Vec::new();
        let mut readable: Vec<String> = Vec::new();
        let mut is_root: bool = false;
        for raw in sels.iter().cloned() {
            let sel: String = raw.trim().to_string();
            let why: String = self.selectorProblem(&sel);
            if !why.is_empty() {
                MdCssSupport::note(
                    &mut out,
                    sel_at,
                    sel_at + (MdText::str_len(&sel_text) as i64),
                    sel.clone(),
                    String::new(),
                    why.clone()
                );
            } else {
                if MdCssSupport::rootLike(&sel) {
                    is_root = is_root || sel == ":root";
                }
                if sel != ":root" {
                    readable.push(sel.clone());
                    subjects.push(MdCssSupport::subjectOf(&sel));
                    scoped_flags.push(MdCssSupport::isScoped(&sel));
                }
            }
        }
        // the declarations
        let body: String = MdText::substr(&src, body_from, body_to);
        let mut at: i64 = 0;
        let n: i64 = MdText::str_len(&body) as i64;
        while at <= n {
            let semi: i64 = MdCssSupport::declEnd(&body, at);
            let piece: String = MdText::substr(&body, at, semi);
            let text: String = piece.trim().to_string();
            let p_at: i64 = body_from + (at + ((MdText::str_len(&piece) as i64) - (MdText::str_len(&MdCssSupport::trimStart(&piece)) as i64)));
            let p_end: i64 = p_at + (MdText::str_len(&text) as i64);
            if !text.is_empty() {
                let colon: i64 = MdText::index_of(&text, ":");
                if colon <= 0 {
                    MdCssSupport::note(&mut out, p_at, p_end, sel_text.to_string(), String::new(), format!("{}: \"{}\" has no \":\" and is ignored.", sel_text, MdCssSupport::short(&text)));
                } else {
                    let prop: String = MdCssSupport::lower(&MdText::substr(&text, 0, colon).trim().to_string());
                    let mut value: String = MdText::substr(&text, colon + 1, (MdText::str_len(&text) as i64))
                        .trim()
                        .to_string();
                    let bang: i64 = MdText::index_of(&value, "!important");
                    if bang >= 0 {
                        value = MdText::substr(&value, 0, bang).trim().to_string();
                    }
                    if value.is_empty() {
                        MdCssSupport::note(
                            &mut out,
                            p_at,
                            p_end,
                            sel_text.to_string(),
                            prop.clone(),
                            format!("{}{}{}{}", sel_text, " { ".to_string(), prop, " }: no value; the line is ignored.".to_string())
                        );
                    } else {
                        self.lintDecl(sel_text,
                            &prop,
                            &value,
                            p_at,
                            p_end,
                            &readable,
                            &subjects,
                            &scoped_flags,
                            is_root,
                            names,
                            &mut out
                        );
                    }
                }
            }
            at = semi + 1;
        }
    }
    pub fn lintDecl(&self, sel_text: &str, prop: &str, value: &str, p_at: i64, p_end: i64, readable: &[String], subjects: &[String], scoped_flags: &[bool], is_root: bool, names: &[String], mut out: &mut Vec<Rc<RefCell<MdCssNote>>>) {
        let head: String = format!("{}{}{}{}", sel_text, " { ".to_string(), prop, " }: ".to_string());
        // variables: kept where every sheet can see them
        if MdText::index_of(&prop, "--") == 0 {
            if !is_root && !self.anyRootLike(readable) {
                MdCssSupport::note(
                    &mut out,
                    p_at,
                    p_end,
                    sel_text.to_string(),
                    prop.to_string(),
                    format!("{}variables are read from :root (or document, page) only; define it there.", head)
                );
            }
            return;
        }
        // a var() nobody defined
        let mut v: i64 = MdText::index_of(&value, "var(");
        while v >= 0 {
            let close: i64 = MdCssSupport::findIn(value, v + 4, (MdText::str_len(&value) as i64), 41);
            let mut inner: String = String::new();
            if close > v {
                inner = MdText::substr(&value, v + 4, close).trim().to_string();
            }
            let comma: i64 = MdText::index_of(&inner, ",");
            let mut name: String = inner.clone();
            if comma >= 0 {
                name = MdText::substr(&inner, 0, comma).trim().to_string();
            }
            if comma < 0 && !MdCssSupport::has(names, &name) {
                MdCssSupport::note(
                    &mut out,
                    p_at,
                    p_end,
                    sel_text.to_string(),
                    prop.to_string(),
                    format!("{}var({}) is not defined; the line is ignored.", head, name)
                );
                return;
            }
            let rest: String = MdText::substr(&value, v + 4, (MdText::str_len(&value) as i64));
            let nx: i64 = MdText::index_of(&rest, "var(");
            if nx < 0 {
                v = -1;
            } else {
                v = (v + 4) + nx;
            }
        }
        if readable.is_empty() {
            if is_root {
                MdCssSupport::note(
                    &mut out,
                    p_at,
                    p_end,
                    sel_text.to_string(),
                    prop.to_string(),
                    format!("{}:root holds variables (--name) only; this is ignored.", head)
                );
            }
            return;
        }
        let low_v: String = MdCssSupport::lower(value);
        if MdText::index_of(&low_v, "gradient(") >= 0 && (prop == "background" || (prop == "background-color" || prop == "background-image")) {
            MdCssSupport::note(
                &mut out,
                p_at,
                p_end,
                sel_text.to_string(),
                prop.to_string(),
                format!("{}gradients are not drawn; use one colour.", head)
            );
            return;
        }
        // read by at least one selector of the list?
        let mut any_ok: bool = false;
        let mut why: String = String::new();
        let mut i: i64 = 0;
        while i < (readable.len() as i64) {
            let subj: String = subjects[i as usize].clone();
            let scoped: bool = scoped_flags[i as usize];
            let mut ok: bool = false;
            if subj.is_empty() {
                ok = MdCssSupport::listed(&MdCssSupport::BLOCK(), prop);
                if !ok {
                    why = format!("{} is not read for a class or id; it is ignored.", prop);
                    if !MdCssSupport::anywhere(self, prop) {
                        why = format!("{} is not supported; it is ignored.", prop);
                    }
                }
            } else {
                ok = MdCssSupport::listed(&self.propsOf(&subj), prop);
                if !ok {
                    why = format!("{} is not read for {}; it is ignored.", prop, subj);
                    if !MdCssSupport::anywhere(self, prop) {
                        why = format!("{} is not supported; it is ignored.", prop);
                    }
                } else {
                    if (scoped && !MdCssSupport::scopedOk(prop)) && !MdCssSupport::has(&self.wholeUnderScope.clone(), &subj) {
                        ok = false;
                        why = format!("{} is not read under a slide or section; give the block a class for sizes and spacing.", prop);
                    }
                }
            }
            if ok {
                any_ok = true;
            }
            i += 1;
        }
        if !any_ok {
            if why.is_empty() {
                why = format!("{} is not supported; it is ignored.", prop);
            }
            MdCssSupport::note(
                &mut out,
                p_at,
                p_end,
                sel_text.to_string(),
                prop.to_string(),
                format!("{}{}", head, why)
            );
        }
    }
    pub fn anyRootLike(&self, sels: &[String]) -> bool {
        for s in sels.iter().cloned() {
            if MdCssSupport::rootLike(&s) {
                return true;
            }
        }
        false
    }
    /// What is wrong with one selector, "" when it can be read.
    pub fn selectorProblem(&self, sel: &str) -> String {
        if sel.is_empty() {
            return "An empty selector (a stray comma) is ignored.".to_string();
        }
        if sel == ":root" {
            return String::new();
        }
        if sel == "*" {
            return String::new();
        }
        if MdText::index_of(&sel, "::") >= 0 {
            return format!("{}: pseudo-elements (::before, ::after) are not supported; the rule is ignored.", sel);
        }
        if MdText::index_of(&sel, "[") >= 0 {
            return format!("{}: attribute selectors are not supported; give the block a class.", sel);
        }
        if (MdText::index_of(&sel, ">") >= 0 || MdText::index_of(&sel, "+") >= 0) || MdText::index_of(&sel, "~") >= 0 {
            return format!("{}: only the space between two parts is read (not >, +, ~); the rule is ignored.", sel);
        }
        let mut words: Vec<String> = CssSheet::split_whitespace(sel);
        let n: i64 = words.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let w: String = words[i as usize].clone();
            let colon: i64 = MdText::index_of(&w, ":");
            if colon >= 0 {
                let pseudo: String = MdText::substr(&w, colon, (MdText::str_len(&w) as i64));
                let paren: i64 = MdText::index_of(&pseudo, "(");
                let mut pname: String = pseudo.clone();
                if paren > 0 {
                    pname = MdText::substr(&pseudo, 0, paren);
                }
                let base: String = MdText::substr(&w, 0, colon);
                if (pname == ":nth" && base == "cell") && i == n - 1 {
                    // a gallery's cell:nth(n)
                } else {
                    return format!("{}: {} is not supported (only cell:nth(n) in a gallery); give the block a class.", sel, pname);
                }
            }
            let tag: String = MdCssSupport::tagOf(&w);
            if i == n - 1 {
                if !tag.is_empty() && (tag != "*" && !self.isElement(&tag)) {
                    return format!("{}: {} is not an element a sheet can style here ({}); the rule is ignored.", sel, tag, self.elementList());
                }
            } else {
                // an outer part: a slide or a section (its heading's id,
                // classes or anchor), or a gallery around its cells
                if !tag.is_empty() && (((tag != "slide" && tag != "section") && (tag != "gallery" && tag != "cell")) && tag != "*") {
                    return format!("{}: the part before the last can be a slide's or section's #id / .class (or slide, section), not {}; the rule is ignored.", sel, tag);
                }
            }
            i += 1;
        }
        String::new()
    }
    pub fn elementList(&self) -> String {
        self.elements.join(&*", ".to_string())
    }
}
