// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_html::MdHtml;
use crate::md_html::MdHtmlTag;
use crate::md_node::MdNode;
use crate::rgr::MdText;

/// ============================================================================
/// MdSvg.rgr — an <svg> written into the Markdown, kept as a picture
/// ============================================================================
///
/// ````markdown
/// <svg width="200" height="100"><circle cx="50" cy="50" r="40" fill="#5ce1ff"/></svg>
/// ````
///
/// A small drawing written where it is shown, without a file of its own. The
/// slide draws it as a picture: `MdLayout` lays out a paragraph or an HTML
/// block that is one `<svg>` element as the picture `pathOf` names, and the
/// host (the editor's page, the MCP server) draws `clean` of the same text
/// into its picture store under that name, as it draws an SVG file.
///
/// CLEAN. What is kept is a drawing and nothing else: the shapes, text,
/// gradients, clips, masks and markers of SVG, with their attributes, except
///   - an `on…` attribute (script),
///   - a link (`href`, `xlink:href`) that does not point inside the drawing
///     (`#id`), which keeps out pictures and pages from elsewhere,
///   - a `style` or any value with `javascript:`, `expression(` or a `url(`
///     that does not point inside the drawing.
/// `<script>`, `<foreignObject>`, `<style>`, `<image>`, `<a>` and any element
/// not named here are dropped with what is in them. Comments are dropped.
/// The same text always cleans to the same text, so its name is stable.
/// ============================================================================
/// an <svg> found in a document: the block it is, and its picture
#[derive(Clone)]
pub struct MdSvgUse {
    pub path: String,
    pub svg: String,
    pub srcStart: i64,
}

impl MdSvgUse {
    pub fn new() -> Self {
        Self {
            path: String::new(),
            svg: String::new(),
            srcStart: -1,
        }
    }
}

#[derive(Clone)]
pub struct MdSvg {
}

