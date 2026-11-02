// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_front_matter::MdFrontMatter;
use crate::md_front_matter::MdMeta;

/// ============================================================================
/// MdBook.rgr — a deck that is a printed book: its pages, sides and spreads.
/// ============================================================================
///
///   ---
///   mode: book            # slides (the default) | book
///   render: realistic     # flat (the default) | realistic: how a viewer shows it
///   book-start: right     # right (the default): page 1 is the cover, alone on
///                         # the right; left: page 1 and 2 face each other
///   margin: 15mm          # every edge
///   margin-top: 20mm      # one edge (top, bottom, inside, outside)
///   margin-inside: 22mm   # the edge at the binding: right of a left page,
///   margin-outside: 12mm  # left of a right one; outside is the other edge
///   ---
///
/// A book is pages that face each other. Page 1 of a book that starts on the
/// right is a recto (a right-hand page) with nothing to its left — the cover —
/// and from there every even page is a verso (left) facing the odd one after
/// it. A spread is the pair, either half of which may be missing (the cover,
/// and the back page of a book with an even count).
///
/// Everything here is a number of pages and a margin: no layout, no drawing.
/// The layout reads the margins (MdLayout.applyBookMargins); a player reads
/// the spreads to show two pages at once.
/// ============================================================================
#[derive(Clone)]
pub struct MdBook {
    pub mode: String,
    pub render: String,
    pub start: String,
    /// in points; -1 is "not given"
    pub margin: f64,
    pub marginTop: f64,
    pub marginBottom: f64,
    pub marginInside: f64,
    pub marginOutside: f64,
}

