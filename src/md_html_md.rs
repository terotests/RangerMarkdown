// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_html::MdHtml;
use crate::md_html::MdHtmlTag;
use crate::rgr::MdText;

/// ============================================================================
/// MdHtmlMd.rgr — HTML a deck carries, said again in what the layout draws
/// ============================================================================
///
/// A deck written by a person, or by an assistant, mixes HTML into its
/// Markdown: `<ul><li>…</li></ul>` for a list, `style="font-size:32pt"` on a
/// span, `style="background:#123;padding:20px"` on a div. The layout draws
/// Markdown, so what such HTML means is read here, without measuring or
/// drawing anything:
///
///   `listItems` / `listMarkdown` — an `<ul>` / `<ol start>` with its `<li>`
///     items (nested lists too) as Markdown list lines, which the layout then
///     lays out like any list; a table cell lists the same items as lines.
///   `MdHtmlStyle.read` — a `style=""` split into what a run of text or a
///     block can show (colour, background, size, weight, slant, a line under
///     or through, padding, rounding) and the properties nothing reads, which
///     the layout reports rather than dropping in silence.
/// ============================================================================
/// one `<li>`: how deep its list is (0 for the outer one), the marker it is
/// written with in Markdown (`-`, `3.`) and its content, on one line
#[derive(Clone)]
pub struct MdHtmlItem {
    pub depth: i64,
    pub marker: String,
    pub text: String,
}

impl MdHtmlItem {
    pub fn new() -> Self {
        Self {
            depth: 0,
            marker: "-".to_string(),
            text: String::new(),
        }
    }
}

/// a list read out of HTML: its items, and one past its closing tag (the
/// end of the text when it is not closed)
#[derive(Clone)]
pub struct MdHtmlList {
    pub items: Vec<Rc<RefCell<MdHtmlItem>>>,
    pub end: i64,
}

impl MdHtmlList {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            end: 0,
        }
    }
}

/// What `style=""` says, in the words the layout uses. "" where it says
/// nothing.
#[derive(Clone)]
pub struct MdHtmlStyle {
    pub color: String,
    /// `background` / `background-color`: the colour, as written
    pub fill: String,
    /// `font-size` as written (`32pt`, `1.5em`, `120%`, `large`)
    pub size: String,
    /// "bold" / "normal"
    pub weight: String,
    /// "italic" / "normal"
    pub slant: String,
    /// 1 on, -1 off (`text-decoration: none`), 0 not said
    pub underline: i64,
    pub strike: i64,
    pub padding: String,
    pub radius: String,
    /// the properties this context does not read, in the order written
    pub ignored: Vec<String>,
}