impl MdSvg {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn KEPT() -> String {
        " svg g path rect circle ellipse line polyline polygon text tspan textpath defs lineargradient radialgradient stop clippath mask pattern symbol use title desc marker ".to_string()
    }
    pub fn kept(lower_name: &str) -> bool {
        MdText::index_of(&MdSvg::KEPT(), &format!(" {} ", lower_name)) >= 0
    }
    /// `text` without its `<!-- … -->` comments
    pub fn noComments(text: &str) -> String {
        let mut out: String = String::new();
        let mut from: i64 = 0;
        let mut at: i64 = MdText::index_of(&text, "<!--");
        while at >= 0 {
            out = format!("{}{}", out, MdText::substr(&text, from, at));
            let e: i64 = MdText::index_of_from(&text, "-->", at + 4);
            if e < 0 {
                return out;
            }
            from = e + 3;
            at = MdText::index_of_from(&text, "<!--", from);
        }
        format!("{}{}", out, MdText::substr(&text, from, (MdText::str_len(&text) as i64)))
    }
    /// Is `text` (trimmed) one <svg> element and nothing else?
    pub fn isWhole(text: &str) -> bool {
        let t: String = MdSvg::noComments(text).trim().to_string();
        let n: i64 = MdText::str_len(&t) as i64;
        if n < 6 {
            return false;
        }
        if MdChar::lower(&MdText::substr(&t, 0, 4)) != "<svg" {
            return false;
        }
        let mut tg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&t, 0);
        if tg.borrow().name != "svg" {
            return false;
        }
        if tg.borrow().selfClose {
            return tg.borrow().end == n;
        }
        let close: i64 = MdHtml::closingOf(&t, "svg", tg.borrow().end.clone());
        if close < 0 {
            return false;
        }
        let mut ct: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&t, close);
        return ct.borrow().end == n;
    }
    pub fn safeValue(v: &str) -> bool {
        let l: String = MdChar::lower(v);
        if MdText::index_of(&l, "javascript:") >= 0 || MdText::index_of(&l, "vbscript:") >= 0 {
            return false;
        }
        if MdText::index_of(&l, "expression(") >= 0 {
            return false;
        }
        // url(#id) only
        let mut at: i64 = MdText::index_of(&l, "url(");
        while at >= 0 {
            let mut k: i64 = at + 4;
            while k < (MdText::str_len(&l) as i64) && (MdText::char_at(&l, k) == 32 || (MdText::char_at(&l, k) == 34 || MdText::char_at(&l, k) == 39)) {
                k += 1;
            }
            if k >= (MdText::str_len(&l) as i64) || MdText::char_at(&l, k) != 35 {
                return false;
            }
            at = MdText::index_of_from(&l, "url(", at + 4);
        }
        true
    }
    pub fn esc(v: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&v) as i64;
        let mut from: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if c == 34 || c == 60 {
                out = format!("{}{}", out, MdText::substr(&v, from, i));
                if c == 34 {
                    out = format!("{}&quot;", out);
                } else {
                    out = format!("{}&lt;", out);
                }
                from = i + 1;
            }
            i += 1;
        }
        format!("{}{}", out, MdText::substr(&v, from, n))
    }
    /// the tag written again with only the attributes it may keep
    pub fn openTag(tg: &Rc<RefCell<MdHtmlTag>>, root: bool) -> String {
        let mut out: String = format!("<{}", tg.borrow().rawName);
        let mut has_ns: bool = false;
        let mut i: i64 = 0;
        while i < (tg.borrow().keys.len() as i64) {
            let k: String = tg.borrow().keys[i as usize].clone();
            let v: String = tg.borrow().vals[i as usize].clone();
            let mut keep: bool = true;
            if MdText::index_of(&k, "on") == 0 {
                keep = false;
            }
            if k == "href" || k == "xlink:href" {
                if v.is_empty() {
                    keep = false;
                } else {
                    if MdText::char_at(&v.trim().to_string(), 0) != 35 {
                        keep = false;
                    }
                }
            }
            if !MdSvg::safeValue(&v) {
                keep = false;
            }
            if k == "xmlns" {
                has_ns = true;
            }
            if keep {
                out = format!("{} {}=\"{}\"", out, tg.borrow().rawKeys[i as usize].clone(), MdSvg::esc(&v));
            }
            i += 1;
        }
        if root && !has_ns {
            out = format!("{} xmlns=\"http://www.w3.org/2000/svg\"", out);
        }
        if tg.borrow().selfClose {
            return format!("{}/>", out);
        }
        format!("{}>", out)
    }
    /// The drawing in `text` with only what a picture needs; "" when `text`
    /// is not an <svg> element.
    pub fn clean(text: &str) -> String {
        let s: String = MdSvg::noComments(text).trim().to_string();
        if !MdSvg::isWhole(&s) {
            return String::new();
        }
        let mut out: String = String::new();
        let mut open: Vec<String> = Vec::new();
        let n: i64 = MdText::str_len(&s) as i64;
        let mut at: i64 = 0;
        let mut i: i64 = MdHtml::nextTag(&s, 0);
        while i >= 0 {
            if !open.is_empty() {
                out = format!("{}{}", out, MdText::substr(&s, at, i));
            }
            let mut tg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&s, i);
            let mut next: i64 = tg.borrow().end;
            if tg.borrow().close {
                if !open.is_empty() {
                    let top: String = open[((open.len() as i64) - 1) as usize].clone();
                    if MdChar::lower(&top) == tg.borrow().name {
                        out = format!("{}</{}>", out, top);
                        open.remove(((open.len() as i64) - 1) as usize);
                    }
                }
            } else {
                if MdSvg::kept(&tg.borrow().name.clone()) {
                    out = format!("{}{}", out, MdSvg::openTag(&tg, open.is_empty()));
                    if !tg.borrow().selfClose {
                        open.push(tg.borrow().rawName.clone());
                    }
                } else {
                    // dropped, and what is inside it
                    if !tg.borrow().selfClose {
                        let mut _tmp_1949 = tg.borrow().name.clone();
                        let ce: i64 = MdHtml::closingOf(&s, &_tmp_1949, tg.borrow().end.clone());
                        if ce < 0 {
                            next = n;
                        } else {
                            let mut ctg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&s, ce);
                            next = ctg.borrow().end;
                        }
                    }
                }
            }
            if open.is_empty() {
                // the root closed: what follows it is not the drawing
                return out;
            }
            at = next;
            i = MdHtml::nextTag(&s, next);
        }
        while !open.is_empty() {
            out = format!("{}</{}>", out, open[((open.len() as i64) - 1) as usize].clone());
            open.remove(((open.len() as i64) - 1) as usize);
        }
        out
    }
    /// The picture a cleaned drawing is stored as: named by its text, so the
    /// same drawing written twice is one picture and a changed one is new.
    pub fn pathOf(clean_svg: &str) -> String {
        let mut a: i64 = 7;
        let mut b: i64 = 11;
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&clean_svg) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&clean_svg, i);
            a = (a * 31 + c) % 1000000007;
            b = (b * 37 + c) % 998244353;
            i += 1;
        }
        format!("inline-svg/{}{}.svg", MdSvg::hex(a), MdSvg::hex(b))
    }
    pub fn hex(v: i64) -> String {
        let digits: String = "0123456789abcdef".to_string();
        let mut out: String = String::new();
        let mut x: i64 = v;
        let mut k: i64 = 0;
        while k < 8 {
            let d: i64 = x % 16;
            out = format!("{}{}", MdText::substr(&digits, d, d + 1), out);
            x = x / 16;
            k += 1;
        }
        out
    }
    /// The drawing a block is, "" when it is not one: an HTML block or a
    /// paragraph that is one <svg> element.
    pub fn ofBlock(node: &Rc<RefCell<MdNode>>) -> String {
        if node.borrow().kind != "html_block" && node.borrow().kind != "paragraph" {
            return String::new();
        }
        let mut lit: String = node.borrow().literal.clone();
        if node.borrow().kind == "paragraph" {
            lit = MdSvg::paraText(node);
        }
        if MdText::index_of(&lit, "<svg") < 0 {
            if MdText::index_of(&lit, "<SVG") < 0 {
                return String::new();
            }
        }
        MdSvg::clean(&lit)
    }
    /// A paragraph said again as the HTML it was written as: its tags, its
    /// text (escaped again), its line breaks. "" when it holds anything else
    /// (emphasis, a link, a picture), which an <svg> does not.
    pub fn paraText(node: &Rc<RefCell<MdNode>>) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if c.borrow().kind == "html_inline" {
                out = format!("{}{}", out, c.borrow().literal);
            } else {
                if c.borrow().kind == "text" {
                    out = format!("{}{}", out, MdSvg::escText(&c.borrow().literal));
                } else {
                    if c.borrow().kind == "softbreak" || c.borrow().kind == "linebreak" {
                        out = format!("{}\n", out);
                    } else {
                        return String::new();
                    }
                }
            }
            i += 1;
        }
        out
    }
    pub fn escText(v: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&v) as i64;
        let mut from: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if c == 38 || (c == 60 || c == 62) {
                out = format!("{}{}", out, MdText::substr(&v, from, i));
                if c == 38 {
                    out = format!("{}&amp;", out);
                }
                if c == 60 {
                    out = format!("{}&lt;", out);
                }
                if c == 62 {
                    out = format!("{}&gt;", out);
                }
                from = i + 1;
            }
            i += 1;
        }
        format!("{}{}", out, MdText::substr(&v, from, n))
    }
    /// Every drawing a document's blocks are, at any depth (a list item, a
    /// quote, a container), for the host to draw into its picture store.
    pub fn inDoc(doc: &Rc<RefCell<MdNode>>) -> Vec<Rc<RefCell<MdSvgUse>>> {
        let mut out: Vec<Rc<RefCell<MdSvgUse>>> = Vec::new();
        MdSvg::collect(doc, &mut out);
        out
    }
    pub fn collect(node: &Rc<RefCell<MdNode>>, mut out: &mut Vec<Rc<RefCell<MdSvgUse>>>) {
        let svg: String = MdSvg::ofBlock(node);
        if !svg.is_empty() {
            let mut u: Rc<RefCell<MdSvgUse>> = Rc::new(RefCell::new(MdSvgUse::new()));
            u.borrow_mut().svg = svg.clone();
            u.borrow_mut().path = MdSvg::pathOf(&svg);
            u.borrow_mut().srcStart = node.borrow().srcStart;
            out.push(u.clone());
            return;
        }
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            MdSvg::collect(&node.borrow().childAt(i), &mut out);
            i += 1;
        }
    }
}