impl MdBook {
    pub fn new() -> Self {
        Self {
            mode: "slides".to_string(),
            render: "flat".to_string(),
            start: "right".to_string(),
            margin: -1.0,
            marginTop: -1.0,
            marginBottom: -1.0,
            marginInside: -1.0,
            marginOutside: -1.0,
        }
    }
    /// The front matter's book, or a deck of slides when it says nothing.
    /// Words the model does not know keep the default, so a typo shows slides
    /// rather than half a book.
    pub fn of(meta: &Rc<RefCell<MdMeta>>) -> Rc<RefCell<MdBook>> {
        let mut b: Rc<RefCell<MdBook>> = Rc::new(RefCell::new(MdBook::new()));
        let m: String = MdBook::word(&meta.borrow().get("mode", ""));
        if m == "book" {
            b.borrow_mut().mode = "book".to_string();
        }
        let r: String = MdBook::word(&meta.borrow().get("render", ""));
        if r == "realistic" {
            b.borrow_mut().render = "realistic".to_string();
        }
        let s: String = MdBook::word(&meta.borrow().get("book-start", ""));
        if s == "left" {
            b.borrow_mut().start = "left".to_string();
        }
        b.borrow_mut().margin = MdBook::length(meta, "margin");
        b.borrow_mut().marginTop = MdBook::length(meta, "margin-top");
        b.borrow_mut().marginBottom = MdBook::length(meta, "margin-bottom");
        b.borrow_mut().marginInside = MdBook::length(meta, "margin-inside");
        b.borrow_mut().marginOutside = MdBook::length(meta, "margin-outside");
        b
    }
    pub fn word(v: &str) -> String {
        MdChar::lower(&v.trim().to_string())
    }
    /// A length the front matter gave, in points, or -1. `0` is a length: a
    /// page with no margin at the top is something a photo book asks for.
    pub fn length(meta: &Rc<RefCell<MdMeta>>, key: &str) -> f64 {
        if !meta.borrow().has(key) {
            return -1.0;
        }
        let v: String = MdChar::lower(&meta.borrow().get(key, "").trim().to_string());
        if (((v == "0" || v == "0mm") || v == "0pt") || v == "0cm") || v == "0in" {
            return 0.0;
        }
        MdFrontMatter::points(&v, -1.0)
    }
    pub fn isBook(&self) -> bool {
        self.mode == "book"
    }
    pub fn isRealistic(&self) -> bool {
        self.mode == "book" && self.render == "realistic"
    }
    pub fn anyMargin(&self) -> bool {
        if self.margin >= 0.0 {
            return true;
        }
        if self.marginTop >= 0.0 {
            return true;
        }
        if self.marginBottom >= 0.0 {
            return true;
        }
        if self.marginInside >= 0.0 {
            return true;
        }
        self.marginOutside >= 0.0
    }
    /// ----------------------------------------------------------------------
    /// Sides and spreads (page indexes from 0)
    /// ----------------------------------------------------------------------
    /// A right-hand page. A deck of slides has no sides; its pages are all
    /// "right" so a caller that mirrors nothing for them needs no second test.
    pub fn isRecto(&self, page: i64) -> bool {
        if !(self.mode == "book") {
            return true;
        }
        if self.start == "left" {
            return page % 2 == 1;
        }
        page % 2 == 0
    }
    /// How many spreads `pages` pages make. A deck of slides is one page a
    /// spread.
    pub fn spreadCount(&self, pages: i64) -> i64 {
        if pages <= 0 {
            return 0;
        }
        if !(self.mode == "book") {
            return pages;
        }
        if self.start == "left" {
            return (pages + 1) / 2;
        }
        (pages / 2) + 1
    }
    pub fn spreadOf(&self, page: i64) -> i64 {
        if page < 0 {
            return 0;
        }
        if !(self.mode == "book") {
            return page;
        }
        if self.start == "left" {
            return page / 2;
        }
        (page + 1) / 2
    }
    /// The page on the left of spread `s`, or -1 (the cover's spread, or one
    /// past the end).
    pub fn leftOf(&self, s: i64, pages: i64) -> i64 {
        if !(self.mode == "book") {
            return -1;
        }
        let mut p: i64 = s * 2;
        if self.start == "right" {
            p -= 1;
        }
        if p < 0 || p >= pages {
            return -1;
        }
        p
    }
    /// The page on the right of spread `s`, or -1. A deck of slides shows its
    /// one page here.
    pub fn rightOf(&self, s: i64, pages: i64) -> i64 {
        let mut p: i64 = s;
        if self.mode == "book" {
            p = s * 2;
            if self.start == "left" {
                p += 1;
            }
        }
        if p < 0 || p >= pages {
            return -1;
        }
        p
    }
    /// The first page of a spread: where going to the spread lands.
    pub fn firstOf(&self, s: i64, pages: i64) -> i64 {
        let l: i64 = self.leftOf(s, pages);
        if l >= 0 {
            return l;
        }
        self.rightOf(s, pages)
    }
    /// ----------------------------------------------------------------------
    /// Margins
    /// ----------------------------------------------------------------------
    ///
    /// Each edge: its own length, else `margin`, else the page's (the theme's
    /// `page { padding }`). Never more than 40% of the page it is on.
    pub fn edge(&self, own: f64, fallback: f64, room: f64) -> f64 {
        let mut v: f64 = fallback;
        if self.margin >= 0.0 {
            v = self.margin;
        }
        if own >= 0.0 {
            v = own;
        }
        let most: f64 = room * 0.4;
        if v > most {
            v = most;
        }
        if v < 0.0 {
            v = 0.0;
        }
        v
    }
    pub fn top(&self, fallback: f64, page_h: f64) -> f64 {
        self.edge(self.marginTop.clone(), fallback, page_h)
    }
    pub fn bottom(&self, fallback: f64, page_h: f64) -> f64 {
        self.edge(self.marginBottom.clone(), fallback, page_h)
    }
    pub fn inside(&self, fallback: f64, page_w: f64) -> f64 {
        self.edge(self.marginInside.clone(), fallback, page_w)
    }
    pub fn outside(&self, fallback: f64, page_w: f64) -> f64 {
        self.edge(self.marginOutside.clone(), fallback, page_w)
    }
    /// The left margin of page `page`: the outside of a left-hand page, the
    /// inside of a right-hand one. A deck of slides reads inside as left.
    pub fn leftMargin(&self, page: i64, fallback: f64, page_w: f64) -> f64 {
        if self.mode != "book" {
            return self.inside(fallback, page_w);
        }
        if self.isRecto(page) {
            return self.inside(fallback, page_w);
        }
        self.outside(fallback, page_w)
    }
    pub fn rightMargin(&self, page: i64, fallback: f64, page_w: f64) -> f64 {
        if self.mode != "book" {
            return self.outside(fallback, page_w);
        }
        if self.isRecto(page) {
            return self.outside(fallback, page_w);
        }
        self.inside(fallback, page_w)
    }
    /// One line for a cache key: two books that differ here lay out apart.
    pub fn fingerprint(&self) -> String {
        format!("{}/{}/{}/{}/{}/{}/{}/{}", self.mode, self.render, self.start, self.margin, self.marginTop, self.marginBottom, self.marginInside, self.marginOutside)
    }
}