impl MdHtmlStyle {
    pub fn new() -> Self {
        Self {
            color: String::new(),
            fill: String::new(),
            size: String::new(),
            weight: String::new(),
            slant: String::new(),
            underline: 0,
            strike: 0,
            padding: String::new(),
            radius: String::new(),
            ignored: Vec::new(),
        }
    }
    /// `style` read for a context that reads the properties in `allowed`
    /// (names separated by spaces); every other property is `ignored`.
    pub fn read(style: &str, allowed: &str) -> Rc<RefCell<MdHtmlStyle>> {
        let mut out: Rc<RefCell<MdHtmlStyle>> = Rc::new(RefCell::new(MdHtmlStyle::new()));
        let mut parts: Vec<String> = style.split(";").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut i: i64 = 0;
        while i < (parts.len() as i64) {
            let p: String = parts[i as usize].clone();
            i += 1;
            let colon: i64 = MdText::index_of(&p, ":");
            if colon <= 0 {
                continue;
            }
            let k: String = MdChar::lower(&MdText::substr(&p, 0, colon).trim().to_string());
            let mut v: String = MdText::substr(&p, colon + 1, (MdText::str_len(&p) as i64)).trim().to_string();
            // `!important` changes nothing here
            let bang: i64 = MdText::index_of(&v, "!");
            if bang >= 0 {
                v = MdText::substr(&v, 0, bang).trim().to_string();
            }
            if v.is_empty() {
                continue;
            }
            if MdText::index_of(&format!(" {} ", allowed), &format!(" {} ", k)) < 0 {
                out.borrow_mut().ignored.push(k.clone());
                continue;
            }
            let lv: String = MdChar::lower(&v);
            if k == "color" {
                out.borrow_mut().color = v.clone();
            }
            if k == "background" || k == "background-color" {
                out.borrow_mut().fill = MdHtmlStyle::colourOf(&v);
                if out.borrow().fill.is_empty() {
                    out.borrow_mut().ignored.push(k.clone());
                }
            }
            if k == "font-size" {
                out.borrow_mut().size = lv.clone();
            }
            if k == "font-weight" {
                out.borrow_mut().weight = MdHtmlStyle::weightOf(&lv);
                if out.borrow().weight.is_empty() {
                    out.borrow_mut().ignored.push(k.clone());
                }
            }
            if k == "font-style" {
                if lv == "italic" || MdText::index_of(&lv, "oblique") == 0 {
                    out.borrow_mut().slant = "italic".to_string();
                }
                if lv == "normal" {
                    out.borrow_mut().slant = "normal".to_string();
                }
            }
            if k == "text-decoration" || k == "text-decoration-line" {
                let mut words: Vec<String> = lv.split(" ").map(|s| s.to_string()).collect::<Vec<String>>();
                for w in words.iter().cloned() {
                    if w == "underline" {
                        out.borrow_mut().underline = 1;
                    }
                    if w == "line-through" {
                        out.borrow_mut().strike = 1;
                    }
                    if w == "none" {
                        out.borrow_mut().underline = -1;
                        out.borrow_mut().strike = -1;
                    }
                }
            }
            if k == "padding" {
                out.borrow_mut().padding = lv.clone();
            }
            if k == "border-radius" {
                out.borrow_mut().radius = lv.clone();
            }
        }
        out
    }
    /// the colour in a `background` shorthand: its first word when that is a
    /// colour (`#1d2a6b`, `rgb(…)`, `red`), "" for a picture or a gradient
    pub fn colourOf(v: &str) -> String {
        let mut t: String = v.trim().to_string();
        let mut lv: String = MdChar::lower(&t);
        if MdText::index_of(&lv, "gradient(") >= 0 {
            return String::new();
        }
        // `#123 url(x.png) no-repeat`: the colour under the picture
        let u: i64 = MdText::index_of(&lv, "url(");
        if u >= 0 {
            let ue: i64 = MdText::index_of_from(&t, ")", u);
            if ue < 0 {
                return String::new();
            }
            t = format!("{} {}", MdText::substr(&t, 0, u), MdText::substr(&t, ue + 1, (MdText::str_len(&t) as i64)))
                .trim()
                .to_string();
            lv = MdChar::lower(&t);
        }
        if t.is_empty() {
            return String::new();
        }
        if MdText::index_of(&lv, "rgb") == 0 || MdText::index_of(&lv, "hsl") == 0 {
            let close: i64 = MdText::index_of(&t, ")");
            if close > 0 {
                return MdText::substr(&t, 0, close + 1);
            }
            return String::new();
        }
        let sp: i64 = MdText::index_of(&t, " ");
        if sp > 0 {
            return MdText::substr(&t, 0, sp);
        }
        t
    }
    /// "bold", "normal" or "" for a weight that is neither word nor number
    pub fn weightOf(v: &str) -> String {
        if v == "bold" || v == "bolder" {
            return "bold".to_string();
        }
        if v == "normal" || v == "lighter" {
            return "normal".to_string();
        }
        let n: Option<i64> = v.parse::<i64>().ok();
        if n.is_none() {
            return String::new();
        }
        if n.unwrap() >= 600 {
            return "bold".to_string();
        }
        "normal".to_string()
    }
}

#[derive(Clone)]
pub struct MdHtmlMd {
}

