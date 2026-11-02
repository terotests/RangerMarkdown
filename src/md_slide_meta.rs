// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_node::MdNode;
use crate::rgr::MdText;

/// ============================================================================
/// MdSlideMeta.rgr — a slide's own data in its heading: `{jira=ABC-123}`
/// ============================================================================
///
/// ````markdown
/// ## Revenue {jira=ACME-412 owner=Tero status=review}
/// ## Costs {meta-width=wide}
/// ````
///
/// `MdAttrs` already reads `key=value` off a heading; what was missing is a
/// rule for which keys MEAN something to the engine and which are the
/// document's own business. This file is that rule, and it is the only place
/// the engine's key list is written down:
///
///   a key the engine knows (`transition`, `bg`, `fx-density`, …)  ──► engine
///   anything else (`jira`, `owner`, `epic`, `status`, …)          ──► metadata
///   `meta-<key>`                                                 ──► metadata
///
/// The prefix is not needed for `jira`, and is the way to carry a key the
/// engine would otherwise claim — `{meta-width=wide}` is data, `{width=wide}`
/// is a layout. Keys are lowercased; the value is the text as written.
///
/// Metadata is drawn NOWHERE by itself. It travels with the slide, it can be
/// read back (an assistant asking which slide carries a ticket), and it fills
/// a `{jira}` placeholder in a header or a footer the same way `{page}` does.
///
/// A misspelt engine key would otherwise become metadata in silence —
/// `{transtion=fade}` is a key the engine does not know, and no slide would
/// fade. So a metadata key one character away from an engine key is named in
/// `warnings`, and still kept: the report says what happened rather than the
/// slide quietly losing its transition.
/// ============================================================================
#[derive(Clone)]
pub struct MdSlideMeta {
    pub keys: Vec<String>,
    pub values: Vec<String>,
    /// metadata keys that are one character from an engine key, as
    /// "transtion~transition" — the report's business, not the layout's
    pub suspect: Vec<String>,
}

