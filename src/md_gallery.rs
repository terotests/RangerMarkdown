// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;

use crate::md_attrs::MdAttrs;
use crate::md_char::MdChar;
use crate::md_css::MdCss;
use crate::md_node::MdNode;
use crate::md_style::MdStyle;
use crate::rgr::MdText;
use crate::rgr::{CssCascade, CssComputed, CssNode, CssSheet, CssValue, EVGColor};

/// ============================================================================
/// MdGallery.rgr — a photo album as a fence: what it says, and where each
/// picture, caption and text cell goes.
/// ============================================================================
///
/// ````markdown
/// ```gallery
/// - media/ranta.jpg: Hietaniemi heinäkuussa {focus=top}
/// - media/sauna.jpg: Sauna {span=2}
/// - Kesä jäi mieleen.
/// ```
/// {layout=full fit=cover caption=overlay .polaroid}
/// ````
///
/// One line per cell: a picture (`path: caption`, or `![caption](path)`)
/// or, when the line names no picture, a TEXT cell (`cell.text` in a sheet).
/// A `{…}` at the end of a line is that cell's own: `focus=`, `span=`,
/// `.class`.
///
/// Under the fence:
///
///   layout=grid   the pictures in rows (the default); the sheet's
///                 `gallery { columns }` says how many across
///   layout=full   one picture a slide, edge to edge, no page margins; in
///                 print it runs over the bleed
///   fit=cover     fills its place and is cropped (the default);
///   fit=contain   shows all of it, the gallery's background round it
///   caption=below | overlay | none   overlay: the words on the picture, on
///                 a band of the page's colour so they read (as `bg-dim`)
///   focus=        what a crop keeps: `top`, `bottom-left`, `center`,
///                 `30% 70%` or `0.3,0.7` (x then y); per cell or for all
///
/// The template decides the STRUCTURE and the sheet only its look: gaps,
/// borders, padding, corner radius, type. Nothing a sheet says can make a
/// cell overlap another or leave its slide (`MdGalleryLook` takes lengths,
/// the geometry here takes the rest).
///
/// Selectors: `gallery`, `cell`, `caption`, `cell.text`, a class under the
/// fence (`.polaroid cell`) and one cell by its place (`cell:nth(3)`, read
/// by `MdCss.desugar`).
///
/// This file is geometry and nothing else: no text is measured here (the
/// host hands in how tall each caption is at the width it was given) and
/// nothing is drawn. `MdLayout.gallery` turns the answer into boxes.
/// `tests/MdGalleryTest.rgr` checks it.
/// ============================================================================
/// One line of the fence.
#[derive(Clone)]
pub struct MdGalleryItem {
    /// from 1, the number `cell:nth(n)` names
    pub index: i64,
    /// the picture as written ("" for a text cell)
    pub src: String,
    pub caption: String,
    /// a text cell's words
    pub text: String,
    pub isText: bool,
    /// what a crop keeps, 0..1 across and down; the gallery's when unset
    pub focusX: f64,
    pub focusY: f64,
    pub hasFocus: bool,
    /// how many columns the cell takes
    pub span: i64,
    pub classes: Vec<String>,
    /// where the line starts in the fence's text
    pub offset: i64,
}

impl MdGalleryItem {
    pub fn new() -> Self {
        Self {
            index: 0,
            src: String::new(),
            caption: String::new(),
            text: String::new(),
            isText: false,
            focusX: 0.5,
            focusY: 0.5,
            hasFocus: false,
            span: 1,
            classes: Vec::new(),
            offset: 0,
        }
    }
}

/// The look of a cell (or of its caption, or of a text cell's words), as
/// the sheet says. Lengths in points.
#[derive(Clone)]
pub struct MdCellLook {
    pub padT: f64,
    pub padR: f64,
    pub padB: f64,
    pub padL: f64,
    pub radius: f64,
    pub borderWidth: f64,
    pub borderColor: String,
    pub background: String,
    pub fontSize: f64,
    pub family: String,
    pub color: String,
    /// the colour came from the stylesheet: it is the author's, kept as is
    pub colorSet: bool,
    /// "start" | "center" | "end"
    pub align: String,
    pub bold: bool,
}

impl MdCellLook {
    pub fn new() -> Self {
        Self {
            padT: 0.0,
            padR: 0.0,
            padB: 0.0,
            padL: 0.0,
            radius: 0.0,
            borderWidth: 0.0,
            borderColor: String::new(),
            background: String::new(),
            fontSize: 11.0,
            family: String::new(),
            color: String::new(),
            colorSet: false,
            align: "start".to_string(),
            bold: false,
        }
    }
}

#[derive(Clone)]
pub struct MdGalleryLook {
    pub gap: f64,
    /// 0: chosen from the number of cells
    pub columns: i64,
    pub background: String,
}

impl MdGalleryLook {
    pub fn new() -> Self {
        Self {
            gap: 8.0,
            columns: 0,
            background: String::new(),
        }
    }
}

