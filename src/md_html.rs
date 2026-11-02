// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::rgr::MdText;

/// ============================================================================
/// MdHtml.rgr — the HTML a markdown document carries, read far enough to draw
/// ============================================================================
///
/// CommonMark passes HTML through untouched: `<mark>` in a sentence is an
/// `html_inline` node, a `<table>` or a `<div align="center">` block is an
/// `html_block`. A browser would render them; a canvas has nothing to hand
/// them to, so the layout used to show the markup itself. This reads the part
/// of HTML that documents actually use — a tag's name and attributes, the
/// `text-align` in a style, and a table with its row and column spans — and
/// leaves everything else to be shown as it was.
/// ============================================================================
#[derive(Clone)]
pub struct MdHtmlTag {
    pub name: String,
    pub close: bool,
    pub selfClose: bool,
    pub keys: Vec<String>,
    pub vals: Vec<String>,
    /// the name and keys as written: SVG tells `viewBox` from `viewbox`
    pub rawName: String,
    pub rawKeys: Vec<String>,
    /// one past the `>` in the text it was read from
    pub end: i64,
}

impl MdHtmlTag {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            close: false,
            selfClose: false,
            keys: Vec::new(),
            vals: Vec::new(),
            rawName: String::new(),
            rawKeys: Vec::new(),
            end: 0,
        }
    }
    pub fn attr(&self, k: &str) -> String {
        let mut i: i64 = 0;
        while i < (self.keys.len() as i64) {
            if self.keys[i as usize].clone() == k {
                return self.vals[i as usize].clone();
            }
            i += 1;
        }
        String::new()
    }
    pub fn has(&self, k: &str) -> bool {
        let mut i: i64 = 0;
        while i < (self.keys.len() as i64) {
            if self.keys[i as usize].clone() == k {
                return true;
            }
            i += 1;
        }
        false
    }
    /// left / center / right from `align=` or `style="text-align: …"`, or "".
    pub fn align(&self) -> String {
        let mut a: String = MdHtml::lower(&self.attr("align").trim().to_string());
        if a.is_empty() {
            a = MdHtml::lower(&MdHtml::styleProp(&self.attr("style"), "text-align"));
        }
        if a == "middle" {
            a = "center".to_string();
        }
        if (a == "left" || a == "center") || a == "right" {
            return a;
        }
        String::new()
    }
}

#[derive(Clone)]
pub struct MdHtmlCell {
    pub inner: String,
    /// where `inner` starts in the text the table was read from
    pub start: i64,
    pub header: bool,
    pub rowspan: i64,
    pub colspan: i64,
    pub align: String,
    /// its `style=""` and `bgcolor=`, as written
    pub style: String,
    pub bgcolor: String,
    /// the colour behind it once the layout has read those, "" for none
    pub fill: String,
    /// where it lands in the grid
    pub row: i64,
    pub col: i64,
}

impl MdHtmlCell {
    pub fn new() -> Self {
        Self {
            inner: String::new(),
            start: 0,
            header: false,
            rowspan: 1,
            colspan: 1,
            align: String::new(),
            style: String::new(),
            bgcolor: String::new(),
            fill: String::new(),
            row: 0,
            col: 0,
        }
    }
}

#[derive(Clone)]
pub struct MdHtmlRow {
    pub cells: Vec<Rc<RefCell<MdHtmlCell>>>,
    pub head: bool,
}

impl MdHtmlRow {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            head: false,
        }
    }
}

#[derive(Clone)]
pub struct MdHtmlTable {
    pub rows: Vec<Rc<RefCell<MdHtmlRow>>>,
    pub cols: i64,
    pub caption: String,
}

impl MdHtmlTable {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            cols: 0,
            caption: String::new(),
        }
    }
    pub fn rowCount(&self) -> i64 {
        self.rows.len() as i64
    }
}

#[derive(Clone)]
pub struct MdHtml {
}

