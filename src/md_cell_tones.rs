// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;

use crate::md_container::MdContainer;
use crate::md_node::MdNode;
use crate::rgr::MdText;
use crate::rgr::{EVGColor};

/// ============================================================================
/// MdCellTones.rgr — a table's cells coloured by what they say.
/// ============================================================================
///
/// ````markdown
/// | Riski          | Todennäköisyys | Vaikutus   |
/// |----------------|----------------|------------|
/// | Keskittyminen  | Keskisuuri     | Suuri      |
/// | Avainasiakkaat | Matala         | Suuri      |
/// {cells="Suuri=red Korkea=red Keskisuuri=amber Matala=green"}
/// ````
///
/// A risk table, a status table, a traffic light: the same few words in many
/// cells, and each word has a colour. The mapping is said ONCE, under the
/// table, rather than on every cell — markdown has no syntax for a cell's
/// attributes, and the words are what the colour is about anyway.
///
/// THE MAPPING. `text=tone` pairs, separated by spaces, commas or semicolons.
/// The text is matched against the WHOLE of a cell's text, trimmed and
/// case-blind (`suuri` = `Suuri` = ` SUURI `); a text with spaces in it is
/// quoted, `'Very high'=red`. A tone is `red`, `amber` (`yellow`, `orange`),
/// `green`, `blue`, `grey` (`gray`) — or any colour CSS can name, `#8e44ad`.
/// The same text said twice: the later one wins. The header row is never
/// coloured: it says what the column is, not a value in it.
///
/// A theme can say it for every table: `table { cell-tones: "Done=green" }`.
/// A table's own `{cells=…}` is read after it, so it adds to that and wins
/// where both name the same text.
///
/// THE COLOURS are not the tone's own: a pure red cell on a dark slide is a
/// warning light, on a white page a stop sign. The FILL is the page's colour
/// with a little of the tone in it (a muted tint, `fillShare`), so a table
/// reads as one thing on either; the TEXT is the tone moved towards white on
/// a dark page and towards black on a light one until it stands 4.5 : 1 off
/// the fill (WCAG AA), and is set bold. Both are worked out from the page's
/// colour, so one deck reads the same in a light theme and a dark one.
///
/// Nothing here measures or draws: the slide layout, the PPTX, the DOCX and the
/// HTML each ask `forTable` once per table and `toneOf` per cell.
/// ============================================================================
#[derive(Clone)]
pub struct MdCellTones {
    /// matched text (trimmed, lower case) and its tone, in the order said
    pub keys: Vec<String>,
    pub tones: Vec<String>,
}