impl MdHtmlMd {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The `<ul>` or `<ol>` that starts at `from` (its `<`), with the lists
    /// inside its items. Text between tags belongs to the item it is in;
    /// inline tags stay in that text (the layout reads them as it reads any
    /// inline HTML), `<p>` and `<div>` inside an item are only spaces.
    pub fn listItems(s: &str, from: i64) -> Rc<RefCell<MdHtmlList>> {
        let mut out: Rc<RefCell<MdHtmlList>> = Rc::new(RefCell::new(MdHtmlList::new()));
        let n: i64 = MdText::str_len(&s) as i64;
        out.borrow_mut().end = n;
        // per open list: ordered or not, and the number of its next item
        let mut ordered: Vec<bool> = Vec::new();
        let mut next: Vec<i64> = Vec::new();
        let mut text: String = String::new();
        let mut open: bool = false;
        let mut depth: i64 = 0;
        let mut marker: String = "-".to_string();
        let mut at: i64 = from;
        let mut i: i64 = MdHtml::nextTag(s, from);
        while i >= 0 {
            let mut tg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(s, i);
            let nm: String = tg.borrow().name.clone();
            let between: String = MdText::substr(&s, at, i);
            let mut structural: bool = false;
            if (nm == "ul" || nm == "ol") || nm == "li" {
                structural = true;
            }
            if structural {
                if open {
                    text = format!("{}{}", text, between);
                }
            } else {
                if open {
                    text = format!("{}{}", text, between);
                    if (nm == "p" || nm == "div") || nm == "br" {
                        text = format!("{} ", text);
                    } else {
                        text = format!("{}{}", text, MdText::substr(&s, i, tg.borrow().end));
                    }
                }
            }
            if structural {
                // an item ends where the next tag of the list starts
                if open {
                    MdHtmlMd::addItem(&out, depth, marker.clone(), &text);
                    open = false;
                    text = String::new();
                }
                if (nm == "ul" || nm == "ol") && !tg.borrow().close {
                    ordered.push(nm == "ol");
                    let mut first: i64 = 1;
                    if nm == "ol" {
                        let st: Option<i64> = tg.borrow().attr("start")
                            .trim()
                            .to_string()
                            .parse::<i64>()
                            .ok();
                        if st.is_some() {
                            first = st.unwrap();
                        }
                    }
                    next.push(first);
                }
                if (nm == "ul" || nm == "ol") && tg.borrow().close {
                    if !ordered.is_empty() {
                        ordered.remove(((ordered.len() as i64) - 1) as usize);
                        next.remove(((next.len() as i64) - 1) as usize);
                    }
                    if ordered.is_empty() {
                        out.borrow_mut().end = tg.borrow().end;
                        return out;
                    }
                }
                if (nm == "li" && !tg.borrow().close) && !ordered.is_empty() {
                    let top: i64 = (ordered.len() as i64) - 1;
                    depth = top;
                    marker = "-".to_string();
                    if ordered[top as usize] {
                        let num: i64 = next[top as usize];
                        marker = format!("{}.", num.to_string());
                        next[top as usize] = num + 1;
                    }
                    open = true;
                    text = String::new();
                }
            }
            at = tg.borrow().end;
            i = MdHtml::nextTag(s, tg.borrow().end);
        }
        if open {
            MdHtmlMd::addItem(
                &out,
                depth,
                marker.clone(),
                &format!("{}{}", text, MdText::substr(&s, at, n))
            );
        }
        out
    }
    pub fn addItem(out: &Rc<RefCell<MdHtmlList>>, depth: i64, marker: String, text: &str) {
        let mut it: Rc<RefCell<MdHtmlItem>> = Rc::new(RefCell::new(MdHtmlItem::new()));
        it.borrow_mut().depth = depth;
        it.borrow_mut().marker = marker.clone();
        it.borrow_mut().text = MdHtmlMd::oneLine(text);
        out.borrow_mut().items.push(it.clone());
    }
    /// The items as Markdown list lines, a nested item indented under the
    /// text of the item above it.
    pub fn listMarkdown(list: &Rc<RefCell<MdHtmlList>>) -> String {
        let mut lines: Vec<String> = Vec::new();
        // the indent of each depth, from the markers above it
        let mut indent: Vec<String> = Vec::new();
        indent.push(String::new().to_string());
        let __it_it = list.borrow().items.clone();
        for it in __it_it.iter().cloned() {
            while (indent.len() as i64) <= it.borrow().depth {
                indent.push(indent[((indent.len() as i64) - 1) as usize].clone());
            }
            let ind: String = indent[(it.borrow().depth) as usize].clone();
            lines.push(format!("{}{} {}", ind, it.borrow().marker, it.borrow().text));
            let mut under: String = ind.clone();
            let mut m: i64 = 0;
            while m <= (MdText::str_len(&it.borrow().marker) as i64) {
                under = format!("{} ", under);
                m += 1;
            }
            while (indent.len() as i64) > it.borrow().depth + 1 {
                indent.remove(((indent.len() as i64) - 1) as usize);
            }
            indent.push(under.clone());
        }
        lines.join(&*"\n".to_string())
    }
    /// runs of blanks and line breaks as one space, trimmed
    pub fn oneLine(s: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&s) as i64;
        let mut from: i64 = 0;
        let mut blank: bool = true;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            let is_blank: bool = (c == 32 || c == 9) || (c == 10 || c == 13);
            if is_blank {
                if !blank {
                    parts.push(MdText::substr(&s, from, i));
                }
                blank = true;
            } else {
                if blank {
                    from = i;
                }
                blank = false;
            }
            i += 1;
        }
        if !blank {
            parts.push(MdText::substr(&s, from, n));
        }
        parts.join(&*" ".to_string())
    }
}