impl MdHtml {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn lower(s: &str) -> String {
        MdChar::lower(s)
    }
    /// `text-align` out of "color: red; text-align: center".
    pub fn styleProp(style: &str, prop: &str) -> String {
        let mut parts: Vec<String> = style.split(";").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut i: i64 = 0;
        while i < (parts.len() as i64) {
            let p: String = parts[i as usize].clone();
            let colon: i64 = MdText::index_of(&p, ":");
            if colon > 0 {
                let k: String = MdHtml::lower(&MdText::substr(&p, 0, colon).trim().to_string());
                if k == prop {
                    return MdText::substr(&p, colon + 1, (MdText::str_len(&p) as i64)).trim().to_string();
                }
            }
            i += 1;
        }
        String::new()
    }
    pub fn isNameChar(c: i64) -> bool {
        if (c >= 97 && c <= 122) {
            return true;
        }
        if (c >= 65 && c <= 90) {
            return true;
        }
        if (c >= 48 && c <= 57) {
            return true;
        }
        if c == 45 || c == 58 {
            return true;
        }
        false
    }
    /// The tag that starts at `from` (a `<`). An empty name when there is none.
    pub fn tagAt(s: &str, from: i64) -> Rc<RefCell<MdHtmlTag>> {
        let mut t: Rc<RefCell<MdHtmlTag>> = Rc::new(RefCell::new(MdHtmlTag::new()));
        let n: i64 = MdText::str_len(&s) as i64;
        let mut i: i64 = from + 1;
        if i < n && MdText::char_at(&s, i) == 47 {
            t.borrow_mut().close = true;
            i += 1;
        }
        let ns: i64 = i;
        while i < n && MdHtml::isNameChar(MdText::char_at(&s, i)) {
            i += 1;
        }
        t.borrow_mut().rawName = MdText::substr(&s, ns, i);
        let _tmp_1904 = MdHtml::lower(&t.borrow().rawName.clone());
        t.borrow_mut().name = _tmp_1904;
        // attributes, until `>`
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            if c == 62 {
                t.borrow_mut().end = i + 1;
                return t;
            }
            if c == 47 {
                t.borrow_mut().selfClose = true;
                i += 1;
            } else {
                if MdHtml::isNameChar(c) {
                    let ks: i64 = i;
                    while i < n && MdHtml::isNameChar(MdText::char_at(&s, i)) {
                        i += 1;
                    }
                    let raw_key: String = MdText::substr(&s, ks, i);
                    let key: String = MdHtml::lower(&raw_key);
                    let mut val: String = String::new();
                    while i < n && MdText::char_at(&s, i) == 32 {
                        i += 1;
                    }
                    if i < n && MdText::char_at(&s, i) == 61 {
                        i += 1;
                        while i < n && MdText::char_at(&s, i) == 32 {
                            i += 1;
                        }
                        if i < n && (MdText::char_at(&s, i) == 34 || MdText::char_at(&s, i) == 39) {
                            let q: i64 = MdText::char_at(&s, i);
                            i += 1;
                            let vs: i64 = i;
                            while i < n && MdText::char_at(&s, i) != q {
                                i += 1;
                            }
                            val = MdText::substr(&s, vs, i);
                            i += 1;
                        } else {
                            let vs2: i64 = i;
                            while i < n && (MdText::char_at(&s, i) != 32 && MdText::char_at(&s, i) != 62) {
                                i += 1;
                            }
                            val = MdText::substr(&s, vs2, i);
                        }
                    }
                    t.borrow_mut().keys.push(key.clone());
                    t.borrow_mut().rawKeys.push(raw_key.clone());
                    t.borrow_mut().vals.push(val.clone());
                } else {
                    i += 1;
                }
            }
        }
        t.borrow_mut().end = n;
        t
    }
    pub fn tag(lit: &str) -> Rc<RefCell<MdHtmlTag>> {
        let s: String = lit.trim().to_string();
        if s.is_empty() || MdText::char_at(&s, 0) != 60 {
            return Rc::new(RefCell::new(MdHtmlTag::new()));
        }
        MdHtml::tagAt(&s, 0)
    }
    /// The next `<` at or after `from` that opens a tag (a letter or `/`
    /// after it), or -1.
    pub fn nextTag(s: &str, from: i64) -> i64 {
        let n: i64 = MdText::str_len(&s) as i64;
        let mut i: i64 = from;
        while i < n {
            if MdText::char_at(&s, i) == 60 {
                if i + 1 < n {
                    let c: i64 = MdText::char_at(&s, i + 1);
                    if c == 47 || MdHtml::isNameChar(c) {
                        return i;
                    }
                }
            }
            i += 1;
        }
        -1
    }
    /// Where the `</name>` that closes the element opened before `from` begins,
    /// counting nested ones of the same name; -1 when it is not there.
    pub fn closingOf(s: &str, name: &str, from: i64) -> i64 {
        let mut depth: i64 = 1;
        let mut i: i64 = MdHtml::nextTag(s, from);
        while i >= 0 {
            let mut t: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(s, i);
            if t.borrow().name == name {
                if t.borrow().close {
                    depth -= 1;
                    if depth == 0 {
                        return i;
                    }
                } else {
                    if !t.borrow().selfClose {
                        depth += 1;
                    }
                }
            }
            i = MdHtml::nextTag(s, t.borrow().end);
        }
        -1
    }
    /// A `<table>`: its rows and cells, spans placed on a grid.
    pub fn table(s: &str) -> Rc<RefCell<MdHtmlTable>> {
        let mut t: Rc<RefCell<MdHtmlTable>> = Rc::new(RefCell::new(MdHtmlTable::new()));
        let mut in_head: bool = false;
        let mut row: Option<Rc<RefCell<MdHtmlRow>>> = None;
        let mut cell: Option<Rc<RefCell<MdHtmlCell>>> = None;
        let mut seen_table: bool = false;
        let mut i: i64 = MdHtml::nextTag(s, 0);
        while i >= 0 {
            let mut tg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(s, i);
            let nm: String = tg.borrow().name.clone();
            // a table inside a cell is that cell's content: its rows are
            // not this table's
            if (nm == "table" && !tg.borrow().close) && seen_table {
                let inner: i64 = MdHtml::closingOf(s, "table", tg.borrow().end.clone());
                if inner < 0 {
                    i = -1;
                } else {
                    let mut itg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(s, inner);
                    i = MdHtml::nextTag(s, itg.borrow().end);
                }
                continue;
            }
            if nm == "table" && !tg.borrow().close {
                seen_table = true;
            }
            let mut closes_cell: bool = false;
            if nm == "td" || nm == "th" {
                closes_cell = true;
            }
            if nm == "tr" || (nm == "thead" || (nm == "tbody" || (nm == "tfoot" || nm == "table"))) {
                closes_cell = true;
            }
            // the cell is let go after the block that narrowed it: assigned
            // inside, Ranger's C++ writes `cell.value() = none`
            let mut closed: bool = false;
            if closes_cell && cell.is_some() {
                let mut c0: Rc<RefCell<MdHtmlCell>> = cell.clone().unwrap();
                let inner_v = MdText::substr(&s, c0.borrow().start, i);
                c0.borrow_mut().inner = inner_v;
                closed = true;
            }
            if closed {
                let mut none: Option<Rc<RefCell<MdHtmlCell>>> = None;
                cell = none.clone();
            }
            if nm == "thead" {
                in_head = !tg.borrow().close;
            }
            if nm == "tr" && !tg.borrow().close {
                let mut r: Rc<RefCell<MdHtmlRow>> = Rc::new(RefCell::new(MdHtmlRow::new()));
                r.borrow_mut().head = in_head;
                t.borrow_mut().rows.push(r.clone());
                row = Some(r.clone());
            }
            if (nm == "td" || nm == "th") && !tg.borrow().close {
                if row.is_none() {
                    let mut r2: Rc<RefCell<MdHtmlRow>> = Rc::new(RefCell::new(MdHtmlRow::new()));
                    r2.borrow_mut().head = in_head;
                    t.borrow_mut().rows.push(r2.clone());
                    row = Some(r2.clone());
                }
                let mut c: Rc<RefCell<MdHtmlCell>> = Rc::new(RefCell::new(MdHtmlCell::new()));
                c.borrow_mut().header = nm == "th";
                c.borrow_mut().start = tg.borrow().end;
                c.borrow_mut().rowspan = if ((tg.borrow().attr("rowspan").trim().to_string().parse::<i64>().ok()).is_some()) { ((tg.borrow().attr("rowspan").trim().to_string().parse::<i64>().ok()).unwrap()) } else { 1 };
                c.borrow_mut().colspan = if ((tg.borrow().attr("colspan").trim().to_string().parse::<i64>().ok()).is_some()) { ((tg.borrow().attr("colspan").trim().to_string().parse::<i64>().ok()).unwrap()) } else { 1 };
                if c.borrow().rowspan < 1 {
                    c.borrow_mut().rowspan = 1;
                }
                if c.borrow().colspan < 1 {
                    c.borrow_mut().colspan = 1;
                }
                if c.borrow().rowspan > 64 {
                    c.borrow_mut().rowspan = 64;
                }
                if c.borrow().colspan > 64 {
                    c.borrow_mut().colspan = 64;
                }
                c.borrow_mut().align = tg.borrow().align();
                c.borrow_mut().style = tg.borrow().attr("style");
                c.borrow_mut().bgcolor = tg.borrow().attr("bgcolor");
                let mut rr: Rc<RefCell<MdHtmlRow>> = row.clone().unwrap();
                rr.borrow_mut().cells.push(c.clone());
                cell = Some(c.clone());
            }
            if nm == "caption" && !tg.borrow().close {
                let ce: i64 = MdHtml::closingOf(s, "caption", tg.borrow().end.clone());
                if ce > 0 {
                    t.borrow_mut().caption = MdText::substr(&s, tg.borrow().end, ce).trim().to_string();
                }
            }
            i = MdHtml::nextTag(s, tg.borrow().end);
        }
        if cell.is_some() {
            let mut c1: Rc<RefCell<MdHtmlCell>> = cell.clone().unwrap();
            let inner_v = MdText::substr(&s, c1.borrow().start, (MdText::str_len(&s) as i64));
            c1.borrow_mut().inner = inner_v;
        }
        MdHtml::place(&t);
        t
    }
    /// Row and column of every cell, stepping past the places taken by a cell
    /// above that spans down into this row.
    pub fn place(t: &Rc<RefCell<MdHtmlTable>>) {
        let mut taken: HashMap<String,bool> = HashMap::new();
        let mut cols: i64 = 0;
        let mut r: i64 = 0;
        while r < (t.borrow().rows.len() as i64) {
            let mut row: Rc<RefCell<MdHtmlRow>> = t.borrow().rows[r as usize].clone();
            let mut c: i64 = 0;
            let mut k: i64 = 0;
            while k < (row.borrow().cells.len() as i64) {
                let mut cell: Rc<RefCell<MdHtmlCell>> = row.borrow().cells[k as usize].clone();
                while taken.contains_key(&MdHtml::key(r, c)) {
                    c += 1;
                }
                cell.borrow_mut().row = r;
                cell.borrow_mut().col = c;
                let mut dr: i64 = 0;
                while dr < cell.borrow().rowspan {
                    let mut dc: i64 = 0;
                    while dc < cell.borrow().colspan {
                        taken.insert(MdHtml::key(r + dr, c + dc), true);
                        dc += 1;
                    }
                    dr += 1;
                }
                c += cell.borrow().colspan;
                if c > cols {
                    cols = c;
                }
                k += 1;
            }
            r += 1;
        }
        t.borrow_mut().cols = cols;
    }
    pub fn key(r: i64, c: i64) -> String {
        format!("{}:{}", r.to_string(), c)
    }
}
