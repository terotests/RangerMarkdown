// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_anchor::MdAnchor;
use crate::md_container::MdContainer;
use crate::md_css_support::MdCssNote;
use crate::md_css_support::MdCssSupport;
use crate::md_node::MdNode;
use crate::md_style::MdStyle;
use crate::md_style::MdTypo;
use crate::rgr::MdText;
use crate::rgr::{CssCascade, CssComputed, CssNode, CssRule, CssSelector, CssSelectorPart, CssSheet, CssValue, EVGColor};

/// ============================================================================
/// MdCss.rgr — a company template, as a stylesheet over a markdown document.
/// ============================================================================
///
/// ```css
/// page   { width: 13.333in; height: 7.5in; padding: 0.65in; }
/// document { font-family: Inter; font-size: 18pt; color: #1f2328; }
/// h1     { font-size: 42pt; }
/// h2     { font-size: 28pt; }
/// heading { font-family: Inter; margin-top: 24pt; margin-bottom: 12pt; }
/// .lead  { font-size: 22pt; }
/// .c3    { column-count: 3; column-gap: 28pt; }
/// ```
///
/// Two companies are then two files, and the same markdown previews as either
/// without being touched — which is a better thing than a PowerPoint template,
/// because a master slide positions BOXES and this says what a level-two
/// heading IS.
///
/// ============================================================================
/// WHAT THIS FILE DECIDES, AND WHAT CssCore DECIDED ALREADY
/// ============================================================================
///
/// The same split `PptxCss` draws, and this is the third binding rather than
/// the third engine — `PLAN_SLIDES.md` §5 and §7.
///
/// `CssCore` read the text, matched the selectors, ranked them by specificity
/// and handed back the properties that apply. It has no idea what any of them
/// MEAN. Everything below is the meaning:
///
///   * which element names a markdown selector may use — `document`, `page`,
///     `deck`, `p`, `h1`…`h6`, `heading`, `a`, `code`, `pre`, `ul`, `ol`,
///     `list`, `li`, `table`, `blockquote`, `hr`, `slide`;
///   * where each property lands in `MdStyle`;
///   * and which ones this layout cannot honour, which are NAMED and counted
///     rather than dropped in a silence that reads as "applied".
///
/// **`page` and `deck`, not `@page` and `@deck`.** The plan wrote the at-rule
/// spelling; `CssCore` deliberately does not read at-rules — it records them as
/// "not read here" so a consumer cannot silently apply what is inside one — and
/// teaching it to would change what every existing consumer sees. An element
/// name costs one character in the sheet and no change to the engine. Recorded
/// here rather than quietly settled.
///
/// **A document with no sheet is byte-identical.** `MdStyle`'s field
/// initialisers are still the defaults; a sheet only ever overwrites what it
/// names. That is what makes this safe to add underneath a layout that is
/// already gated on its own numbers.
///
/// **Per-block classes are resolved separately.** `resolve` answers "what does
/// the sheet say about THIS node", which is what columns and `{.lead}` need; it
/// is a different question from "what is the document set in", and mixing them
/// is how a stylesheet ends up with two cascades.
/// ============================================================================
pub struct MdCss {
    sheet: Rc<RefCell<CssSheet>>,
    /// The text the sheet was parsed from, kept so a caller that has the
    /// resolved sheet can hand the same text to something else — the LAYOUT
    /// needs it, because it asks the sheet a different question and parses its
    /// own copy.
    pub source: String,
    /// Property names the sheet used that this layout cannot honour, each once.
    pub unsupported: Vec<String>,
    /// How many declarations landed, and how many selectors were asked.
    pub applied: i64,
    pub asked: i64,
    /// What the deck-level rules said. Read by the slide layout rather than by
    /// `MdStyle`, because they are not about how a page looks.
    pub splitLevel: i64,
    pub overflow: String,
    pub aspect: String,
    /// What a sheet can say, per selector (`MdCssSupport`): the lint reads
    /// against it and the guide lists it. A host adds the elements it reads
    /// itself (Sliqtly's `chart`, `diagram`, `figure`).
    pub support: Rc<RefCell<MdCssSupport>>,
    /// `--name=value` for the variables every sheet so far declared in
    /// `:root` (or `document`, `page`); `var(--name)` in a later sheet sees
    /// them, so a theme's brand colour is the author's to use
    pub vars: Vec<String>,
    /// what the lint said about the sheets added, in order
    pub notes: Vec<Rc<RefCell<MdCssNote>>>,
    /// The rules about a slide or a section (`#intro h2`, `.dark page`,
    /// `slide.dark p`): every selector whose first part is the heading a
    /// block is under. Kept apart so the layout can lay them over the style
    /// on the slides they are about (`applyLook`) and take them off after.
    scopedSheet: Rc<RefCell<CssSheet>>,
    pub hasScoped: bool,
    /// The rules about a block's OWN class or id (`.note`, `p.note`, `#intro`,
    /// `#erikoisdia h2`'s subject is `h2`, so not that one): what may put a
    /// block on a plate of its own. An element's rule (`h2 { border-color }`
    /// in a theme) is that element's look everywhere, not a box round the
    /// one heading that also has an id.
    ownSheet: Rc<RefCell<CssSheet>>,
    /// while `applyLook` runs: only colours, faces and typography are read,
    /// and every element is asked as being under `lookCtx`
    pub looksOnly: bool,
    lookCtx: Option<Rc<RefCell<CssNode>>>,
    /// Which medium the sheet is read for: "screen" (the stage, the
    /// thumbnails, the PPTX) or "print" (the PDF for a printing house). A
    /// sheet's `@media print { … }` rules count only for print, where they
    /// come after the rest and so win; `@media screen { … }` only on a
    /// screen. `CssCore` reads no at-rules (see its banner), so the blocks
    /// are taken apart here, before it parses what is left.
    pub media: String,
    /// the `@media print` rules alone: what the print page says (its size,
    /// `bleed`, `safe-area`) is read from them and from nothing else
    printSheet: Rc<RefCell<CssSheet>>,
    pub printText: String,
}