impl MdCellTones {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            tones: Vec::new(),
        }
    }
    /// how much of the tone is in a cell's fill
    pub fn fillShare() -> f64 {
        0.22
    }
    /// the contrast the text is held to, against its fill
    pub fn needContrast() -> f64 {
        4.5
    }
    /// ----------------------------------------------------------------------
    /// Reading a mapping
    /// ----------------------------------------------------------------------
    pub fn parse(spec: &str) -> Rc<RefCell<MdCellTones>> {
        let mut t: Rc<RefCell<MdCellTones>> = Rc::new(RefCell::new(MdCellTones::new()));
        t.borrow_mut().addSpec(spec);
        t
    }
    /// The mapping for one table: the theme's (`table { cell-tones }`), then
    /// the table's own `{cells=…}` over it.
    pub fn forTable(node: &Rc<RefCell<MdNode>>, theme_spec: &str) -> Rc<RefCell<MdCellTones>> {
        let mut t: Rc<RefCell<MdCellTones>> = MdCellTones::parse(theme_spec);
        t.borrow_mut().addSpec(&node.borrow().attrValue("cells"));
        t
    }
    pub fn keyOf(text: &str) -> String {
        text.trim().to_string().to_lowercase()
    }
    pub fn isGap(c: i64) -> bool {
        if c == 32 || c == 9 {
            return true;
        }
        if c == 10 || c == 13 {
            return true;
        }
        if c == 44 || c == 59 {
            return true;
        }
        false
    }
    /// a CSS string value comes with its quotes on
    pub fn unquote(s: &str) -> String {
        let n: i64 = MdText::str_len(&s) as i64;
        if n < 2 {
            return s.to_string();
        }
        let q: i64 = MdText::char_at(&s, 0);
        if (q == 34 || q == 39) && MdText::char_at(&s, n - 1) == q {
            // …unless the quotes are a quoted TEXT's: `'a b'=red` ends in d
            return MdText::substr(&s, 1, n - 1);
        }
        s.to_string()
    }
    /// ----------------------------------------------------------------------
    /// The colours
    /// ----------------------------------------------------------------------
    /// The tone's own colour, `#RRGGBB`, or "" when it names none.
    pub fn hueOf(tone: &str) -> String {
        let t: String = tone.trim().to_string().to_lowercase();
        if t == "red" {
            return "#D64545".to_string();
        }
        if t == "amber" || (t == "yellow" || t == "orange") {
            return "#D99A1E".to_string();
        }
        if t == "green" {
            return "#2F9E66".to_string();
        }
        if t == "blue" {
            return "#3B7DD8".to_string();
        }
        if t == "grey" || t == "gray" {
            return "#8A8F98".to_string();
        }
        if t.is_empty() {
            return String::new();
        }
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(&t);
        if false == c.borrow().is_set {
            return String::new();
        }
        return EVGColor::to_hex_string(&c);
    }
    /// The colour cells stand on: the page's own when it has one, else the
    /// page the text colour was chosen for (light text, a dark page).
    pub fn paperOf(page_background: &str, text_color: &str) -> String {
        if !page_background.is_empty() {
            let mut pc: Rc<RefCell<EVGColor>> = EVGColor::parse(page_background);
            if pc.borrow().is_set && EVGColor::alpha(&pc) > 0.5 {
                return EVGColor::to_hex_string(&EVGColor::rgb(
                    EVGColor::red(&pc),
                    EVGColor::green(&pc),
                    EVGColor::blue(&pc)
                )).clone();
            }
        }
        if MdContainer::luminance(text_color) > 0.4 {
            return "#1B1F24".to_string();
        }
        "#FFFFFF".to_string()
    }
    pub fn isDark(paper: &str) -> bool {
        MdContainer::luminance(paper) < 0.18
    }
    /// A cell's fill: the paper with a little of the tone in it.
    pub fn fillOf(tone: &str, paper: &str) -> String {
        let hue: String = MdCellTones::hueOf(tone);
        if hue.is_empty() {
            return String::new();
        }
        MdCellTones::mix(paper, &hue, MdCellTones::fillShare())
    }
    /// A cell's text: the tone, lighter on a dark page and darker on a light
    /// one, until it reads on the fill.
    pub fn inkOf(tone: &str, paper: &str) -> String {
        let hue: String = MdCellTones::hueOf(tone);
        if hue.is_empty() {
            return String::new();
        }
        let fill: String = MdCellTones::fillOf(tone, paper);
        let mut pole: String = "#000000".to_string();
        let mut t: f64 = 0.3;
        if MdCellTones::isDark(&fill) {
            pole = "#FFFFFF".to_string();
            t = 0.45;
        }
        while t < 1.0 {
            let ink: String = MdCellTones::mix(&hue, &pole, t);
            if MdContainer::contrast(&ink, &fill) >= MdCellTones::needContrast() {
                return ink;
            }
            t += 0.05;
        }
        pole.to_string()
    }
    /// `a` taken `t` of the way to `b`, `#RRGGBB`
    pub fn mix(a: &str, b: &str, t: f64) -> String {
        let mut ca: Rc<RefCell<EVGColor>> = EVGColor::parse(a);
        let mut cb: Rc<RefCell<EVGColor>> = EVGColor::parse(b);
        if false == ca.borrow().is_set {
            return b.to_string();
        }
        if false == cb.borrow().is_set {
            return a.to_string();
        }
        let r: i64 = MdCellTones::lerp(EVGColor::red(&ca), EVGColor::red(&cb), t);
        let g: i64 = MdCellTones::lerp(EVGColor::green(&ca), EVGColor::green(&cb), t);
        let bl: i64 = MdCellTones::lerp(EVGColor::blue(&ca), EVGColor::blue(&cb), t);
        EVGColor::to_hex_string(&EVGColor::rgb(
            r,
            g,
            bl
        )).clone()
    }
    pub fn lerp(a: i64, b: i64, t: f64) -> i64 {
        let v: f64 = (a as f64) + ((b as f64) - (a as f64)) * t;
        (v + 0.5).floor() as i64
    }
    pub fn isEmpty(&self) -> bool {
        self.keys.is_empty()
    }
    pub fn count(&self) -> i64 {
        self.keys.len() as i64
    }
    /// `a=red b=green`, added to what is here; a pair that is not one (no
    /// `=`, an empty side, a tone that is no colour) is skipped.
    pub fn addSpec(&mut self, spec: &str) {
        let s: String = MdCellTones::unquote(&spec.trim().to_string());
        let n: i64 = MdText::str_len(&s) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            if MdCellTones::isGap(c) {
                i += 1;
                continue;
            }
            // the text: quoted, or up to the `=`
            let mut key: String = String::new();
            if c == 39 || c == 34 {
                let mut e: i64 = i + 1;
                while e < n && MdText::char_at(&s, e) != c {
                    e += 1;
                }
                key = MdText::substr(&s, i + 1, e);
                i = e + 1;
            } else {
                let mut e2: i64 = i;
                while e2 < n {
                    let c2: i64 = MdText::char_at(&s, e2);
                    if c2 == 61 {
                        break;
                    }
                    if MdCellTones::isGap(c2) {
                        break;
                    }
                    e2 += 1;
                }
                key = MdText::substr(&s, i, e2);
                i = e2;
            }
            // `=` and the tone, up to the next gap
            if i >= n || MdText::char_at(&s, i) != 61 {
                // not a pair: skip to the next gap
                while i < n && false == MdCellTones::isGap(MdText::char_at(&s, i)) {
                    i += 1;
                }
                continue;
            }
            i += 1;
            let t0: i64 = i;
            while i < n && false == MdCellTones::isGap(MdText::char_at(&s, i)) {
                i += 1;
            }
            let tone: String = MdText::substr(&s, t0, i).to_lowercase();
            self.add(&key, &tone);
        }
    }
    /// One pair. A text said again takes the new tone.
    pub fn add(&mut self, text: &str, tone: &str) {
        let k: String = MdCellTones::keyOf(text);
        if k.is_empty() {
            return;
        }
        if (MdText::str_len(&MdCellTones::hueOf(tone)) as i64) == 0 {
            return;
        }
        let at: i64 = self.keys.iter()
            .position( |__r| __r.clone() == k.clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        if at >= 0 {
            self.tones[at as usize] = tone.to_string();
            return;
        }
        self.keys.push(k);
        self.tones.push(tone.to_string());
    }
    /// The tone of a cell that says `text`, or "" for none.
    pub fn toneOf(&self, text: &str) -> String {
        if self.keys.is_empty() {
            return String::new();
        }
        let at: i64 = self.keys.iter()
            .position( |__r| __r.clone() == (MdCellTones::keyOf(text)).clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        if at < 0 {
            return String::new();
        }
        self.tones[at as usize].clone()
    }
    /// …of a table cell, by its text. A header cell has none.
    pub fn toneOfCell(&self, cell: &Rc<RefCell<MdNode>>) -> String {
        if cell.borrow().isHeader {
            return String::new();
        }
        return self.toneOf(&cell.borrow().plainText());
    }
}