/// Where one cell went. All in the layout's points; the picture rectangle
/// can be larger than the cell (a `cover` crop is expressed by the crop
/// fractions instead, so it is not) and, in print, run past the page's edge
/// by the bleed.
#[derive(Clone)]
pub struct MdGalleryCell {
    pub item: i64,
    /// which page of the gallery's, from 0 (a `full` gallery: one per cell)
    pub pageOff: i64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// the picture: where it is drawn and which part of it (0..1 of its
    /// pixels) is shown there
    pub picX: f64,
    pub picY: f64,
    pub picW: f64,
    pub picH: f64,
    pub cropX: f64,
    pub cropY: f64,
    pub cropW: f64,
    pub cropH: f64,
    pub cropped: bool,
    /// the caption (or a text cell's words): the box its lines are set in
    pub capX: f64,
    pub capY: f64,
    pub capW: f64,
    pub capH: f64,
    /// overlay: a band of the page's colour under the caption
    pub dim: bool,
    pub dimX: f64,
    pub dimY: f64,
    pub dimW: f64,
    pub dimH: f64,
}

impl MdGalleryCell {
    pub fn new() -> Self {
        Self {
            item: 0,
            pageOff: 0,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            picX: 0.0,
            picY: 0.0,
            picW: 0.0,
            picH: 0.0,
            cropX: 0.0,
            cropY: 0.0,
            cropW: 1.0,
            cropH: 1.0,
            cropped: false,
            capX: 0.0,
            capY: 0.0,
            capW: 0.0,
            capH: 0.0,
            dim: false,
            dimX: 0.0,
            dimY: 0.0,
            dimW: 0.0,
            dimH: 0.0,
        }
    }
}

#[derive(Clone)]
pub struct MdGallery {
    pub layout: String,
    pub fit: String,
    /// "" until read: below on a grid, overlay on a full page
    pub caption: String,
    pub focusX: f64,
    pub focusY: f64,
    pub items: Vec<Rc<RefCell<MdGalleryItem>>>,
    /// the classes under the fence (`{.polaroid}`)
    pub classes: Vec<String>,
}