impl MdSlideMeta {
    pub fn new() -> Self {
        let mut me = Self {
            keys: Vec::new(),
            values: Vec::new(),
            suspect: Vec::new(),
        };
        let mut k: Vec<String> = Vec::new();
        me.keys = k.clone();
        let mut v_2: Vec<String> = Vec::new();
        me.values = v_2.clone();
        let mut s_2: Vec<String> = Vec::new();
        me.suspect = s_2.clone();
        me
    }
    /// ----------------------------------------------------------------------
    /// The engine's keys
    /// ----------------------------------------------------------------------
    ///
    /// Everything a heading's attribute block means to the layout, the deck or
    /// a figure. Longhand on purpose: a list is readable and a new key added
    /// to the engine without a line here shows up as metadata in the report,
    /// which is a louder failure than a silent one.
    pub fn engineKeys() -> Vec<String> {
        vec!["align".to_string(), "allow".to_string(), "anim".to_string(), "art".to_string(), "art-seed".to_string(), "background".to_string(), "ball".to_string(), "bg".to_string(), "bg-dim".to_string(), "caption".to_string(), "cells".to_string(), "chart-style".to_string(), "choose".to_string(), "colors".to_string(), "container".to_string(), "diagram".to_string(), "duration".to_string(), "export-frame".to_string(), "fit".to_string(), "float".to_string(), "focus".to_string(), "footer".to_string(), "furniture".to_string(), "fx".to_string(), "header".to_string(), "heading".to_string(), "height".to_string(), "hold".to_string(), "layout".to_string(), "lines".to_string(), "list-style".to_string(), "numbers".to_string(), "padding".to_string(), "radius".to_string(), "script".to_string(), "seconds".to_string(), "span".to_string(), "step".to_string(), "style".to_string(), "tour".to_string(), "transition".to_string(), "valign".to_string(), "width".to_string(), "widths".to_string(), "zoom".to_string()].clone()
    }
    pub fn isEngineKey(k0: &str) -> bool {
        let k: String = MdChar::lower(&k0.trim().to_string());
        // `fx-density`, `fx-speed`, … all belong to the effect named by `fx`
        if (MdSlideMeta::startsWith(&k, "fx-")) {
            return true;
        }
        let mut all: Vec<String> = MdSlideMeta::engineKeys();
        let mut i: i64 = 0;
        while i < (all.len() as i64) {
            if all[i as usize].clone() == k {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn startsWith(text: &str, what: &str) -> bool {
        let n: i64 = MdText::str_len(&what) as i64;
        if (MdText::str_len(&text) as i64) < n {
            return false;
        }
        MdText::substr(&text, 0, n) == what
    }
    /// The engine key this is one character away from, or "".
    pub fn nearEngineKey(k0: &str) -> String {
        let k: String = MdChar::lower(&k0.trim().to_string());
        MdSlideMeta::nearIn(&k, &MdSlideMeta::engineKeys())
    }
    /// One substitution, insertion, deletion or swap of two neighbours apart —
    /// the four ways a key is mistyped (`tilte`). Not a general edit
    /// distance: two differences are two words, and `owner` is not a misspelt
    /// `order`.
    pub fn oneApart(a: &str, b: &str) -> bool {
        let la: i64 = MdText::str_len(&a) as i64;
        let lb: i64 = MdText::str_len(&b) as i64;
        if a == b {
            return false;
        }
        if la == lb {
            let mut diff: i64 = 0;
            let mut first: i64 = -1;
            let mut i: i64 = 0;
            while i < la {
                if MdText::char_at(&a, i) != MdText::char_at(&b, i) {
                    if diff == 0 {
                        first = i;
                    }
                    diff += 1;
                }
                i += 1;
            }
            if diff == 1 {
                return true;
            }
            if diff == 2 && first + 1 < la {
                return MdText::char_at(&a, first) == MdText::char_at(&b, first + 1) && MdText::char_at(&a, first + 1) == MdText::char_at(&b, first);
            }
            return false;
        }
        if la == lb + 1 {
            return MdSlideMeta::oneLonger(a, b);
        }
        if lb == la + 1 {
            return MdSlideMeta::oneLonger(b, a);
        }
        false
    }
    /// `long` is `short` with one character inserted.
    pub fn oneLonger(long: &str, short: &str) -> bool {
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&short) as i64;
        while i < n {
            if MdText::char_at(&long, i) != MdText::char_at(&short, i) {
                break;
            }
            i += 1;
        }
        let mut j: i64 = i;
        while j < n {
            if MdText::char_at(&long, j + 1) != MdText::char_at(&short, j) {
                return false;
            }
            j += 1;
        }
        true
    }
    /// ----------------------------------------------------------------------
    /// The deck's own data: the front matter
    /// ----------------------------------------------------------------------
    ///
    /// ````yaml
    /// ---
    /// title: Q3 review
    /// lang: fi
    /// jira: ACME-400
    /// owner: Tero
    /// ---
    /// ````
    ///
    /// The same rule one level up: a front matter key the engine reads is the
    /// engine's (`title`, `lang`, `theme`, the header's and footer's, …),
    /// anything else is the deck's own data, and `meta-<key>` is data whatever
    /// it is called. A slide's own value goes over the deck's: `{jira}` in a
    /// footer prints the slide's ticket where it has one and the deck's where
    /// it has none.
    pub fn frontMatterKeys() -> Vec<String> {
        vec!["art".to_string(), "art-seed".to_string(), "book-start".to_string(), "breakonh1".to_string(), "chart-style".to_string(), "diagram-style".to_string(), "footer".to_string(), "fx".to_string(), "header".to_string(), "highlight".to_string(), "hold".to_string(), "lang".to_string(), "margin".to_string(), "margin-bottom".to_string(), "margin-inside".to_string(), "margin-outside".to_string(), "margin-top".to_string(), "mode".to_string(), "orientation".to_string(), "page".to_string(), "pagenumbers".to_string(), "render".to_string(), "runninghead".to_string(), "seconds".to_string(), "select-text".to_string(), "slide-grow".to_string(), "slide-overflow".to_string(), "slide-split-level".to_string(), "step".to_string(), "style".to_string(), "theme".to_string(), "title".to_string(), "toc".to_string(), "transition".to_string(), "typographer".to_string(), "valign".to_string()].clone()
    }
    pub fn isFrontMatterKey(k0: &str) -> bool {
        let k: String = MdChar::lower(&k0.trim().to_string());
        if (MdSlideMeta::startsWith(&k, "fx-")) {
            return true;
        }
        // every `header-…` and `footer-…` belongs to the edges (MdLayout)
        if (MdSlideMeta::startsWith(&k, "header-")) {
            return true;
        }
        if (MdSlideMeta::startsWith(&k, "footer-")) {
            return true;
        }
        let mut all: Vec<String> = MdSlideMeta::frontMatterKeys();
        let mut i: i64 = 0;
        while i < (all.len() as i64) {
            if all[i as usize].clone() == k {
                return true;
            }
            i += 1;
        }
        false
    }
    /// Whether a front matter key, as written, is the deck's data.
    pub fn isDeckDataKey(k0: &str) -> bool {
        let k: String = MdChar::lower(&k0.trim().to_string());
        if k.is_empty() {
            return false;
        }
        if MdSlideMeta::startsWith(&k, "meta-") {
            return (MdText::str_len(&k) as i64) > 5;
        }
        !MdSlideMeta::isFrontMatterKey(&k)
    }
    /// The deck's data from its front matter's keys and values.
    pub fn ofFrontMatter(keys: &[String], values: &[String]) -> Rc<RefCell<MdSlideMeta>> {
        let mut out: Rc<RefCell<MdSlideMeta>> = Rc::new(RefCell::new(MdSlideMeta::new()));
        let mut i: i64 = 0;
        while i < (keys.len() as i64) {
            let key: String = MdChar::lower(&keys[i as usize].clone().trim().to_string());
            if MdSlideMeta::isDeckDataKey(&key) {
                if MdSlideMeta::startsWith(&key, "meta-") {
                    out.borrow_mut().put(&MdText::substr(&key, 5, (MdText::str_len(&key) as i64)), &values[i as usize].clone());
                } else {
                    out.borrow_mut().put(&key, &values[i as usize].clone());
                    let near: String = MdSlideMeta::nearIn(&key, &MdSlideMeta::frontMatterKeys());
                    if !near.is_empty() {
                        out.borrow_mut().suspect.push(format!("{}~{}", key, near));
                    }
                }
            }
            i += 1;
        }
        out
    }
    /// The key in `all` this one is one character away from, or "".
    pub fn nearIn(k: &str, all: &[String]) -> String {
        if (MdText::str_len(&k) as i64) < 3 {
            return String::new();
        }
        let mut i: i64 = 0;
        while i < (all.len() as i64) {
            let cand: String = all[i as usize].clone();
            if MdSlideMeta::oneApart(k, &cand) {
                return cand;
            }
            i += 1;
        }
        String::new()
    }
    /// ----------------------------------------------------------------------
    /// Reading one off a heading
    /// ----------------------------------------------------------------------
    pub fn of(node: &Rc<RefCell<MdNode>>) -> Rc<RefCell<MdSlideMeta>> {
        let mut out: Rc<RefCell<MdSlideMeta>> = Rc::new(RefCell::new(MdSlideMeta::new()));
        let n: i64 = node.borrow().attrCount();
        let mut i: i64 = 0;
        while i < n {
            let key: String = MdChar::lower(&node.borrow().attrKeys[i as usize].clone().trim().to_string());
            let val: String = node.borrow().attrVals[i as usize].clone();
            if MdSlideMeta::startsWith(&key, "meta-") {
                out.borrow_mut().put(&MdText::substr(&key, 5, (MdText::str_len(&key) as i64)), &val);
            } else {
                if !MdSlideMeta::isEngineKey(&key) {
                    out.borrow_mut().put(&key, &val);
                    let near: String = MdSlideMeta::nearEngineKey(&key);
                    if !near.is_empty() {
                        out.borrow_mut().suspect.push(format!("{}~{}", key, near));
                    }
                }
            }
            i += 1;
        }
        out
    }
    pub fn count(&self) -> i64 {
        self.keys.len() as i64
    }
    pub fn has(&self, k: &str) -> bool {
        let want: String = MdChar::lower(&k.trim().to_string());
        let mut i: i64 = 0;
        while i < (self.keys.len() as i64) {
            if self.keys[i as usize].clone() == want {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn value(&self, k: &str) -> String {
        let want: String = MdChar::lower(&k.trim().to_string());
        let mut i: i64 = 0;
        while i < (self.keys.len() as i64) {
            if self.keys[i as usize].clone() == want {
                return self.values[i as usize].clone();
            }
            i += 1;
        }
        String::new()
    }
    /// Last one wins, as an attribute block does.
    pub fn put(&mut self, k: &str, v: &str) {
        let key: String = MdChar::lower(&k.trim().to_string());
        if key.is_empty() {
            return;
        }
        let mut i: i64 = 0;
        while i < (self.keys.len() as i64) {
            if self.keys[i as usize].clone() == key {
                self.values[i as usize] = v.to_string();
                return;
            }
            i += 1;
        }
        self.keys.push(key);
        self.values.push(v.to_string());
    }
    /// `jira=ACME-412 owner=Tero`, in the order they were written.
    pub fn asText(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut i: i64 = 0;
        while i < (self.keys.len() as i64) {
            parts.push(format!("{}={}", self.keys[i as usize].clone(), self.values[i as usize].clone()));
            i += 1;
        }
        parts.join(&*" ".to_string())
    }
}
