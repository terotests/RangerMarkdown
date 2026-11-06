// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_node::MdNode;
use crate::rgr::MdText;

/// ============================================================================
/// MdSlideLayout.rgr — where a slide's content sits: at the top, centred
/// under the title, or the whole slide centred.
/// ============================================================================
///
/// ````markdown
/// ## Osa 2: tulokset {layout=section}
///
/// ## Yksi lause keskellä {valign=center}
///
/// ## Viesti {layout=statement}
///
/// Iso keskitetty viesti.
/// ````
///
/// On a slide's title (or `valign:` / `layout:` in the front matter, for every
/// slide that says nothing itself):
///
///   valign=center | bottom   the content under the title is set in the
///                            middle (at the foot) of the room under it;
///                            `layout=center` is the same as valign=center
///   layout=section | title   the title and what is under it, together, in
///                            the middle of the slide, the lines centred
///   layout=statement         as section, and the text under the title is set
///                            at the title's size; a statement with text
///                            under it hides its title (it still names the
///                            slide), one without is its title, large
///
/// `layout=columns | comparison | two-column | image-right | image-left` put
/// the content side by side (MdColumns) and are no business of this file.
///
/// Only geometry: `MdLayout.alignSlides` collects the boxes and moves them.
/// ============================================================================
#[derive(Clone)]
pub struct MdSlideLayout {
}

impl MdSlideLayout {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The word that places slide `heading`'s content: "" (the top),
    /// "center", "bottom", "section", "statement". `deck` is the front
    /// matter's, for a title that says nothing.
    pub fn wordOf(heading: &Rc<RefCell<MdNode>>, deck: &str) -> String {
        let lay: String = MdChar::lower(&heading.borrow().attrValue("layout").trim().to_string());
        if lay == "section" || lay == "title" {
            return "section".to_string();
        }
        if lay == "statement" {
            return "statement".to_string();
        }
        if lay == "center" || lay == "middle" {
            return "center".to_string();
        }
        // said on the slide, `valign=top` too, wins over the deck's
        if heading.borrow().hasAttr("valign") {
            return MdSlideLayout::valignOf(&heading.borrow().attrValue("valign"));
        }
        if !lay.is_empty() {
            return String::new();
        }
        deck.to_string()
    }
    /// `valign=` as a word of this file's: "center", "bottom", "" (top)
    pub fn valignOf(v0: &str) -> String {
        let v: String = MdChar::lower(&v0.trim().to_string());
        if (v == "center" || v == "middle") || v == "centre" {
            return "center".to_string();
        }
        if v == "bottom" || v == "end" {
            return "bottom".to_string();
        }
        String::new()
    }
    /// the whole slide moves (and its lines are centred), not only what is
    /// under the title
    pub fn whole(word: &str) -> bool {
        word == "section" || word == "statement"
    }
    /// How far down the boxes `top`..`bottom` move. `whole`: they include the
    /// title and go to the middle of contentTop..contentBottom. Otherwise they
    /// are what is under the title and go to the middle (`center`) or the
    /// foot (`bottom`) of the room left under it; they never move up.
    pub fn shift(word: &str, top: f64, bottom: f64, content_top: f64, content_bottom: f64) -> f64 {
        if word.is_empty() {
            return 0.0;
        }
        if bottom <= top {
            return 0.0;
        }
        let free: f64 = content_bottom - bottom;
        if free <= 0.0 {
            return 0.0;
        }
        if MdSlideLayout::whole(word) {
            let d: f64 = (free - (top - content_top)) * 0.5;
            if d < 0.0 {
                return 0.0;
            }
            return d;
        }
        if word == "bottom" {
            return free;
        }
        free * 0.5
    }
    /// `float=top-right` (and top-left, bottom-right, bottom-left; `right-top`
    /// and the like read the same): a picture set in that corner of the
    /// slide, out of the flow. "" for anything else.
    pub fn cornerOf(v0: &str) -> String {
        let v: String = MdChar::lower(&v0.trim().to_string());
        let top: bool = MdText::index_of(&v, "top") >= 0;
        let bottom: bool = MdText::index_of(&v, "bottom") >= 0;
        let left: bool = MdText::index_of(&v, "left") >= 0;
        let right: bool = MdText::index_of(&v, "right") >= 0;
        if top == bottom {
            return String::new();
        }
        if left == right {
            return String::new();
        }
        let mut a: String = "top".to_string();
        if bottom {
            a = "bottom".to_string();
        }
        let mut b: String = "left".to_string();
        if right {
            b = "right".to_string();
        }
        format!("{}-{}", a, b)
    }
    /// the corner block of a slide's body, when its section has one at the
    /// top: the share of the width it takes (its `width=` or 12%), 0 for none
    pub fn topCornerShare(node: &Rc<RefCell<MdNode>>) -> f64 {
        let c: String = MdSlideLayout::cornerOf(&node.borrow().attrValue("float"));
        if MdText::index_of(&c, "top") != 0 {
            return 0.0;
        }
        let v: String = node.borrow().attrValue("width").trim().to_string();
        let n: i64 = MdText::str_len(&v) as i64;
        if n > 1 && MdText::char_at(&v, n - 1) == 37 {
            let d: Option<f64> = MdText::substr(&v, 0, n - 1).trim().to_string().parse::<f64>().ok();
            let f: f64 = (if d.is_some() { d.unwrap() } else { 12.0 }) / 100.0;
            if f > 0.0 && f < 0.5 {
                return f;
            }
        }
        0.12
    }
    /// A statement whose title has text under it hides the title (the text
    /// is the slide); run on the parsed document (MdBlock.run), so the
    /// slide is named by its hidden title everywhere.
    pub fn hideStatementTitles(doc: &Rc<RefCell<MdNode>>) {
        let n: i64 = doc.borrow().childCount();
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
            if c.borrow().kind == "heading" && MdSlideLayout::wordOf(&c, "") == "statement" {
                if i + 1 < n && !c.borrow().hasAttr("heading") {
                    let mut next: Rc<RefCell<MdNode>> = doc.borrow().childAt(i + 1);
                    let opens: bool = next.borrow()
                        .kind == "heading" && next.borrow()
                        .level <= c.borrow()
                        .level;
                    if !opens && next.borrow().kind != "front_matter" {
                        c.borrow_mut().setAttr("heading", "hidden");
                    }
                }
            }
            i += 1;
        }
    }
}