impl MdGallery {
    pub fn new() -> Self {
        Self {
            layout: "grid".to_string(),
            fit: "cover".to_string(),
            caption: String::new(),
            focusX: 0.5,
            focusY: 0.5,
            items: Vec::new(),
            classes: Vec::new(),
        }
    }
    /// ----------------------------------------------------------------------
    /// Reading the fence
    /// ----------------------------------------------------------------------
    pub fn isGallery(node: &Rc<RefCell<MdNode>>) -> bool {
        if node.borrow().kind != "code_block" {
            return false;
        }
        let mut info: String = node.borrow().info.trim().to_string();
        let sp: i64 = MdText::index_of(&info, " ");
        if sp > 0 {
            info = MdText::substr(&info, 0, sp);
        }
        let brace: i64 = MdText::index_of(&info, "{");
        if brace > 0 {
            info = MdText::substr(&info, 0, brace);
        }
        MdChar::lower(&info) == "gallery"
    }
    pub fn of(node: &Rc<RefCell<MdNode>>) -> Rc<RefCell<MdGallery>> {
        let mut g: Rc<RefCell<MdGallery>> = MdGallery::parse(&node.borrow().literal.clone());
        let lay: String = MdChar::lower(&node.borrow().attrValue("layout").trim().to_string());
        if lay == "full" {
            g.borrow_mut().layout = "full".to_string();
        }
        let fit: String = MdChar::lower(&node.borrow().attrValue("fit").trim().to_string());
        if fit == "contain" {
            g.borrow_mut().fit = "contain".to_string();
        }
        let cap: String = MdChar::lower(&node.borrow().attrValue("caption").trim().to_string());
        if (cap == "below" || cap == "overlay") || cap == "none" {
            g.borrow_mut().caption = cap.clone();
        }
        if g.borrow().caption.is_empty() {
            g.borrow_mut().caption = "below".to_string();
            if g.borrow().layout == "full" {
                g.borrow_mut().caption = "overlay".to_string();
            }
        }
        let mut f: Vec<f64> = MdGallery::focusOf(&node.borrow().attrValue("focus"));
        if (f.len() as i64) == 2 {
            g.borrow_mut().focusX = f[0];
            g.borrow_mut().focusY = f[1];
        }
        let mut i: i64 = 0;
        while i < node.borrow().classCount() {
            g.borrow_mut().classes.push(node.borrow().classAt(i));
            i += 1;
        }
        let mut k: i64 = 0;
        while k < (g.borrow().items.len() as i64) {
            let mut it: Rc<RefCell<MdGalleryItem>> = g.borrow().items[k as usize].clone();
            if !it.borrow().hasFocus {
                it.borrow_mut().focusX = g.borrow().focusX;
                it.borrow_mut().focusY = g.borrow().focusY;
            }
            k += 1;
        }
        g
    }
    /// The lines of the fence, one cell each. A line that does not start a
    /// list item continues the one before it (a long caption wrapped by
    /// hand); blank lines are nothing.
    pub fn parse(literal: &str) -> Rc<RefCell<MdGallery>> {
        let mut g: Rc<RefCell<MdGallery>> = Rc::new(RefCell::new(MdGallery::new()));
        let mut lines: Vec<String> = literal.split("\n")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        let mut off: i64 = 0;
        let mut i: i64 = 0;
        while i < (lines.len() as i64) {
            let raw: String = lines[i as usize].clone();
            let line: String = raw.trim().to_string();
            let n: i64 = MdText::str_len(&line) as i64;
            if n > 0 {
                let mut bullet: bool = false;
                if n >= 2 {
                    let c0: i64 = MdText::char_at(&line, 0);
                    if (c0 == 45 || c0 == 42) || c0 == 43 {
                        if MdText::char_at(&line, 1) == 32 {
                            bullet = true;
                        }
                    }
                }
                if bullet || g.borrow().items.is_empty() {
                    let mut body: String = line.clone();
                    if bullet {
                        body = MdText::substr(&line, 2, n).trim().to_string();
                    }
                    let mut it: Rc<RefCell<MdGalleryItem>> = MdGallery::item(&body);
                    it.borrow_mut().index = (g.borrow().items.len() as i64) + 1;
                    it.borrow_mut().offset = off;
                    g.borrow_mut().items.push(it.clone());
                } else {
                    let mut last: Rc<RefCell<MdGalleryItem>> = g.borrow().items[((g.borrow().items.len() as i64) - 1) as usize].clone();
                    if last.borrow().isText {
                        let text_v = format!("{} {}", last.borrow().text, line);
                        last.borrow_mut().text = text_v;
                    } else {
                        let caption_v = format!("{} {}", last.borrow().caption, line).trim().to_string();
                        last.borrow_mut().caption = caption_v;
                    }
                }
            }
            off = (off + (MdText::str_len(&raw) as i64)) + 1;
            i += 1;
        }
        g
    }
    /// One cell from its line (without the bullet).
    pub fn item(body0: &str) -> Rc<RefCell<MdGalleryItem>> {
        let mut it: Rc<RefCell<MdGalleryItem>> = Rc::new(RefCell::new(MdGalleryItem::new()));
        let mut body: String = body0.to_string();
        // its own `{focus=top span=2 .wide}` at the end
        let at: i64 = MdAttrs::trailingAt(&body);
        if at > 0 {
            let mut tmp: Rc<RefCell<MdNode>> = Rc::new(RefCell::new(MdNode::new()));
            if MdAttrs::applyTo(&tmp, &MdText::substr(&body, at, (MdText::str_len(&body) as i64))) {
                body = MdText::substr(&body, 0, at).trim().to_string();
                let mut f: Vec<f64> = MdGallery::focusOf(&tmp.borrow().attrValue("focus"));
                if (f.len() as i64) == 2 {
                    it.borrow_mut().focusX = f[0];
                    it.borrow_mut().focusY = f[1];
                    it.borrow_mut().hasFocus = true;
                }
                let sp: String = tmp.borrow().attrValue("span").trim().to_string();
                if !sp.is_empty() {
                    let v: Option<i64> = sp.parse::<i64>().ok();
                    let s: i64 = if v.is_some() { v.unwrap() } else { 1 };
                    if s > 1 {
                        it.borrow_mut().span = s;
                    }
                }
                let mut ci: i64 = 0;
                while ci < tmp.borrow().classCount() {
                    it.borrow_mut().classes.push(tmp.borrow().classAt(ci));
                    ci += 1;
                }
            }
        }
        // `![caption](path)`
        if (MdText::str_len(&body) as i64) > 4 && MdText::substr(&body, 0, 2) == "![" {
            let close: i64 = MdText::index_of(&body, "](");
            let end: i64 = (MdText::str_len(&body) as i64) - 1;
            if close > 1 && MdText::char_at(&body, end) == 41 {
                it.borrow_mut().caption = MdText::substr(&body, 2, close);
                it.borrow_mut().src = MdText::substr(&body, close + 2, end).trim().to_string();
                return it;
            }
        }
        // `path: caption` or a bare path
        let colon: i64 = MdText::index_of(&body, ": ");
        let mut head: String = body.clone();
        let mut rest: String = String::new();
        if colon > 0 {
            head = MdText::substr(&body, 0, colon).trim().to_string();
            rest = MdText::substr(&body, colon + 2, (MdText::str_len(&body) as i64)).trim().to_string();
        } else {
            if (MdText::str_len(&body) as i64) > 1 && MdText::char_at(&body, (MdText::str_len(&body) as i64) - 1) == 58 {
                head = MdText::substr(&body, 0, (MdText::str_len(&body) as i64) - 1).trim().to_string();
            }
        }
        if MdGallery::isPicturePath(&head) {
            it.borrow_mut().src = head.clone();
            it.borrow_mut().caption = rest.clone();
            return it;
        }
        it.borrow_mut().isText = true;
        it.borrow_mut().text = body.clone();
        it
    }
    /// A picture's name: one word ending in a picture's extension.
    pub fn isPicturePath(s: &str) -> bool {
        if (MdText::str_len(&s) as i64) < 5 {
            return false;
        }
        if MdText::index_of(&s, " ") >= 0 {
            return false;
        }
        let mut low: String = MdChar::lower(s);
        let q: i64 = MdText::index_of(&low, "?");
        if q > 0 {
            low = MdText::substr(&low, 0, q);
        }
        let mut exts: Vec<String> = ".jpg,.jpeg,.png,.gif,.webp,.svg,.bmp,.avif".to_string()
            .split(",")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        let mut i: i64 = 0;
        while i < (exts.len() as i64) {
            let e: String = exts[i as usize].clone();
            let n: i64 = MdText::str_len(&low) as i64;
            let m: i64 = MdText::str_len(&e) as i64;
            if n > m && MdText::substr(&low, n - m, n) == e {
                return true;
            }
            i += 1;
        }
        false
    }
    /// `focus=`: words (`top`, `bottom-left`, `left`, `center`) or two
    /// numbers, x then y, as percentages (`30% 70%`) or fractions
    /// (`0.3,0.7`). Empty when it says nothing this can read.
    pub fn focusOf(v0: &str) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        let v: String = MdChar::lower(&v0.trim().to_string());
        if v.is_empty() {
            return out;
        }
        let mut x: f64 = 0.5;
        let mut y: f64 = 0.5;
        let mut words: bool = false;
        if MdText::index_of(&v, "left") >= 0 {
            x = 0.0;
            words = true;
        }
        if MdText::index_of(&v, "right") >= 0 {
            x = 1.0;
            words = true;
        }
        if MdText::index_of(&v, "top") >= 0 {
            y = 0.0;
            words = true;
        }
        if MdText::index_of(&v, "bottom") >= 0 {
            y = 1.0;
            words = true;
        }
        if MdText::index_of(&v, "center") >= 0 {
            words = true;
        }
        if MdText::index_of(&v, "centre") >= 0 {
            words = true;
        }
        if words {
            out.push(x);
            out.push(y);
            return out;
        }
        let mut parts: Vec<String> = Vec::new();
        let mut cur: String = String::new();
        let mut i: i64 = 0;
        while i < (MdText::str_len(&v) as i64) {
            let c: i64 = MdText::char_at(&v, i);
            if (c == 32 || c == 44) || c == 59 {
                if !cur.is_empty() {
                    parts.push(cur.clone());
                }
                cur = String::new();
            } else {
                // the unit itself (`strfromcode` of a byte breaks "ä" on Go)
                cur = format!("{}{}", cur, MdText::substr(&v, i, i + 1));
            }
            i += 1;
        }
        if !cur.is_empty() {
            parts.push(cur.clone());
        }
        if (parts.len() as i64) != 2 {
            return out;
        }
        let a: f64 = MdGallery::fraction(&parts[0].clone());
        let b: f64 = MdGallery::fraction(&parts[1].clone());
        if a < 0.0 || b < 0.0 {
            return out;
        }
        out.push(a);
        out.push(b);
        out
    }
    /// `30%` or `0.3` as 0.3, clamped to 0..1; -1 when it is not a number.
    pub fn fraction(s0: &str) -> f64 {
        let mut s: String = s0.trim().to_string();
        let mut pct: bool = false;
        if (MdText::str_len(&s) as i64) > 1 && MdText::char_at(&s, (MdText::str_len(&s) as i64) - 1) == 37 {
            pct = true;
            s = MdText::substr(&s, 0, (MdText::str_len(&s) as i64) - 1);
        }
        if !CssValue::is_bare_number(&s) {
            return -1.0;
        }
        let d: Option<f64> = s.trim().to_string().parse::<f64>().ok();
        if d.is_none() {
            return -1.0;
        }
        let mut v: f64 = d.unwrap();
        if pct || v > 1.0 {
            v /= 100.0;
        }
        if v < 0.0 {
            v = 0.0;
        }
        if v > 1.0 {
            v = 1.0;
        }
        v
    }
    fn boxLook(out: &Rc<RefCell<MdCellLook>>, got: &Rc<RefCell<CssComputed>>) {
        if CssComputed::has(&got, "padding") {
            let mut pads: Vec<f64> = MdGallery::sides(&CssComputed::get(&got, "padding"));
            if (pads.len() as i64) == 4 {
                out.borrow_mut().padT = pads[0];
                out.borrow_mut().padR = pads[1];
                out.borrow_mut().padB = pads[2];
                out.borrow_mut().padL = pads[3];
            }
        }
        let _tmp_1806 = MdGallery::lenOf(got, "padding-top", out.borrow().padT.clone());
        out.borrow_mut().padT = _tmp_1806;
        let _tmp_1808 = MdGallery::lenOf(got, "padding-right", out.borrow().padR.clone());
        out.borrow_mut().padR = _tmp_1808;
        let _tmp_1810 = MdGallery::lenOf(got, "padding-bottom", out.borrow().padB.clone());
        out.borrow_mut().padB = _tmp_1810;
        let _tmp_1812 = MdGallery::lenOf(got, "padding-left", out.borrow().padL.clone());
        out.borrow_mut().padL = _tmp_1812;
        let _tmp_1814 = MdGallery::lenOf(got, "border-radius", out.borrow().radius.clone());
        out.borrow_mut().radius = _tmp_1814;
        let _tmp_1816 = MdGallery::lenOf(got, "border-width", out.borrow().borderWidth.clone());
        out.borrow_mut().borderWidth = _tmp_1816;
        let _tmp_1818 = MdGallery::colorOf(got, "border-color", &out.borrow().borderColor.clone());
        out.borrow_mut().borderColor = _tmp_1818;
        let _tmp_1820 = MdGallery::colorOf(got, "background-color", &out.borrow().background.clone());
        out.borrow_mut().background = _tmp_1820;
        let _tmp_1822 = MdGallery::lenOf(got, "font-size", out.borrow().fontSize.clone());
        out.borrow_mut().fontSize = _tmp_1822;
        let _tmp_1824 = MdGallery::colorOf(got, "color", &out.borrow().color.clone());
        out.borrow_mut().color = _tmp_1824;
        if CssComputed::has(&got, "color") {
            out.borrow_mut().colorSet = true;
        }
        if CssComputed::has(&got, "font-family") {
            let f: String = CssValue::first_family(&CssComputed::get(&got, "font-family"));
            if !f.is_empty() {
                out.borrow_mut().family = f.clone();
            }
        }
        if CssComputed::has(&got, "text-align") {
            let a: String = MdCss::alignWord(&CssComputed::get(&got, "text-align"));
            if !a.is_empty() {
                out.borrow_mut().align = a.clone();
            }
        }
        if CssComputed::has(&got, "font-weight") {
            out.borrow_mut().bold = CssValue::is_bold(&CssComputed::get(&got, "font-weight"));
        }
    }
    /// `10pt`, `10pt 20pt`, `10pt 10pt 32pt` or four: top, right, bottom,
    /// left as CSS has them.
    pub fn sides(v: &str) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        let mut words: Vec<String> = Vec::new();
        let mut cur: String = String::new();
        let t: String = CssSheet::tidy(v);
        let mut i: i64 = 0;
        while i < (MdText::str_len(&t) as i64) {
            let c: i64 = MdText::char_at(&t, i);
            if c == 32 {
                if !cur.is_empty() {
                    words.push(cur.clone());
                }
                cur = String::new();
            } else {
                cur = format!("{}{}", cur, MdText::substr(&t, i, i + 1));
            }
            i += 1;
        }
        if !cur.is_empty() {
            words.push(cur.clone());
        }
        let n: i64 = words.len() as i64;
        if n < 1 || n > 4 {
            return out;
        }
        let mut vals: Vec<f64> = Vec::new();
        let mut k: i64 = 0;
        while k < n {
            vals.push(MdGallery::safeLen(CssValue::font_points(&words[k as usize].clone(), 0.0)));
            k += 1;
        }
        let top: f64 = vals[0];
        let mut right: f64 = top;
        let mut bottom: f64 = top;
        let mut left: f64 = top;
        if n >= 2 {
            right = vals[1];
            left = right;
        }
        if n >= 3 {
            bottom = vals[2];
        }
        if n == 4 {
            left = vals[3];
        }
        out.push(top);
        out.push(right);
        out.push(bottom);
        out.push(left);
        out
    }
    fn lenOf(got: &Rc<RefCell<CssComputed>>, name: &str, fallback: f64) -> f64 {
        if !CssComputed::has(&got, name) {
            return fallback;
        }
        return MdGallery::safeLen(CssValue::font_points(&CssComputed::get(&got, name), fallback));
    }
    /// A length a sheet gave, within what a slide can hold: no negative
    /// padding, nothing past 500pt (MdCssLimits holds the document's own).
    pub fn safeLen(v: f64) -> f64 {
        if v < 0.0 {
            return 0.0;
        }
        if v > 500.0 {
            return 500.0;
        }
        v
    }
    fn colorOf(got: &Rc<RefCell<CssComputed>>, name: &str, fallback: &str) -> String {
        if !CssComputed::has(&got, name) {
            return fallback.to_string();
        }
        let word: String = CssSheet::lower(&CssSheet::tidy(&CssComputed::get(&got, name)));
        if word == "none" || word == "transparent" {
            return String::new();
        }
        let mut c: Rc<RefCell<EVGColor>> = CssValue::color(&CssComputed::get(&got, name));
        if !c.borrow().is_set {
            return fallback.to_string();
        }
        MdCss::hexOf(&c)
    }
    pub fn spanIn(it: &Rc<RefCell<MdGalleryItem>>, cols: i64) -> i64 {
        let mut s: i64 = it.borrow().span;
        if s < 1 {
            s = 1;
        }
        if s > cols {
            s = cols;
        }
        s
    }
    /// How many rows go on one slide: as many as keep a row at least `minRow`
    /// high in `h`, at least one.
    pub fn rowsPerPage(h: f64, gap: f64, min_row: f64, row_count: i64) -> i64 {
        let mut n: i64 = row_count;
        while n > 1 {
            let row_h: f64 = (h - gap * ((n - 1) as f64)) / (n as f64);
            if row_h >= min_row {
                return n;
            }
            n -= 1;
        }
        1
    }
    /// The picture in the box `x, y, w, h`: `cover` fills it and keeps the
    /// part `fx, fy` names, `contain` shows all of it in the middle.
    pub fn fitPicture(c: &Rc<RefCell<MdGalleryCell>>, fit: &str, fx: f64, fy: f64, nat_w: f64, nat_h: f64, x: f64, y: f64, w: f64, h: f64) {
        c.borrow_mut().picX = x;
        c.borrow_mut().picY = y;
        c.borrow_mut().picW = w;
        c.borrow_mut().picH = h;
        c.borrow_mut().cropX = 0.0;
        c.borrow_mut().cropY = 0.0;
        c.borrow_mut().cropW = 1.0;
        c.borrow_mut().cropH = 1.0;
        c.borrow_mut().cropped = false;
        if ((nat_w <= 0.0 || nat_h <= 0.0) || w <= 0.0) || h <= 0.0 {
            return;
        }
        let pr: f64 = nat_w / nat_h;
        let br: f64 = w / h;
        if fit == "contain" {
            if pr > br {
                c.borrow_mut().picH = w / pr;
                let picY_v = y + (h - c.borrow().picH) * 0.5;
                c.borrow_mut().picY = picY_v;
            } else {
                c.borrow_mut().picW = h * pr;
                let picX_v = x + (w - c.borrow().picW) * 0.5;
                c.borrow_mut().picX = picX_v;
            }
            return;
        }
        if ((pr - br).abs()) < 0.0001 {
            return;
        }
        c.borrow_mut().cropped = true;
        if pr > br {
            // wider than its place: a slice across, from where `fx` says
            c.borrow_mut().cropW = br / pr;
            let cropX_v = (1.0 - c.borrow().cropW) * fx;
            c.borrow_mut().cropX = cropX_v;
        } else {
            c.borrow_mut().cropH = pr / br;
            let cropY_v = (1.0 - c.borrow().cropH) * fy;
            c.borrow_mut().cropY = cropY_v;
        }
    }
    /// ----------------------------------------------------------------------
    /// Print
    /// ----------------------------------------------------------------------
    /// How many dots an inch a picture of `natW` pixels gets when it is drawn
    /// `drawnW` points wide (a crop counts: only `cropW` of it is in the
    /// place). 0 when it is not known.
    pub fn effectiveDpi(nat_w: f64, crop_w: f64, drawn_w: f64) -> f64 {
        if (nat_w <= 0.0 || drawn_w <= 0.0) || crop_w <= 0.0 {
            return 0.0;
        }
        ((nat_w * crop_w) / drawn_w) * 72.0
    }
    /// ----------------------------------------------------------------------
    /// What the sheet says
    /// ----------------------------------------------------------------------
    fn galleryNode(&self) -> Rc<RefCell<CssNode>> {
        let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        n.borrow_mut().tag = "gallery".to_string();
        let mut i: i64 = 0;
        while i < (self.classes.len() as i64) {
            CssNode::add_class(&n, &self.classes[i as usize].clone());
            i += 1;
        }
        n
    }
    fn cellNode(&self, it: &Rc<RefCell<MdGalleryItem>>) -> Rc<RefCell<CssNode>> {
        let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        n.borrow_mut().tag = "cell".to_string();
        let mut p: Option<Rc<RefCell<CssNode>>> = Some(self.galleryNode());
        n.borrow_mut().parent = p.clone();
        if it.borrow().isText {
            CssNode::add_class(&n, "text");
        }
        CssNode::add_class(&n, &MdCss::nthClass(it.borrow().index.clone()));
        let mut i: i64 = 0;
        while i < (it.borrow().classes.len() as i64) {
            CssNode::add_class(&n, &it.borrow().classes[i as usize].clone());
            i += 1;
        }
        n
    }
    pub fn look(&self, css: &Rc<RefCell<MdCss>>) -> Rc<RefCell<MdGalleryLook>> {
        let mut out: Rc<RefCell<MdGalleryLook>> = Rc::new(RefCell::new(MdGalleryLook::new()));
        let mut _tmp_1832 = css.borrow().sheet.clone();
        let mut got: Rc<RefCell<CssComputed>> = CssCascade::resolve(&_tmp_1832, &self.galleryNode());
        let _tmp_1835 = MdGallery::lenOf(&got, "gap", out.borrow().gap.clone());
        out.borrow_mut().gap = _tmp_1835;
        if CssComputed::has(&got, "column-gap") {
            let _tmp_1837 = MdGallery::lenOf(&got, "column-gap", out.borrow().gap.clone());
            out.borrow_mut().gap = _tmp_1837;
        }
        let mut cols: String = CssSheet::tidy(&CssComputed::get(&got, "columns"));
        if cols.is_empty() {
            cols = CssSheet::tidy(&CssComputed::get(&got, "column-count"));
        }
        if !cols.is_empty() {
            let c: Option<i64> = cols.parse::<i64>().ok();
            let ci: i64 = if c.is_some() { c.unwrap() } else { 0 };
            if ci > 0 {
                out.borrow_mut().columns = ci;
            }
        }
        out.borrow_mut().background = MdGallery::colorOf(&got, "background-color", "");
        out
    }
    /// A cell's own look, over the defaults a style gives (`text`: a text
    /// cell, whose words are set in the cell's own type).
    pub fn cellLook(&self, css: &Rc<RefCell<MdCss>>, it: &Rc<RefCell<MdGalleryItem>>, st: &Rc<RefCell<MdStyle>>) -> Rc<RefCell<MdCellLook>> {
        let mut out: Rc<RefCell<MdCellLook>> = Rc::new(RefCell::new(MdCellLook::new()));
        out.borrow_mut().fontSize = st.borrow().bodySize;
        out.borrow_mut().family = st.borrow().bodyFamily.clone();
        out.borrow_mut().color = st.borrow().textColor.clone();
        if it.borrow().isText {
            out.borrow_mut().padT = 12.0;
            out.borrow_mut().padR = 12.0;
            out.borrow_mut().padB = 12.0;
            out.borrow_mut().padL = 12.0;
        }
        let mut _tmp_1842 = css.borrow().sheet.clone();
        let mut got: Rc<RefCell<CssComputed>> = CssCascade::resolve(&_tmp_1842, &self.cellNode(it));
        MdGallery::boxLook(&out, &got);
        out
    }
    /// A caption's look: a step smaller than the text, the page's ink on an
    /// overlay band, muted under a picture.
    pub fn captionLook(&self, css: &Rc<RefCell<MdCss>>, it: &Rc<RefCell<MdGalleryItem>>, st: &Rc<RefCell<MdStyle>>) -> Rc<RefCell<MdCellLook>> {
        let mut out: Rc<RefCell<MdCellLook>> = Rc::new(RefCell::new(MdCellLook::new()));
        out.borrow_mut().fontSize = st.borrow().bodySize * 0.8;
        out.borrow_mut().family = st.borrow().bodyFamily.clone();
        out.borrow_mut().color = st.borrow().mutedColor.clone();
        if self.caption == "overlay" {
            out.borrow_mut().color = st.borrow().textColor.clone();
            out.borrow_mut().padT = 6.0;
            out.borrow_mut().padB = 6.0;
        } else {
            out.borrow_mut().padT = 4.0;
        }
        let mut n: Rc<RefCell<CssNode>> = Rc::new(RefCell::new(CssNode::new()));
        n.borrow_mut().tag = "caption".to_string();
        let mut p: Option<Rc<RefCell<CssNode>>> = Some(self.cellNode(it));
        n.borrow_mut().parent = p.clone();
        let mut got: Rc<RefCell<CssComputed>> = CssCascade::resolve(&css.borrow().sheet.clone(), &n);
        MdGallery::boxLook(&out, &got);
        out
    }
    /// ----------------------------------------------------------------------
    /// The grid
    /// ----------------------------------------------------------------------
    /// How many across: the sheet's, else as many as there are cells up to
    /// three (four go two by two).
    pub fn columnsFor(&self, look: &Rc<RefCell<MdGalleryLook>>) -> i64 {
        let n: i64 = self.items.len() as i64;
        if look.borrow().columns > 0 {
            return look.borrow().columns;
        }
        if n == 4 {
            return 2;
        }
        if n < 1 {
            return 1;
        }
        if n <= 3 {
            return n;
        }
        3
    }
    /// How many across in a room `w` × `h` (a slide's), when the sheet does
    /// not say: the count that shows the most of the pictures — each cell's
    /// area times the share of its picture a `cover` crop keeps. Four
    /// portraits go four across rather than two by two in wide cells that
    /// cut their heads off; four landscapes still go two by two. `aspects`
    /// is each cell's picture's width / height (0: not known, a text cell or
    /// a span: the count from the number of cells, `columnsFor`). `capH` is
    /// the room a caption under a picture takes, `minRow` the lowest row.
    pub fn columnsForRoom(&self, look: &Rc<RefCell<MdGalleryLook>>, aspects: &[f64], w: f64, h: f64, gap: f64, cap_h: f64, min_row: f64) -> i64 {
        let n: i64 = self.items.len() as i64;
        if look.borrow().columns > 0 || n < 2 {
            return self.columnsFor(look);
        }
        let mut i: i64 = 0;
        while i < n {
            let mut it: Rc<RefCell<MdGalleryItem>> = self.items[i as usize].clone();
            if (it.borrow().isText || it.borrow().span > 1) || aspects[i as usize] <= 0.0 {
                return self.columnsFor(look);
            }
            i += 1;
        }
        let mut best: i64 = self.columnsFor(look);
        let mut best_score: f64 = -1.0;
        let mut most: i64 = n;
        if most > 6 {
            most = 6;
        }
        let mut c: i64 = 1;
        while c <= most {
            let r: i64 = (n + (c - 1)) / c;
            let row_h: f64 = (h - gap * ((r - 1) as f64)) / (r as f64);
            let cell_w: f64 = (w - gap * ((c - 1) as f64)) / (c as f64);
            let box_h: f64 = row_h - cap_h;
            if (row_h >= min_row || c == most) && (box_h > 0.0 && cell_w > 0.0) {
                let br: f64 = cell_w / box_h;
                let mut score: f64 = 0.0;
                let mut k: i64 = 0;
                while k < n {
                    let pr: f64 = aspects[k as usize];
                    let mut keep: f64 = pr / br;
                    if keep > 1.0 {
                        keep = br / pr;
                    }
                    score += (cell_w * box_h) * keep;
                    k += 1;
                }
                if score > best_score * 1.02 {
                    best_score = score;
                    best = c;
                }
            }
            c += 1;
        }
        best
    }
    /// The rows: item indexes (from 0), each row filled up to `cols` columns
    /// by the cells' spans. A cell wider than the grid is as wide as it.
    pub fn rows(&self, cols: i64) -> Vec<Vec<i64>> {
        let mut out: Vec<Vec<i64>> = Vec::new();
        let mut row: Vec<i64> = Vec::new();
        let mut used: i64 = 0;
        let mut i: i64 = 0;
        while i < (self.items.len() as i64) {
            let mut it: Rc<RefCell<MdGalleryItem>> = self.items[i as usize].clone();
            let s: i64 = MdGallery::spanIn(&it, cols);
            if used + s > cols && !row.is_empty() {
                out.push(row.clone());
                let mut fresh: Vec<i64> = Vec::new();
                row = fresh.clone();
                used = 0;
            }
            row.push(i);
            used += s;
            i += 1;
        }
        if !row.is_empty() {
            out.push(row.clone());
        }
        out
    }
    /// The width of each cell of a row across `w`: in proportion to its
    /// span, the row's gaps taken out, so a short last row fills the width
    /// too rather than leaving a hole.
    pub fn rowWidths(&self, row: &[i64], cols: i64, w: f64, gap: f64) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        let mut total: i64 = 0;
        let mut i: i64 = 0;
        while i < (row.len() as i64) {
            total += MdGallery::spanIn(&self.items[(row[i as usize]) as usize].clone(), cols);
            i += 1;
        }
        let n: i64 = row.len() as i64;
        let mut room: f64 = w - gap * ((n - 1) as f64);
        if room < 0.0 {
            room = 0.0;
        }
        let mut k: i64 = 0;
        while k < n {
            let s: i64 = MdGallery::spanIn(&self.items[(row[k as usize]) as usize].clone(), cols);
            out.push(room * ((s as f64) / (total as f64)));
            k += 1;
        }
        out
    }
    /// One cell at `x, y`, `w` × `h`: where its picture and its caption go.
    /// `capH` is how tall its caption's lines are (at the width `captionWidth`
    /// gives), `natW × natH` the picture's own pixels (0: unknown).
    pub fn placeCell(&self, it: &Rc<RefCell<MdGalleryItem>>, idx: i64, x: f64, y: f64, w: f64, h: f64, cl: &Rc<RefCell<MdCellLook>>, cap_look: &Rc<RefCell<MdCellLook>>, cap_h: f64, nat_w: f64, nat_h: f64) -> Rc<RefCell<MdGalleryCell>> {
        let mut c: Rc<RefCell<MdGalleryCell>> = Rc::new(RefCell::new(MdGalleryCell::new()));
        c.borrow_mut().item = idx;
        c.borrow_mut().x = x;
        c.borrow_mut().y = y;
        c.borrow_mut().w = w;
        c.borrow_mut().h = h;
        let ix: f64 = x + cl.borrow().padL;
        let iy: f64 = y + cl.borrow().padT;
        let mut iw: f64 = (w - cl.borrow().padL) - cl.borrow().padR;
        let mut ih: f64 = (h - cl.borrow().padT) - cl.borrow().padB;
        if iw < 1.0 {
            iw = 1.0;
        }
        if ih < 1.0 {
            ih = 1.0;
        }
        if it.borrow().isText {
            c.borrow_mut().capX = ix;
            c.borrow_mut().capY = iy;
            c.borrow_mut().capW = iw;
            c.borrow_mut().capH = ih;
            return c;
        }
        let has_cap: bool = !it.borrow().caption.is_empty() && self.caption != "none";
        let mut box_h: f64 = ih;
        let mut band: f64 = (cap_h + cap_look.borrow().padT) + cap_look.borrow().padB;
        if has_cap && self.caption == "below" {
            // the caption under the picture, inside the cell (a polaroid's
            // deep bottom padding is where it sits); never more than half
            if band > ih * 0.5 {
                band = ih * 0.5;
            }
            box_h = ih - band;
            c.borrow_mut().capX = ix + cap_look.borrow().padL;
            c.borrow_mut().capY = (iy + box_h) + cap_look.borrow().padT;
            c.borrow_mut().capW = (iw - cap_look.borrow().padL) - cap_look.borrow().padR;
            c.borrow_mut().capH = cap_h;
        }
        let mut _tmp_1852 = self.fit.clone();
        let mut _tmp_1853 = it.borrow().focusX.clone();
        MdGallery::fitPicture(&c, &_tmp_1852, _tmp_1853, it.borrow().focusY.clone(), nat_w, nat_h, ix, iy, iw, box_h);
        if has_cap && self.caption == "overlay" {
            if band > box_h * 0.6 {
                band = box_h * 0.6;
            }
            c.borrow_mut().dim = true;
            c.borrow_mut().dimX = ix;
            c.borrow_mut().dimW = iw;
            c.borrow_mut().dimY = (iy + box_h) - band;
            c.borrow_mut().dimH = band;
            c.borrow_mut().capX = ix + cap_look.borrow().padL;
            let capY_v = c.borrow().dimY + cap_look.borrow().padT;
            c.borrow_mut().capY = capY_v;
            c.borrow_mut().capW = (iw - cap_look.borrow().padL) - cap_look.borrow().padR;
            c.borrow_mut().capH = cap_h;
        }
        c
    }
}