impl MdCss {
    pub fn new() -> Self {
        Self {
            sheet: Rc::new(RefCell::new(CssSheet::new())),
            source: String::new(),
            unsupported: Vec::new(),
            applied: 0,
            asked: 0,
            splitLevel: 0,
            overflow: String::new(),
            aspect: String::new(),
            support: Rc::new(RefCell::new(MdCssSupport::new())),
            vars: Vec::new(),
            notes: Vec::new(),
            scopedSheet: Rc::new(RefCell::new(CssSheet::new())),
            hasScoped: false,
            ownSheet: Rc::new(RefCell::new(CssSheet::new())),
            looksOnly: false,
            lookCtx: None,
            media: "screen".to_string(),
            printSheet: Rc::new(RefCell::new(CssSheet::new())),
            printText: String::new(),
        }
    }
    /// whether the selector's subject (its last part) names a class or an id
    fn ownSelector(sel: &Rc<RefCell<CssSelector>>) -> bool {
        let n: i64 = sel.borrow().parts.len() as i64;
        if n == 0 {
            return false;
        }
        if sel.borrow().broken {
            return false;
        }
        let mut subj: Rc<RefCell<CssSelectorPart>> = sel.borrow().parts[(n - 1) as usize].clone();
        if !subj.borrow().id.is_empty() {
            return true;
        }
        return !subj.borrow().classes.is_empty();
    }
    /// more than one part, and the outer ones not a gallery's (`gallery
    /// cell`, `cell caption` are the gallery's own nesting)
    fn scopedSelector(sel: &Rc<RefCell<CssSelector>>) -> bool {
        if (sel.borrow().parts.len() as i64) < 2 {
            return false;
        }
        let mut first: Rc<RefCell<CssSelectorPart>> = sel.borrow().parts[0].clone();
        return first.borrow().tag != "gallery" && first.borrow().tag != "cell";
    }
    /// The slide and sections a block is under, as the sheet sees them: one
    /// `slide` per heading in `heads` (by level, outermost first; empty
    /// places skipped), each with the heading's `{#id}` — or, without one,
    /// its anchor — and its `{.classes}`. Absent when there is no heading.
    fn scopeOf(heads: &Vec<Rc<RefCell<MdNode>>>) -> Option<Rc<RefCell<CssNode>>> {
        let mut out: Option<Rc<RefCell<CssNode>>> = None;
        let mut i: i64 = 0;
        while i < (heads.len() as i64) {
            let mut h: Rc<RefCell<MdNode>> = heads[i as usize].clone();
            if h.borrow().kind == "heading" {
                let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
                n.borrow_mut().tag = "slide".to_string();
                n.borrow_mut().id = MdCss::anchorOf(&h);
                let mut k: i64 = 0;
                while k < h.borrow().classCount() {
                    CssNode::addClass(&n, &h.borrow().classAt(k));
                    k += 1;
                }
                n.borrow_mut().parent = out.clone();
                out = Some(n.clone());
            }
            i += 1;
        }
        out
    }
    /// a heading's `{#id}`, else the anchor its text makes (MdAnchor.slug)
    pub fn anchorOf(h: &Rc<RefCell<MdNode>>) -> String {
        if !h.borrow().nodeId.is_empty() {
            return h.borrow().nodeId.clone();
        }
        return MdAnchor::slug(&h.borrow().plainText());
    }
    /// The element name a node is written as in a sheet. A heading is `h2` AND
    /// `heading`, and both have to match — a sheet says `h1 { font-size }` and
    /// `heading { font-family }` — so the level-specific name is the tag and
    /// the generic one is carried as a class. That is a lie about the DOM and
    /// the truth about what the sheet's author meant.
    pub fn tagOf(node: &Rc<RefCell<MdNode>>) -> String {
        let k: String = node.borrow().kind.clone();
        if k == "heading" {
            return format!("h{}", node.borrow().level);
        }
        if k == "paragraph" {
            return "p".to_string();
        }
        if k == "code_block" {
            return "pre".to_string();
        }
        if k == "block_quote" {
            return "blockquote".to_string();
        }
        if k == "thematic_break" {
            return "hr".to_string();
        }
        if k == "item" {
            return "li".to_string();
        }
        if k == "table" {
            return "table".to_string();
        }
        if k == "list" {
            if node.borrow().listOrdered {
                return "ol".to_string();
            }
            return "ul".to_string();
        }
        k
    }
    /// The inside of a `url(...)`, unquoted. Anything else is left alone: a
    /// gradient or a `none` is not a picture and is better ignored than
    /// half-understood.
    pub fn urlOf(value: &str) -> String {
        let v: String = value.trim().to_string();
        let n: i64 = MdText::str_len(&v) as i64;
        if n < 6 {
            return String::new();
        }
        if MdText::substr(&v, 0, 4) != "url(" {
            return String::new();
        }
        if MdText::char_at(&v, n - 1) != 41 {
            return String::new();
        }
        let inner: String = MdText::substr(&v, 4, n - 1).trim().to_string();
        let m: i64 = MdText::str_len(&inner) as i64;
        if m < 2 {
            return String::new();
        }
        let q: i64 = MdText::char_at(&inner, 0);
        if q == 34 || q == 39 {
            if MdText::char_at(&inner, m - 1) == q {
                return MdText::substr(&inner, 1, m - 1);
            }
        }
        inner
    }
    /// The typography one rule says, unset where it says nothing.
    fn typoOf(got: &Rc<RefCell<CssComputed>>) -> Rc<RefCell<MdTypo>> {
        let mut t: Rc<RefCell<MdTypo>> = Rc::new(RefCell::new(MdTypo::new()));
        if CssComputed::has(&got, "font-weight") {
            let w: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, "font-weight")));
            if w == "normal" || (w == "lighter" || (w == "400" || (w == "300" || (w == "100" || (w == "200" || w == "500"))))) {
                t.borrow_mut().weight = 0;
            }
            if CssValue::isBold(&w) {
                t.borrow_mut().weight = 1;
            }
        }
        if CssComputed::has(&got, "font-style") {
            let fs: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, "font-style")));
            if CssValue::isItalic(&fs) {
                t.borrow_mut().italic = 1;
            }
            if fs == "normal" {
                t.borrow_mut().italic = 0;
            }
        }
        if CssComputed::has(&got, "text-transform") {
            let tt: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, "text-transform")));
            if tt == "uppercase" || (tt == "lowercase" || (tt == "capitalize" || tt == "none")) {
                t.borrow_mut().transform = tt.clone();
            }
        }
        if CssComputed::has(&got, "letter-spacing") {
            let ls: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, "letter-spacing")));
            if ls == "normal" {
                t.borrow_mut().spacing = 0.0;
                t.borrow_mut().spacingSet = true;
            } else {
                let em: i64 = MdText::index_of(&ls, "em");
                if em > 0 && em == (MdText::str_len(&ls) as i64) - 2 {
                    // `0.1em`: a share of the size, kept negative so the
                    // layout knows to multiply (MdLayout.spacingFor)
                    let d: Option<f64> = MdText::substr(&ls, 0, em).trim().to_string().parse::<f64>().ok();
                    if d.is_some() {
                        t.borrow_mut().spacing = 0.0 - d.unwrap();
                        t.borrow_mut().spacingSet = true;
                    }
                } else {
                    let pts: f64 = CssValue::fontPoints(&ls, -9999.0);
                    if pts > -9999.0 {
                        t.borrow_mut().spacing = MdCss::atLeastZero(pts);
                        t.borrow_mut().spacingSet = true;
                    }
                }
            }
        }
        t
    }
    /// letter-spacing in points is kept zero or more: a negative number in the
    /// field means `em` (see typoOf)
    pub fn atLeastZero(v: f64) -> f64 {
        if v < 0.0 {
            return 0.0;
        }
        v
    }
    /// `padding` (one to four lengths) and `padding-top` … `-left`, as
    /// [top, right, bottom, left] in points; -1 where nothing was said
    fn padding(got: &Rc<RefCell<CssComputed>>) -> Vec<f64> {
        let mut out: Vec<f64> = vec![-1.0, -1.0, -1.0, -1.0];
        if CssComputed::has(&got, "padding") {
            let mut words: Vec<String> = CssSheet::splitWhitespace(&CssSheet::tidy(&CssComputed::get(&got, "padding")));
            let n: i64 = words.len() as i64;
            if n > 0 {
                let a: f64 = CssValue::fontPoints(&words[0].clone(), -1.0);
                let mut b: f64 = a;
                let mut c: f64 = a;
                let mut d: f64 = a;
                if n >= 2 {
                    b = CssValue::fontPoints(&words[1].clone(), -1.0);
                    d = b;
                }
                if n >= 3 {
                    c = CssValue::fontPoints(&words[2].clone(), -1.0);
                }
                if n >= 4 {
                    d = CssValue::fontPoints(&words[3].clone(), -1.0);
                }
                out = vec![a, b, c, d];
            }
        }
        let mut sides: Vec<String> = vec!["padding-top".to_string(), "padding-right".to_string(), "padding-bottom".to_string(), "padding-left".to_string()];
        let mut i: i64 = 0;
        while i < 4 {
            let name: String = sides[i as usize].clone();
            if CssComputed::has(&got, &name) {
                out[i as usize] = CssValue::fontPoints(&CssComputed::get(&got, &name), out[i as usize]);
            }
            i += 1;
        }
        out
    }
    /// a border's colour from `border-color` or `border: 2pt solid #7ee0ff`,
    /// "" for none (`border: none`, or nothing said)
    fn borderColour(got: &Rc<RefCell<CssComputed>>) -> String {
        if CssComputed::has(&got, "border-color") {
            let mut c: Rc<RefCell<EVGColor>> = CssValue::color(&CssComputed::get(&got, "border-color"));
            if c.borrow().isSet {
                return MdCss::hexOf(&c);
            }
        }
        if CssComputed::has(&got, "border") {
            let mut words: Vec<String> = CssSheet::splitWhitespace(&CssSheet::tidy(&CssComputed::get(&got, "border")));
            for w in words.iter().cloned() {
                let mut c2: Rc<RefCell<EVGColor>> = CssValue::color(&w);
                if c2.borrow().isSet {
                    return MdCss::hexOf(&c2);
                }
            }
            let low: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, "border")));
            if low != "none" && low != "0" {
                return "#000000".to_string();
            }
        }
        String::new()
    }
    /// …and its width: `border-width`, the length in `border`, else `fallback`
    fn borderWidth(got: &Rc<RefCell<CssComputed>>, fallback: f64) -> f64 {
        if CssComputed::has(&got, "border-width") {
            return CssValue::fontPoints(&CssComputed::get(&got, "border-width"), fallback);
        }
        if CssComputed::has(&got, "border") {
            let mut words: Vec<String> = CssSheet::splitWhitespace(&CssSheet::tidy(&CssComputed::get(&got, "border")));
            for w in words.iter().cloned() {
                let first: i64 = MdText::char_at(&w, 0);
                if (first >= 48 && first <= 57) || first == 46 {
                    return CssValue::fontPoints(&w, fallback);
                }
            }
        }
        fallback
    }
    /// "center" / "end" / "start" from what CSS says about it; "" for nothing.
    pub fn alignWord(v: &str) -> String {
        let t: String = CssSheet::lower(&CssSheet::tidy(v));
        if t.is_empty() {
            return String::new();
        }
        if t == "center" || t == "centre" {
            return "center".to_string();
        }
        if (t == "right" || t == "end") || t == "flex-end" {
            return "end".to_string();
        }
        "start".to_string()
    }
    /// `"text"` or `'text'` as text; anything else as it was written.
    pub fn unquote(v: &str) -> String {
        let t: String = v.trim().to_string();
        let n: i64 = MdText::str_len(&t) as i64;
        if n >= 2 {
            let q: i64 = MdText::char_at(&t, 0);
            if (q == 34 || q == 39) && MdText::char_at(&t, n - 1) == q {
                return MdText::substr(&t, 1, n - 1);
            }
        }
        if t == "none" {
            return String::new();
        }
        t
    }
    /// `#RRGGBB`, or `#RRGGBBAA` when the colour lets what is behind it show
    /// through — `#151c4833` kept its alpha nowhere before this.
    fn hexOf(c: &Rc<RefCell<EVGColor>>) -> String {
        let hex: String = EVGColor::toHexString(&c);
        let a: f64 = EVGColor::alpha(&c);
        if a >= 0.999 {
            return hex;
        }
        let digits: String = "0123456789ABCDEF";
        let v: i64 = (a * 255.0 + 0.5).floor() as i64;
        let hi: i64 = v / 16;
        let lo: i64 = v % 16;
        format!("{}{}{}", hex, char::from_u32((MdText::char_at(&digits, hi)) as u32).unwrap_or('\0'), char::from_u32((MdText::char_at(&digits, lo)) as u32).unwrap_or('\0'))
    }
    pub fn listed(csv: &str, name: &str) -> bool {
        let mut parts: Vec<String> = csv.split(",").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut i: i64 = 0;
        while i < (parts.len() as i64) {
            if parts[i as usize].clone() == name {
                return true;
            }
            i += 1;
        }
        false
    }
    /// ----------------------------------------------------------------------
    /// @media, and `:nth(n)`
    /// ----------------------------------------------------------------------
    /// The sheet in three: [everything outside an `@media` block, the inside
    /// of every `@media print` block, the inside of every `@media screen`
    /// block]. Another medium's block (`@media (max-width…)`) is left in the
    /// first part, where `CssCore` reports it as an at-rule it does not read.
    pub fn splitMedia(css: &str) -> Vec<String> {
        let src: String = CssSheet::stripComments(css);
        let mut rest: String = String::new();
        let mut on_print: String = String::new();
        let mut on_screen: String = String::new();
        let n: i64 = MdText::str_len(&src) as i64;
        let mut i: i64 = 0;
        let mut from: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&src, i);
            if c == 64 {
                let open: i64 = CssSheet::findChar(&src, i, 123);
                if open < 0 {
                    i = n;
                } else {
                    let head: String = CssSheet::lower(&CssSheet::tidy(&MdText::substr(&src, i, open)));
                    let close: i64 = CssSheet::matchingBrace(&src, open);
                    let mut which: String = "";
                    if MdText::index_of(&head, "@media") == 0 {
                        let q: String = MdText::substr(&head, 6, (MdText::str_len(&head) as i64)).trim().to_string();
                        if q == "print" {
                            which = "print";
                        }
                        if q == "screen" {
                            which = "screen";
                        }
                    }
                    if !which.is_empty() {
                        rest = format!("{}{}", rest, MdText::substr(&src, from, i));
                        let mut end: i64 = close;
                        if end < 0 {
                            end = n;
                        }
                        let inner: String = MdText::substr(&src, open + 1, end);
                        if which == "print" {
                            on_print = format!("{}{}\n", on_print, inner);
                        } else {
                            on_screen = format!("{}{}\n", on_screen, inner);
                        }
                        if close < 0 {
                            i = n;
                        } else {
                            i = close + 1;
                        }
                        from = i;
                    } else {
                        // another at-rule: skip its block whole, so a `{`
                        // inside it is not taken for the next rule's
                        if close < 0 {
                            i = n;
                        } else {
                            i = close + 1;
                        }
                    }
                }
            } else {
                if c == 123 {
                    let close2: i64 = CssSheet::matchingBrace(&src, i);
                    if close2 < 0 {
                        i = n;
                    } else {
                        i = close2 + 1;
                    }
                } else {
                    i += 1;
                }
            }
        }
        if from < n {
            rest = format!("{}{}", rest, MdText::substr(&src, from, n));
        }
        let mut out: Vec<String> = Vec::new();
        out.push(rest);
        out.push(on_print);
        out.push(on_screen);
        out
    }
    /// `cell:nth(3)` — the third cell of a gallery — is read as the class
    /// `MdCss.nthClass(3)`, which every gallery cell carries for its own
    /// place. A pseudo-class weighs what a class weighs in CSS, so the
    /// cascade ranks it exactly as it would rank `:nth-child(3)`. Only in
    /// selectors: a declaration's value is left as it was written.
    pub fn desugar(css: &str) -> String {
        if MdText::index_of(&css, ":nth(") < 0 {
            return css.to_string();
        }
        let mut out: String = String::new();
        let n: i64 = MdText::str_len(&css) as i64;
        let mut depth: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&css, i);
            if c == 123 {
                depth += 1;
            }
            if c == 125 {
                depth -= 1;
            }
            let mut took: bool = false;
            if depth == 0 && (c == 58 && i + 5 < n) {
                if MdText::substr(&css, i, i + 5) == ":nth(" {
                    let close: i64 = CssSheet::findChar(css, i + 5, 41);
                    if close > 0 {
                        let arg: String = MdText::substr(&css, i + 5, close).trim().to_string();
                        let num: Option<i64> = arg.parse::<i64>().ok();
                        if num.is_some() {
                            out = format!("{}.{}", out, MdCss::nthClass(num.unwrap()));
                            i = close + 1;
                            took = true;
                        }
                    }
                }
            }
            if !took {
                // the unit itself (`strfromcode` of a byte breaks "ä" on Go)
                out = format!("{}{}", out, MdText::substr(&css, i, i + 1));
                i += 1;
            }
        }
        out
    }
    /// The class a gallery's `n`th cell (from 1) carries for `cell:nth(n)`.
    pub fn nthClass(n: i64) -> String {
        format!("nth--{}", n)
    }
    pub fn text(&self) -> String {
        self.source.clone()
    }
    /// The medium to read the sheet for, parsed again from its text when it
    /// changes.
    pub fn setMedia(&mut self, m: String) {
        if m == self.media {
            return;
        }
        self.media = m.clone();
        let was: String = self.source.clone();
        self.source = String::new();
        self.printText = String::new();
        self.sheet = Rc::new(RefCell::new(CssSheet::new()));
        self.printSheet = Rc::new(RefCell::new(CssSheet::new()));
        self.scopedSheet = Rc::new(RefCell::new(CssSheet::new()));
        self.hasScoped = false;
        self.ownSheet = Rc::new(RefCell::new(CssSheet::new()));
        self.unsupported.clear();
        let mut no_vars: Vec<String> = Vec::new();
        self.vars = no_vars.clone();
        let mut no_notes: Vec<Rc<RefCell<MdCssNote>>> = Vec::new();
        self.notes = no_notes.clone();
        self.addStyleSheet(&was);
    }
    pub fn hasPrint(&self) -> bool {
        !self.printText.is_empty()
    }
    pub fn addStyleSheet(&mut self, raw: &str) {
        self.source = format!("{}{}", self.source, raw);
        let mut said: Vec<Rc<RefCell<MdCssNote>>> = self.support.borrow().lint(raw, &self.vars.clone());
        for nt in said.iter().cloned() {
            self.notes.push(nt.clone());
        }
        // `var(--brand)` becomes the colour it names before anything reads
        // the text: every reader after this sees plain values
        let mut own: Vec<String> = MdCssSupport::varPairs(raw);
        for vp in own.iter().cloned() {
            self.vars.push(vp.clone());
        }
        let css: String = MdCssSupport::substitute(raw, &self.vars.clone());
        let mut parts: Vec<String> = MdCss::splitMedia(&css);
        let screen_text: String = MdCss::desugar(&parts[0].clone());
        let print_part: String = MdCss::desugar(&parts[1].clone());
        let screen_only: String = MdCss::desugar(&parts[2].clone());
        self.printText = format!("{}{}", self.printText, print_part);
        CssSheet::parse(&self.printSheet.clone(), &print_part);
        CssSheet::parse(&self.sheet.clone(), &screen_text);
        self.keepScoped(&screen_text);
        self.keepOwn(&screen_text);
        if self.media == "print" {
            CssSheet::parse(&self.sheet.clone(), &print_part);
            self.keepScoped(&print_part);
            self.keepOwn(&print_part);
        } else {
            CssSheet::parse(&self.sheet.clone(), &screen_only);
            self.keepScoped(&screen_only);
            self.keepOwn(&screen_only);
        }
        // The deck rules are read here rather than in `applyTo`, because they
        // are not about how a page LOOKS and a caller that only wants the
        // slide rules should not have to resolve a style to get them.
        self.readDeck();
    }
    /// The rules of `css` whose selectors are under a slide or a section,
    /// into `scopedSheet` (in the order they are written).
    pub fn keepScoped(&mut self, css: &str) {
        if css.trim().to_string().is_empty() {
            return;
        }
        let mut tmp: Rc<RefCell<CssSheet>> = Rc::new(RefCell::new(CssSheet::new()));
        CssSheet::parse(&tmp, css);
        let __it_r = tmp.borrow().rules.clone();
        for r in __it_r.iter().cloned() {
            let mut keep: Vec<Rc<RefCell<CssSelector>>> = Vec::new();
            let __it_sel = r.borrow().selectors.clone();
            for sel in __it_sel.iter().cloned() {
                if MdCss::scopedSelector(&sel) {
                    sel.borrow_mut().order = self.scopedSheet.borrow().counter;
                    let counter_v = self.scopedSheet.borrow().counter + 1;
                    self.scopedSheet.borrow_mut().counter = counter_v;
                    keep.push(sel.clone());
                }
            }
            if !keep.is_empty() {
                let mut nr: Rc<RefCell<CssRule>> = Rc::new(RefCell::new(CssRule::new()));
                nr.borrow_mut().selectors = keep.clone();
                nr.borrow_mut().decls = r.borrow().decls.clone();
                self.scopedSheet.borrow_mut().rules.push(nr.clone());
                self.hasScoped = true;
            }
        }
    }
    /// The rules of `css` whose subject names a class or an id, into
    /// `ownSheet` (in the order they are written).
    pub fn keepOwn(&self, css: &str) {
        if css.trim().to_string().is_empty() {
            return;
        }
        let mut tmp: Rc<RefCell<CssSheet>> = Rc::new(RefCell::new(CssSheet::new()));
        CssSheet::parse(&tmp, css);
        let __it_r = tmp.borrow().rules.clone();
        for r in __it_r.iter().cloned() {
            let mut keep: Vec<Rc<RefCell<CssSelector>>> = Vec::new();
            let __it_sel = r.borrow().selectors.clone();
            for sel in __it_sel.iter().cloned() {
                if MdCss::ownSelector(&sel) {
                    sel.borrow_mut().order = self.ownSheet.borrow().counter;
                    let counter_v = self.ownSheet.borrow().counter + 1;
                    self.ownSheet.borrow_mut().counter = counter_v;
                    keep.push(sel.clone());
                }
            }
            if !keep.is_empty() {
                let mut nr: Rc<RefCell<CssRule>> = Rc::new(RefCell::new(CssRule::new()));
                nr.borrow_mut().selectors = keep.clone();
                nr.borrow_mut().decls = r.borrow().decls.clone();
                self.ownSheet.borrow_mut().rules.push(nr.clone());
            }
        }
    }
    /// ----------------------------------------------------------------------
    /// A slide's own rules, over the style
    /// ----------------------------------------------------------------------
    ///
    /// `#intro h2 { color }`, `.dark page { background-color }`: what the
    /// scoped rules say about every element under `ctx`, written into
    /// `style` — colours, faces, backgrounds and typography only (see
    /// `MdCssSupport.scopedOk`). The layout puts the style's look back after
    /// (`MdStyle.setLook`). Answers whether there was anything to apply.
    fn applyLook(&mut self, style: &Rc<RefCell<MdStyle>>, ctx: Option<Rc<RefCell<CssNode>>>) -> bool {
        if !self.hasScoped || ctx.is_none() {
            return false;
        }
        let mut keep: Rc<RefCell<CssSheet>> = self.sheet.clone().clone();
        let keep_applied: i64 = self.applied;
        let keep_asked: i64 = self.asked;
        self.sheet = self.scopedSheet.clone();
        self.looksOnly = true;
        self.lookCtx = ctx.clone();
        self.page(style, &self.forTag("page".to_string()));
        self.body(style, &self.forTag("document".to_string()));
        self.body(style, &self.forTag("body".to_string()));
        self.para(style, &self.forTag("p".to_string()));
        self.headings(style);
        self.code(style, &self.forTag("code".to_string()));
        self.mark(style, &self.forTag("mark".to_string()));
        self.kbd(style, &self.forTag("kbd".to_string()));
        self.link(style, &self.forTag("a".to_string()));
        self.rule(style, &self.forTag("hr".to_string()));
        self.quote(style, &self.forTag("blockquote".to_string()));
        self.container(style, &self.forTag("container".to_string()));
        self.list(style, &self.forTag("list".to_string()));
        self.item(style, &self.forTag("li".to_string()));
        self.table(style, &self.forTag("table".to_string()));
        self.tableHead(style, &self.forTag("th".to_string()));
        self.picture(style, &self.forTag("img".to_string()));
        self.furniture(style, &self.forTag("header".to_string()), true);
        self.furniture(style, &self.forTag("footer".to_string()), false);
        let mut none: Option<Rc<RefCell<CssNode>>> = None;
        self.lookCtx = none.clone();
        self.looksOnly = false;
        self.sheet = keep.clone();
        self.applied = keep_applied;
        self.asked = keep_asked;
        true
    }
    /// What the sheet says about one block under a slide or section (`ctx`,
    /// from `scopeOf`): the same as `resolve`, with `#intro .lead` able to
    /// match.
    fn resolveIn(&mut self, node: &Rc<RefCell<MdNode>>, ctx: Option<Rc<RefCell<CssNode>>>) -> Rc<RefCell<CssComputed>> {
        self.resolveAs(node, MdCss::tagOf(node), ctx.clone())
    }
    /// …as element `tag` (a list is `ul` / `ol` and also `list`)
    fn resolveAs(&mut self, node: &Rc<RefCell<MdNode>>, tag: String, ctx: Option<Rc<RefCell<CssNode>>>) -> Rc<RefCell<CssComputed>> {
        let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        n.borrow_mut().tag = tag.clone();
        n.borrow_mut().id = node.borrow().nodeId.clone();
        let mut i: i64 = 0;
        while i < node.borrow().classCount() {
            CssNode::addClass(&n, &node.borrow().classAt(i));
            i += 1;
        }
        n.borrow_mut().parent = ctx.clone();
        self.asked += 1;
        CssCascade::resolve(&self.sheet.clone(), &n)
    }
    /// What only the block's own classes and id say about it (`ownSheet`):
    /// whether it is a box, and that box's colour, border and padding.
    fn resolveOwn(&self, node: &Rc<RefCell<MdNode>>, ctx: Option<Rc<RefCell<CssNode>>>) -> Rc<RefCell<CssComputed>> {
        let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        n.borrow_mut().tag = MdCss::tagOf(node);
        n.borrow_mut().id = node.borrow().nodeId.clone();
        let mut i: i64 = 0;
        while i < node.borrow().classCount() {
            CssNode::addClass(&n, &node.borrow().classAt(i));
            i += 1;
        }
        n.borrow_mut().parent = ctx.clone();
        CssCascade::resolve(&self.ownSheet.clone(), &n)
    }
    pub fn readDeck(&mut self) {
        self.deck(&self.forTag("deck".to_string()));
    }
    pub fn errorCount(&self) -> i64 {
        return CssSheet::errorCount(&self.sheet);
    }
    pub fn errorAt(&self, i: i64) -> String {
        return CssSheet::errorAt(&self.sheet, i);
    }
    /// What the lint says about `css` were it added now: the notes for an
    /// editor's gutter, with offsets into `css` itself.
    pub fn lintText(&self, css: &str) -> Vec<Rc<RefCell<MdCssNote>>> {
        return self.support.borrow().lint(css, &self.vars.clone());
    }
    pub fn noteCount(&self) -> i64 {
        self.notes.len() as i64
    }
    pub fn noteAt(&self, i: i64) -> String {
        let mut n: Rc<RefCell<MdCssNote>> = self.notes[i as usize].clone();
        return n.borrow().message.clone();
    }
    pub fn unsupportedCount(&self) -> i64 {
        self.unsupported.len() as i64
    }
    pub fn unsupportedAt(&self, i: i64) -> String {
        self.unsupported[i as usize].clone()
    }
    pub fn note(&mut self, name: &str) {
        let mut i: i64 = 0;
        while i < (self.unsupported.len() as i64) {
            if self.unsupported[i as usize].clone() == name {
                return;
            }
            i += 1;
        }
        self.unsupported.push(name.to_string());
    }
    /// ----------------------------------------------------------------------
    /// The document's own style
    /// ----------------------------------------------------------------------
    ///
    /// Each element name is asked once and what it said is written into the
    /// fields it is about. The order is the order a reader would expect a
    /// cascade to run in: the page, then the document's text, then the things
    /// inside it.
    pub fn applyTo(&mut self, style: &Rc<RefCell<MdStyle>>) {
        self.applied = 0;
        self.asked = 0;
        self.page(style, &self.forTag("page".to_string()));
        self.body(style, &self.forTag("document".to_string()));
        // `body` says the same thing, because that is what somebody who has
        // written CSS before types — and a rule that is silently ignored is
        // worse than one that is refused by name: the page simply looks
        // unchanged and there is nothing to read about why. Second, so it
        // wins where a sheet says both.
        self.body(style, &self.forTag("body".to_string()));
        self.para(style, &self.forTag("p".to_string()));
        self.headings(style);
        self.code(style, &self.forTag("code".to_string()));
        self.mark(style, &self.forTag("mark".to_string()));
        self.kbd(style, &self.forTag("kbd".to_string()));
        self.link(style, &self.forTag("a".to_string()));
        self.rule(style, &self.forTag("hr".to_string()));
        self.quote(style, &self.forTag("blockquote".to_string()));
        self.container(style, &self.forTag("container".to_string()));
        self.list(style, &self.forTag("list".to_string()));
        self.item(style, &self.forTag("li".to_string()));
        self.table(style, &self.forTag("table".to_string()));
        self.tableHead(style, &self.forTag("th".to_string()));
        self.picture(style, &self.forTag("img".to_string()));
        self.furniture(style, &self.forTag("header".to_string()), true);
        self.furniture(style, &self.forTag("footer".to_string()), false);
        self.deck(&self.forTag("deck".to_string()));
    }
    fn forTag(&mut self, tag: String) -> Rc<RefCell<CssComputed>> {
        let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        n.borrow_mut().tag = tag.clone();
        n.borrow_mut().parent = self.lookCtx.clone();
        self.asked += 1;
        CssCascade::resolve(&self.sheet.clone(), &n)
    }
    /// What the sheet says about one block of the document — its element name,
    /// its `{.classes}` and its `{#id}`. This is the question columns and
    /// `{.lead}` ask; the answer is handed back rather than written anywhere,
    /// because a block's style belongs to the block.
    fn resolve(&mut self, node: &Rc<RefCell<MdNode>>) -> Rc<RefCell<CssComputed>> {
        let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        n.borrow_mut().tag = MdCss::tagOf(node);
        n.borrow_mut().id = node.borrow().nodeId.clone();
        let mut i: i64 = 0;
        while i < node.borrow().classCount() {
            CssNode::addClass(&n, &node.borrow().classAt(i));
            i += 1;
        }
        self.asked += 1;
        CssCascade::resolve(&self.sheet.clone(), &n)
    }
    /// ----------------------------------------------------------------------
    /// …and what each property means
    /// ----------------------------------------------------------------------
    fn page(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_689 = self.len(got, "width", style.borrow().pageWidth.clone());
        style.borrow_mut().pageWidth = _tmp_689;
        let _tmp_691 = self.len(got, "height", style.borrow().pageHeight.clone());
        style.borrow_mut().pageHeight = _tmp_691;
        let _tmp_693 = self.len(got, "padding", style.borrow().margin.clone());
        style.borrow_mut().margin = _tmp_693;
        let _tmp_695 = self.color(got, "background-color", &style.borrow().pageBackground.clone());
        style.borrow_mut().pageBackground = _tmp_695;
        // `url(logo.png)` — the name of a picture in the byte store, not a
        // path on anybody's disk. `Vfs.resolve` decides what it means, the
        // same function that answers for `![](x)` and `r:embed`, because four
        // copies of that arithmetic would be four answers to what `../` means.
        let _tmp_697 = MdCss::urlOf(&self.raw(got, "background-image"));
        style.borrow_mut().pageBackgroundImage = _tmp_697;
        self.rest(got,
            "width,height,padding,background-color,background-image,bleed,safe-area"
        );
    }
    fn body(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_699 = self.family(got, "font-family", &style.borrow().bodyFamily.clone());
        style.borrow_mut().bodyFamily = _tmp_699;
        let bodySize_v = self.size(got, "font-size", style.borrow().bodySize);
        style.borrow_mut().bodySize = bodySize_v;
        let _tmp_702 = self.ratio(got, "line-height", style.borrow().lineHeight.clone());
        style.borrow_mut().lineHeight = _tmp_702;
        let _tmp_704 = self.color(got, "color", &style.borrow().textColor.clone());
        style.borrow_mut().textColor = _tmp_704;
        self.typo(style, "document", got);
        self.rest(got, &format!("font-family,font-size,line-height,color,{}", MdCssSupport::TYPO()).clone());
    }
    fn para(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_706 = self.len(got, "margin-bottom", style.borrow().paraSpacing.clone());
        style.borrow_mut().paraSpacing = _tmp_706;
        let _tmp_708 = self.color(got, "color", &style.borrow().paraColor.clone());
        style.borrow_mut().paraColor = _tmp_708;
        self.typo(style, "p", got);
        self.rest(got, &format!("margin-bottom,color,text-align,{}", MdCssSupport::TYPO()).clone());
    }
    /// `font-weight`, `font-style`, `text-transform`, `letter-spacing` for
    /// the blocks written as `tag` (MdStyle.typoAt), when the rule says any
    fn typo(&mut self, style: &Rc<RefCell<MdStyle>>, tag: &str, got: &Rc<RefCell<CssComputed>>) {
        let mut t: Rc<RefCell<MdTypo>> = MdCss::typoOf(got);
        if t.borrow().isEmpty() {
            return;
        }
        self.applied += 1;
        let mut into: Rc<RefCell<MdTypo>> = style.borrow_mut().typoAt(tag);
        into.borrow_mut().over(&t);
    }
    /// The six levels have six sizes and ONE family and ONE pair of margins, so
    /// a per-level `margin-top` is a property this layout cannot honour and
    /// says so. `heading` is where a sheet states the shared ones.
    pub fn headings(&mut self, style: &Rc<RefCell<MdStyle>>) {
        let mut shared: Rc<RefCell<CssComputed>> = self.forTag("heading".to_string());
        let _tmp_710 = self.family(&shared, "font-family", &style.borrow().headingFamily.clone());
        style.borrow_mut().headingFamily = _tmp_710;
        let _tmp_712 = self.len(&shared, "margin-top", style.borrow().headingSpaceBefore.clone());
        style.borrow_mut().headingSpaceBefore = _tmp_712;
        let _tmp_714 = self.len(&shared, "margin-bottom", style.borrow().headingSpaceAfter.clone());
        style.borrow_mut().headingSpaceAfter = _tmp_714;
        let h1Size_v = self.size(&shared, "font-size", style.borrow().h1Size);
        style.borrow_mut().h1Size = h1Size_v;
        let _tmp_717 = self.color(&shared, "color", &style.borrow().headingColor.clone());
        style.borrow_mut().headingColor = _tmp_717;
        self.typo(style, "heading", &shared);
        self.rest(&shared, &format!("font-family,font-size,margin-top,margin-bottom,color,text-align,{}", MdCssSupport::TYPO()).clone());
        let mut h1: Rc<RefCell<CssComputed>> = self.forTag("h1".to_string());
        let h1Size_v = self.size(&h1, "font-size", style.borrow().h1Size);
        style.borrow_mut().h1Size = h1Size_v;
        let _tmp_720 = self.color(&h1, "color", &style.borrow().h1Color.clone());
        style.borrow_mut().h1Color = _tmp_720;
        self.typo(style, "h1", &h1);
        self.rest(&h1, &format!("font-size,color,text-align,{}", MdCssSupport::TYPO()).clone());
        let mut h2: Rc<RefCell<CssComputed>> = self.forTag("h2".to_string());
        let h2Size_v = self.size(&h2, "font-size", style.borrow().h2Size);
        style.borrow_mut().h2Size = h2Size_v;
        let _tmp_723 = self.color(&h2, "color", &style.borrow().h2Color.clone());
        style.borrow_mut().h2Color = _tmp_723;
        self.typo(style, "h2", &h2);
        self.rest(&h2, &format!("font-size,color,text-align,{}", MdCssSupport::TYPO()).clone());
        let mut h3: Rc<RefCell<CssComputed>> = self.forTag("h3".to_string());
        let h3Size_v = self.size(&h3, "font-size", style.borrow().h3Size);
        style.borrow_mut().h3Size = h3Size_v;
        let _tmp_726 = self.color(&h3, "color", &style.borrow().h3Color.clone());
        style.borrow_mut().h3Color = _tmp_726;
        self.typo(style, "h3", &h3);
        self.rest(&h3, &format!("font-size,color,text-align,{}", MdCssSupport::TYPO()).clone());
        let mut h4: Rc<RefCell<CssComputed>> = self.forTag("h4".to_string());
        let h4Size_v = self.size(&h4, "font-size", style.borrow().h4Size);
        style.borrow_mut().h4Size = h4Size_v;
        let _tmp_729 = self.color(&h4, "color", &style.borrow().h4Color.clone());
        style.borrow_mut().h4Color = _tmp_729;
        self.typo(style, "h4", &h4);
        self.rest(&h4, &format!("font-size,color,text-align,{}", MdCssSupport::TYPO()).clone());
        let mut h5: Rc<RefCell<CssComputed>> = self.forTag("h5".to_string());
        let h5Size_v = self.size(&h5, "font-size", style.borrow().h5Size);
        style.borrow_mut().h5Size = h5Size_v;
        let _tmp_732 = self.color(&h5, "color", &style.borrow().h5Color.clone());
        style.borrow_mut().h5Color = _tmp_732;
        self.typo(style, "h5", &h5);
        self.rest(&h5, &format!("font-size,color,text-align,{}", MdCssSupport::TYPO()).clone());
        let mut h6: Rc<RefCell<CssComputed>> = self.forTag("h6".to_string());
        let h6Size_v = self.size(&h6, "font-size", style.borrow().h6Size);
        style.borrow_mut().h6Size = h6Size_v;
        let _tmp_735 = self.color(&h6, "color", &style.borrow().h6Color.clone());
        style.borrow_mut().h6Color = _tmp_735;
        self.typo(style, "h6", &h6);
        self.rest(&h6, &format!("font-size,color,text-align,{}", MdCssSupport::TYPO()).clone());
    }
    fn code(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_737 = self.family(got, "font-family", &style.borrow().monoFamily.clone());
        style.borrow_mut().monoFamily = _tmp_737;
        let codeSize_v = self.size(got, "font-size", style.borrow().codeSize);
        style.borrow_mut().codeSize = codeSize_v;
        let _tmp_740 = self.ratio(got, "line-height", style.borrow().codeLineHeight.clone());
        style.borrow_mut().codeLineHeight = _tmp_740;
        let _tmp_742 = self.len(got, "padding", style.borrow().codePadding.clone());
        style.borrow_mut().codePadding = _tmp_742;
        let _tmp_744 = self.color(got, "background-color", &style.borrow().codeBackground.clone());
        style.borrow_mut().codeBackground = _tmp_744;
        let _tmp_746 = self.color(got, "border-color", &style.borrow().codeBorder.clone());
        style.borrow_mut().codeBorder = _tmp_746;
        // the text of a listing that is not a keyword, string, comment or
        // number — and code in a sentence
        let _tmp_748 = self.color(got, "color", &style.borrow().codePlain.clone());
        style.borrow_mut().codePlain = _tmp_748;
        self.rest(got,
            "font-family,font-size,line-height,padding,color,background-color,border-color"
        );
    }
    fn mark(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_750 = self.color(got, "background-color", &style.borrow().markBackground.clone());
        style.borrow_mut().markBackground = _tmp_750;
        let _tmp_752 = self.color(got, "color", &style.borrow().markColor.clone());
        style.borrow_mut().markColor = _tmp_752;
        self.rest(got, "background-color,color");
    }
    fn kbd(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_754 = self.color(got, "background-color", &style.borrow().kbdBackground.clone());
        style.borrow_mut().kbdBackground = _tmp_754;
        let _tmp_756 = self.color(got, "border-color", &style.borrow().kbdBorder.clone());
        style.borrow_mut().kbdBorder = _tmp_756;
        self.rest(got, "background-color,border-color");
    }
    fn link(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_758 = self.color(got, "color", &style.borrow().linkColor.clone());
        style.borrow_mut().linkColor = _tmp_758;
        self.rest(got, "color");
    }
    fn rule(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_760 = self.color(got, "color", &style.borrow().ruleColor.clone());
        style.borrow_mut().ruleColor = _tmp_760;
        self.rest(got, "color");
    }
    fn quote(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_762 = self.color(got, "color", &style.borrow().quoteColor.clone());
        style.borrow_mut().quoteColor = _tmp_762;
        let _tmp_764 = self.color(got, "border-color", &style.borrow().quoteBarColor.clone());
        style.borrow_mut().quoteBarColor = _tmp_764;
        let _tmp_766 = self.len(got, "border-width", style.borrow().quoteBarWidth.clone());
        style.borrow_mut().quoteBarWidth = _tmp_766;
        let _tmp_768 = self.len(got, "padding-left", style.borrow().quoteIndent.clone());
        style.borrow_mut().quoteIndent = _tmp_768;
        let _tmp_770 = self.len(got, "padding-top", style.borrow().quotePadY.clone());
        style.borrow_mut().quotePadY = _tmp_770;
        let _tmp_772 = self.color(got, "background", &style.borrow().quoteBackground.clone());
        style.borrow_mut().quoteBackground = _tmp_772;
        let _tmp_774 = self.color(got, "background-color", &style.borrow().quoteBackground.clone());
        style.borrow_mut().quoteBackground = _tmp_774;
        self.typo(style, "blockquote", got);
        self.rest(got, &format!("color,border-color,border-width,padding-left,padding-top,background,background-color,{}", MdCssSupport::TYPO()).clone());
    }
    /// the plate under a `{container=…}` block
    fn container(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_776 = self.color(got, "background", &style.borrow().containerBackground.clone());
        style.borrow_mut().containerBackground = _tmp_776;
        let _tmp_778 = self.color(got, "background-color", &style.borrow().containerBackground.clone());
        style.borrow_mut().containerBackground = _tmp_778;
        let _tmp_780 = self.len(got, "border-radius", style.borrow().containerRadius.clone());
        style.borrow_mut().containerRadius = _tmp_780;
        if CssComputed::has(&got, "box-shadow") {
            let _tmp_782 = MdContainer::shadowColour(&self.raw(got, "box-shadow"));
            style.borrow_mut().containerShadow = _tmp_782;
        }
        let mut pads: Vec<f64> = MdCss::padding(got);
        if self.looksOnly {
            pads = vec![-1.0, -1.0, -1.0, -1.0];
        }
        if pads[0] >= 0.0 {
            style.borrow_mut().containerPadY = pads[0];
            self.applied += 1;
        }
        if pads[3] >= 0.0 {
            style.borrow_mut().containerPadX = pads[3];
        }
        let bc: String = MdCss::borderColour(got);
        if !bc.is_empty() && !self.looksOnly {
            style.borrow_mut().containerBorder = bc.clone();
            style.borrow_mut().containerBorderWidth = MdCss::borderWidth(got, 1.0);
            self.applied += 1;
        }
        self.rest(got,
            "background,background-color,border-radius,box-shadow,padding,padding-top,padding-right,padding-bottom,padding-left,border,border-color,border-width,border-style"
        );
    }
    fn list(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_785 = self.len(got, "padding-left", style.borrow().listIndent.clone());
        style.borrow_mut().listIndent = _tmp_785;
        let _tmp_787 = self.len(got, "margin-left", style.borrow().markerGap.clone());
        style.borrow_mut().markerGap = _tmp_787;
        let _tmp_789 = self.color(got, "color", &style.borrow().markerColor.clone());
        style.borrow_mut().markerColor = _tmp_789;
        self.rest(got, "padding-left,margin-left,color");
    }
    fn item(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_791 = self.len(got, "margin-bottom", style.borrow().listTightSpacing.clone());
        style.borrow_mut().listTightSpacing = _tmp_791;
        self.typo(style, "li", got);
        self.rest(got, &format!("margin-bottom,{}", MdCssSupport::TYPO()).clone());
    }
    /// `th { background-color; color }`: the header row. `table {
    /// background-color }` says the same for the row, and `th` wins.
    fn tableHead(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_793 = self.color(got, "background-color", &style.borrow().tableHeadBackground.clone());
        style.borrow_mut().tableHeadBackground = _tmp_793;
        let _tmp_795 = self.color(got, "color", &style.borrow().tableHeadColor.clone());
        style.borrow_mut().tableHeadColor = _tmp_795;
        self.rest(got, "background-color,color");
    }
    fn table(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let _tmp_797 = self.color(got, "border-color", &style.borrow().tableBorder.clone());
        style.borrow_mut().tableBorder = _tmp_797;
        let _tmp_799 = self.color(got, "background-color", &style.borrow().tableHeadBackground.clone());
        style.borrow_mut().tableHeadBackground = _tmp_799;
        let _tmp_801 = self.len(got, "padding-left", style.borrow().cellPadX.clone());
        style.borrow_mut().cellPadX = _tmp_801;
        let _tmp_803 = self.len(got, "padding-top", style.borrow().cellPadY.clone());
        style.borrow_mut().cellPadY = _tmp_803;
        let tableSize_v = self.size(got, "font-size", style.borrow().tableSize);
        style.borrow_mut().tableSize = tableSize_v;
        // cells coloured by their text, `MdCellTones`' mapping
        if CssComputed::has(&got, "cell-tones") {
            style.borrow_mut().tableCellTones = self.raw(got, "cell-tones").trim().to_string();
        }
        self.rest(got,
            "border-color,background-color,padding-left,padding-top,font-size,cell-tones"
        );
    }
    /// `img { justify-content: center; max-height: 300px }`
    fn picture(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>) {
        let mut a: String = MdCss::alignWord(&self.raw(got, "justify-content"));
        if a.is_empty() {
            a = MdCss::alignWord(&self.raw(got, "text-align"));
        }
        if !a.is_empty() {
            style.borrow_mut().imgAlign = a.clone();
        }
        let _tmp_808 = self.len(got, "max-height", style.borrow().imgMaxHeight.clone());
        style.borrow_mut().imgMaxHeight = _tmp_808;
        self.rest(got, "justify-content,text-align,max-height");
    }
    /// `header { content: "ACME · {page}/{pages}"; background-image:
    /// url(logo.png); text-align: right; font-size: 10pt; color: #777;
    /// height: 24px }` — and `footer` the same.
    fn furniture(&mut self, style: &Rc<RefCell<MdStyle>>, got: &Rc<RefCell<CssComputed>>, top: bool) {
        let text: String = MdCss::unquote(&self.raw(got, "content"));
        let img: String = MdCss::urlOf(&self.raw(got, "background-image"));
        let al: String = MdCss::alignWord(&self.raw(got, "text-align"));
        if top {
            if !text.is_empty() {
                style.borrow_mut().headerText = text.clone();
            }
            if !img.is_empty() {
                style.borrow_mut().headerImage = img.clone();
            }
            if !al.is_empty() {
                style.borrow_mut().headerAlign = al.clone();
            }
            let _tmp_813 = self.len(got, "font-size", style.borrow().headerSize.clone());
            style.borrow_mut().headerSize = _tmp_813;
            let _tmp_815 = self.color(got, "color", &style.borrow().headerColor.clone());
            style.borrow_mut().headerColor = _tmp_815;
            let _tmp_817 = self.len(got, "height", style.borrow().headerImageH.clone());
            style.borrow_mut().headerImageH = _tmp_817;
            let _tmp_819 = self.slot(got, "content-left", &style.borrow().headerLeft.clone());
            style.borrow_mut().headerLeft = _tmp_819;
            let _tmp_821 = self.slot(got, "content-center", &style.borrow().headerCenter.clone());
            style.borrow_mut().headerCenter = _tmp_821;
            let _tmp_823 = self.slot(got, "content-right", &style.borrow().headerRight.clone());
            style.borrow_mut().headerRight = _tmp_823;
            let _tmp_825 = self.boldOf(got, style.borrow().headerBold.clone());
            style.borrow_mut().headerBold = _tmp_825;
            let _tmp_827 = self.color(got, "background-color", &style.borrow().headerBackground.clone());
            style.borrow_mut().headerBackground = _tmp_827;
            let _tmp_829 = self.color(got, "border-color", &style.borrow().headerRule.clone());
            style.borrow_mut().headerRule = _tmp_829;
        } else {
            if !text.is_empty() {
                style.borrow_mut().footerText = text.clone();
            }
            if !img.is_empty() {
                style.borrow_mut().footerImage = img.clone();
            }
            if !al.is_empty() {
                style.borrow_mut().footerAlign = al.clone();
            }
            let _tmp_831 = self.len(got, "font-size", style.borrow().footerSize.clone());
            style.borrow_mut().footerSize = _tmp_831;
            let _tmp_833 = self.color(got, "color", &style.borrow().footerColor.clone());
            style.borrow_mut().footerColor = _tmp_833;
            let _tmp_835 = self.len(got, "height", style.borrow().footerImageH.clone());
            style.borrow_mut().footerImageH = _tmp_835;
            let _tmp_837 = self.slot(got, "content-left", &style.borrow().footerLeft.clone());
            style.borrow_mut().footerLeft = _tmp_837;
            let _tmp_839 = self.slot(got, "content-center", &style.borrow().footerCenter.clone());
            style.borrow_mut().footerCenter = _tmp_839;
            let _tmp_841 = self.slot(got, "content-right", &style.borrow().footerRight.clone());
            style.borrow_mut().footerRight = _tmp_841;
            let _tmp_843 = self.boldOf(got, style.borrow().footerBold.clone());
            style.borrow_mut().footerBold = _tmp_843;
            let _tmp_845 = self.color(got, "background-color", &style.borrow().footerBackground.clone());
            style.borrow_mut().footerBackground = _tmp_845;
            let _tmp_847 = self.color(got, "border-color", &style.borrow().footerRule.clone());
            style.borrow_mut().footerRule = _tmp_847;
        }
        self.rest(got,
            "content,content-left,content-center,content-right,background-image,background-color,border-color,font-weight,text-align,font-size,color,height"
        );
    }
    /// `content-left: "ACME"` — one of an edge's three places.
    fn slot(&mut self, got: &Rc<RefCell<CssComputed>>, name: &str, fallback: &str) -> String {
        if false == CssComputed::has(&got, name) {
            return fallback.to_string();
        }
        MdCss::unquote(&self.raw(got, name))
    }
    /// `font-weight: bold` (or 600 and over) for an edge's text.
    fn boldOf(&mut self, got: &Rc<RefCell<CssComputed>>, fallback: bool) -> bool {
        if false == CssComputed::has(&got, "font-weight") {
            return fallback;
        }
        let _tmp_850 = CssSheet::tidy(&self.raw(got, "font-weight"));
        let w: String = CssSheet::lower(&_tmp_850);
        if w == "bold" || w == "bolder" {
            return true;
        }
        let n: Option<i64> = w.parse::<i64>().ok();
        if n.is_some() {
            return n.unwrap() >= 600;
        }
        false
    }
    /// `deck { split-level: 2; overflow: split; aspect-ratio: 16 / 9 }` —
    /// carried, not applied. The slide layout reads these; a page does not
    /// have them and `MdStyle` should not grow fields it never uses.
    fn deck(&mut self, got: &Rc<RefCell<CssComputed>>) {
        if CssComputed::has(&got, "split-level") {
            let v: String = CssComputed::get(&got, "split-level");
            let n: Option<i64> = CssSheet::tidy(&v).parse::<i64>().ok();
            self.splitLevel = if n.is_some() { n.unwrap() } else { 0 };
            self.applied += 1;
        }
        if CssComputed::has(&got, "overflow") {
            let _tmp_853 = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, "overflow")));
            self.overflow = _tmp_853;
            self.applied += 1;
        }
        if CssComputed::has(&got, "aspect-ratio") {
            self.aspect = CssSheet::tidy(&CssComputed::get(&got, "aspect-ratio"));
            self.applied += 1;
        }
        self.rest(got, "split-level,overflow,aspect-ratio,crop-marks");
    }
    /// ----------------------------------------------------------------------
    /// Reading one value, and counting what was read
    /// ----------------------------------------------------------------------
    fn len(&mut self, got: &Rc<RefCell<CssComputed>>, name: &str, fallback: f64) -> f64 {
        if false == CssComputed::has(&got, name) {
            return fallback;
        }
        if self.looksOnly {
            return fallback;
        }
        self.applied += 1;
        return CssValue::fontPoints(&CssComputed::get(&got, name), fallback);
    }
    fn size(&mut self, got: &Rc<RefCell<CssComputed>>, name: &str, fallback: f64) -> f64 {
        self.len(got, name, fallback)
    }
    /// `line-height: 1.45` is a ratio and `line-height: 16pt` is a length. Both
    /// are written; the field is a ratio, so a length is divided by the size it
    /// was stated against — which this layer does not know, so a length here is
    /// the one thing reported rather than guessed.
    fn ratio(&mut self, got: &Rc<RefCell<CssComputed>>, name: &str, fallback: f64) -> f64 {
        if false == CssComputed::has(&got, name) {
            return fallback;
        }
        if self.looksOnly {
            return fallback;
        }
        let v: String = CssSheet::tidy(&CssComputed::get(&got, name));
        if CssValue::isBareNumber(&v) {
            self.applied += 1;
            let d: Option<f64> = v.parse::<f64>().ok();
            return if d.is_some() { d.unwrap() } else { fallback };
        }
        self.note(&format!("{} as a length", name));
        fallback
    }
    fn family(&mut self, got: &Rc<RefCell<CssComputed>>, name: &str, fallback: &str) -> String {
        if false == CssComputed::has(&got, name) {
            return fallback.to_string();
        }
        let f: String = CssValue::firstFamily(&CssComputed::get(&got, name));
        if f.is_empty() {
            return fallback.to_string();
        }
        self.applied += 1;
        f
    }
    /// The declaration as it was WRITTEN, for the properties this file has to
    /// read itself. Everything else goes through `CssValue`, which knows about
    /// lengths and colours and nothing about `url(...)`.
    fn raw(&mut self, got: &Rc<RefCell<CssComputed>>, name: &str) -> String {
        if false == CssComputed::has(&got, name) {
            return String::new();
        }
        self.applied += 1;
        return CssComputed::get(&got, name);
    }
    fn color(&mut self, got: &Rc<RefCell<CssComputed>>, name: &str, fallback: &str) -> String {
        if false == CssComputed::has(&got, name) {
            return fallback.to_string();
        }
        let word: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, name)));
        if word == "none" || word == "transparent" {
            self.applied += 1;
            return "transparent".to_string();
        }
        let mut c: Rc<RefCell<EVGColor>> = CssValue::color(&CssComputed::get(&got, name));
        if false == c.borrow().isSet {
            return fallback.to_string();
        }
        self.applied += 1;
        MdCss::hexOf(&c)
    }
    /// Every property of a rule this layout did not just read. Named once each,
    /// so a template author is told which of their lines did nothing rather
    /// than being left to wonder — `PLAN_SLIDES.md` §8.4, and the problem
    /// `PptxCss` already had and answered.
    fn rest(&mut self, got: &Rc<RefCell<CssComputed>>, known: &str) {
        if self.looksOnly {
            return;
        }
        let mut i: i64 = 0;
        while i < CssComputed::count(&got) {
            let name: String = got.borrow().names[i as usize].clone();
            if false == MdCss::listed(known, &name) {
                self.note(&name);
            }
            i += 1;
        }
    }
    /// ----------------------------------------------------------------------
    /// The print page
    /// ----------------------------------------------------------------------
    ///
    /// `@media print { page { width: 297mm; height: 210mm; bleed: 3mm;
    /// safe-area: 8mm } deck { crop-marks: on } }`. Applied after
    /// everything else that sets the page (the theme, `page:` in the front
    /// matter, the reader's choice), because paper is paper: the printing
    /// house's size is the size. A sheet with no print rules leaves the style
    /// as it was, apart from saying it is for print.
    pub fn applyPrint(&mut self, style: &Rc<RefCell<MdStyle>>) {
        style.borrow_mut().forPrint = true;
        let mut pn: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        pn.borrow_mut().tag = "page".to_string();
        let mut pg: Rc<RefCell<CssComputed>> = CssCascade::resolve(&self.printSheet.clone(), &pn);
        let _tmp_858 = self.len(&pg, "width", style.borrow().pageWidth.clone());
        style.borrow_mut().pageWidth = _tmp_858;
        let _tmp_860 = self.len(&pg, "height", style.borrow().pageHeight.clone());
        style.borrow_mut().pageHeight = _tmp_860;
        let _tmp_862 = self.len(&pg, "padding", style.borrow().margin.clone());
        style.borrow_mut().margin = _tmp_862;
        let _tmp_864 = self.len(&pg, "bleed", style.borrow().bleed.clone());
        style.borrow_mut().bleed = _tmp_864;
        let _tmp_866 = self.len(&pg, "safe-area", style.borrow().safeArea.clone());
        style.borrow_mut().safeArea = _tmp_866;
        let mut dn: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        dn.borrow_mut().tag = "deck".to_string();
        let mut dk: Rc<RefCell<CssComputed>> = CssCascade::resolve(&self.printSheet.clone(), &dn);
        if CssComputed::has(&dk, "crop-marks") {
            let v: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&dk, "crop-marks")));
            style.borrow_mut().cropMarks = (v == "on" || v == "true") || v == "yes";
            self.applied += 1;
        }
    }
}
