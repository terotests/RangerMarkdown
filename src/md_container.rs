// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;

use crate::md_node::MdNode;
use crate::rgr::MdText;
use crate::rgr::{EVGColor};

/// ============================================================================
/// MdContainer.rgr — a block set on a rounded plate of its own colour.
/// ============================================================================
///
/// ````markdown
/// Text over a busy picture, readable again.
/// {container=bubble}
///
/// The same on a plate of a colour of its own.
/// {container=box background=#fff8}
/// ````
///
/// A markdown-it container (`::: warning` … `:::`) is a `box` too, and takes
/// `{container=bubble}` / `{container=none}` after its closing line like any
/// block. Its name is its class, so a sheet's `.warning { background-color }`
/// is its plate's colour (`MdLayout.containerBlock`).
///
/// `box` is a rounded rectangle across the column. `bubble` is rounder, as wide
/// as its text, and has a tail at the bottom left — a speech bubble. Any other
/// word is a box; `none` / `off` is no plate.
///
/// THE COLOUR. `background=` (or `bg=` when what follows is a colour, not a
/// picture) wins; else the theme's `container { background-color }`; else the
/// page's own colour at 85 % — the colour the theme's text was chosen against,
/// so it reads. A page with no colour of its own (a picture only) gets white or
/// black, whichever the text is written to stand on.
///
/// THE INK. Whatever shows through a see-through plate is not known here — it
/// is a picture. So the contrast is taken at its WORST: the plate laid over
/// black and over white, and the lower of the two. When the text's own colour
/// does not reach 4.5:1 on that, the block is set in near-black or white,
/// whichever reads better. A plate that is opaque is exactly its colour.
///
/// Nothing here measures or draws: `MdLayout.containerBlock` asks `plan` and
/// places the boxes. That is what lets this be tested without a layout.
/// ============================================================================
#[derive(Clone)]
pub struct MdContainer {
    /// "box" / "bubble", "" for none
    pub kind: String,
    /// the plate, `#RRGGBB` or `#RRGGBBAA`
    pub fill: String,
    /// the text colour inside, or "" for the block's own
    pub ink: String,
    pub radius: f64,
    pub padX: f64,
    /// over the text (`padding-top`) and under it (`padding-bottom`)
    pub padY: f64,
    pub padBottom: f64,
    /// a bubble's tail: how far it hangs below the plate, 0 for none
    pub tail: f64,
    /// …and whether the plate hugs its text rather than filling the column
    pub hug: bool,
    /// the colour of a soft shadow under the plate, "" for none
    pub shadow: String,
    /// a line round the plate (`border: 2pt solid #7ee0ff`), "" for none
    pub border: String,
    pub borderWidth: f64,
}

impl MdContainer {
    pub fn new() -> Self {
        Self {
            kind: String::new(),
            fill: String::new(),
            ink: String::new(),
            radius: 0.0,
            padX: 0.0,
            padY: 0.0,
            padBottom: 0.0,
            tail: 0.0,
            hug: false,
            shadow: String::new(),
            border: String::new(),
            borderWidth: 0.0,
        }
    }
    /// ----------------------------------------------------------------------
    /// What the block asked for
    /// ----------------------------------------------------------------------
    pub fn kindOf(node: &Rc<RefCell<MdNode>>) -> String {
        if false == node.borrow().hasAttr("container") {
            // `::: warning` … `:::` is a box unless it says otherwise;
            // `::: columns` / `::: col` only arrange (MdColumns)
            if node.borrow().kind == "container" {
                let nm: String = MdContainer::lower(&node.borrow().info.clone());
                if ((nm == "columns" || nm == "cols") || nm == "col") || nm == "column" {
                    return String::new();
                }
                return "box".to_string();
            }
            return String::new();
        }
        let v: String = MdContainer::lower(&node.borrow().attrValue("container").trim().to_string());
        if v.is_empty() {
            return String::new();
        }
        if v == "none" || v == "off" {
            return String::new();
        }
        if v == "bubble" {
            return "bubble".to_string();
        }
        "box".to_string()
    }
    /// The colour the block names for its plate, "" when it names none.
    /// `bg=` is also a slide's PICTURE (`{bg=media/kansi.jpg}` on a heading),
    /// so it is a colour here only when it reads as one.
    pub fn fillOf(node: &Rc<RefCell<MdNode>>) -> String {
        if node.borrow().hasAttr("background") {
            let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(&node.borrow().attrValue("background").trim().to_string());
            if c.borrow().is_set {
                return MdContainer::hexOf(&c);
            }
        }
        if node.borrow().hasAttr("bg") {
            let v: String = node.borrow().attrValue("bg").trim().to_string();
            if MdContainer::colourLike(&v) {
                let mut c2: Rc<RefCell<EVGColor>> = EVGColor::parse(&v);
                if c2.borrow().is_set {
                    return MdContainer::hexOf(&c2);
                }
            }
        }
        String::new()
    }
    /// `#…`, `rgb(…)` / `rgba(…)`, or a bare word with no path in it
    pub fn colourLike(v: &str) -> bool {
        let n: i64 = MdText::str_len(&v) as i64;
        if n == 0 {
            return false;
        }
        if MdText::char_at(&v, 0) == 35 {
            return true;
        }
        if MdText::index_of(&v, "rgb") == 0 {
            return true;
        }
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if c == 47 || c == 46 {
                return false;
            }
            i += 1;
        }
        true
    }
    /// ----------------------------------------------------------------------
    /// The plan
    /// ----------------------------------------------------------------------
    ///
    /// `own` the block's colour (`fillOf`), `themeFill` / `themeRadius` the
    /// sheet's `container` rule ("" / negative when it says nothing), `paper`
    /// the page's colour ("" when it has only a picture), `ink` the text colour
    /// the block would be set in, `size` its body size in points.
    pub fn plan(kind: String, own: &str, theme_fill: &str, theme_radius: f64, paper: &str, ink: &str, size: f64) -> Rc<RefCell<MdContainer>> {
        let mut p: Rc<RefCell<MdContainer>> = Rc::new(RefCell::new(MdContainer::new()));
        p.borrow_mut().kind = kind.clone();
        if kind.is_empty() {
            return p;
        }
        p.borrow_mut().padX = size * 0.8;
        p.borrow_mut().padY = size * 0.5;
        if kind == "bubble" {
            p.borrow_mut().radius = size * 1.1;
            p.borrow_mut().padX = size * 1.0;
            p.borrow_mut().tail = size * 0.8;
            p.borrow_mut().hug = true;
        } else {
            p.borrow_mut().radius = size * 0.45;
        }
        if theme_radius >= 0.0 {
            p.borrow_mut().radius = theme_radius;
        }
        let mut fill: String = own.to_string();
        if fill.is_empty() {
            fill = theme_fill.to_string();
        }
        if fill.is_empty() {
            fill = MdContainer::defaultFill(paper, ink);
        }
        p.borrow_mut().fill = fill.clone();
        p.borrow_mut().ink = MdContainer::inkOnPaper(&fill, ink, paper);
        let padBottom_v = p.borrow().padY;
        p.borrow_mut().padBottom = padBottom_v;
        p
    }
    /// `box-shadow: 0 6pt 18pt rgba(46,58,99,0.12)` → the shadow's colour
    /// (`#2e3a63`; its own alpha is left to the layers that draw it), ""
    /// for `none` and for nothing; a shadow with no colour is black
    pub fn shadowColour(v: &str) -> String {
        let t: String = MdContainer::lower(&v.trim().to_string());
        if t.is_empty() || t == "none" {
            return String::new();
        }
        let mut at: i64 = MdText::index_of(&t, "rgb");
        if at < 0 {
            at = MdText::index_of(&t, "#");
        }
        if at >= 0 {
            let rest: String = MdText::substr(&t, at, (MdText::str_len(&t) as i64));
            let mut end: i64 = MdText::str_len(&rest) as i64;
            if MdText::char_at(&rest, 0) == 35 {
                let mut k: i64 = 1;
                while k < (MdText::str_len(&rest) as i64) && MdText::char_at(&rest, k) != 32 {
                    k += 1;
                }
                end = k;
            } else {
                let close: i64 = MdText::index_of(&rest, ")");
                if close >= 0 {
                    end = close + 1;
                }
            }
            let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(&MdText::substr(&rest, 0, end));
            if c.borrow().is_set {
                return MdContainer::hexOf(&EVGColor::rgba(
                    EVGColor::red(&c),
                    EVGColor::green(&c),
                    EVGColor::blue(&c),
                    1.0
                )).clone();
            }
        }
        "#000000".to_string()
    }
    /// --- callout tones -------------------------------------------------------
    /// `::: info` / `::: tip` / `::: warning` / `::: danger`: four named
    /// callouts a theme does not have to know. Their plate is the page's own
    /// colour with a little of the tone's hue in it and a line round it in
    /// the hue, so they are told apart by more than the tint, and the text
    /// on them is the page's text (`inkOnPaper` still guards it). A theme
    /// that styles `.warning { background-color }` wins; any other name is
    /// the neutral plate it always was.
    pub fn tones() -> Vec<String> {
        let mut out: Vec<String> = "info,tip,warning,danger".to_string()
            .split(",")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        out
    }
    pub fn toneOf(name: &str) -> String {
        let nm: String = MdContainer::lower(&name.trim().to_string());
        if nm == "info" {
            return "info".to_string();
        }
        if nm == "tip" {
            return "tip".to_string();
        }
        if nm == "warning" {
            return "warning".to_string();
        }
        if nm == "danger" {
            return "danger".to_string();
        }
        String::new()
    }
    pub fn toneHue(tone: &str) -> String {
        if tone == "info" {
            return "#2F6FDE".to_string();
        }
        if tone == "tip" {
            return "#1F9D55".to_string();
        }
        if tone == "warning" {
            return "#D99A00".to_string();
        }
        if tone == "danger" {
            return "#D1453B".to_string();
        }
        String::new()
    }
    /// The page under the plate, opaque: the paper, or — over a picture or
    /// nothing — white under dark text and near-black under light.
    pub fn toneGround(paper: &str, ink: &str) -> String {
        let mut pc: Rc<RefCell<EVGColor>> = EVGColor::parse(paper);
        if !paper.is_empty() && pc.borrow().is_set {
            if EVGColor::alpha(&pc) > 0.5 {
                return MdContainer::hexOf(&EVGColor::rgb(
                    EVGColor::red(&pc),
                    EVGColor::green(&pc),
                    EVGColor::blue(&pc)
                )).clone();
            }
        }
        if MdContainer::luminance(ink) > 0.4 {
            return "#16191D".to_string();
        }
        "#FFFFFF".to_string()
    }
    /// `a` with `t` (0…1) of `b` in it, opaque
    pub fn mix(a: &str, b: &str, t: f64) -> String {
        let mut ca: Rc<RefCell<EVGColor>> = EVGColor::parse(a);
        let mut cb: Rc<RefCell<EVGColor>> = EVGColor::parse(b);
        let r: i64 = ((((EVGColor::red(&ca)) as f64) * (1.0 - t) + ((EVGColor::red(&cb)) as f64) * t) + 0.5).floor() as i64;
        let g: i64 = ((((EVGColor::green(&ca)) as f64) * (1.0 - t) + ((EVGColor::green(&cb)) as f64) * t) + 0.5).floor() as i64;
        let bl: i64 = ((((EVGColor::blue(&ca)) as f64) * (1.0 - t) + ((EVGColor::blue(&cb)) as f64) * t) + 0.5).floor() as i64;
        MdContainer::hexOf(&EVGColor::rgb(r, g, bl))
    }
    /// The tone's plate: 14 % of the hue on a light page, 24 % on a dark one,
    /// backed off while the page's text would not read on it at 4.5:1.
    pub fn toneFill(tone: &str, paper: &str, ink: &str) -> String {
        let hue: String = MdContainer::toneHue(tone);
        if hue.is_empty() {
            return String::new();
        }
        let ground: String = MdContainer::toneGround(paper, ink);
        let mut t: f64 = if (MdContainer::luminance(&ground) > 0.4) { 0.14 } else { 0.24 };
        let mut fill: String = MdContainer::mix(&ground, &hue, t);
        while t > 0.04 && MdContainer::contrast(ink, &fill) < 4.5 {
            t -= 0.02;
            fill = MdContainer::mix(&ground, &hue, t);
        }
        fill
    }
    /// The line round it: the hue, taken toward the text until it stands out
    /// from the plate at 3:1 (WCAG's non-text contrast).
    pub fn toneBorder(tone: &str, fill: &str, ink: &str) -> String {
        let hue: String = MdContainer::toneHue(tone);
        if hue.is_empty() {
            return String::new();
        }
        let mut t: f64 = 0.0;
        let mut line: String = MdContainer::mix(&hue, ink, t);
        while t < 1.0 && MdContainer::contrast(&line, fill) < 3.0 {
            t += 0.1;
            line = MdContainer::mix(&hue, ink, t);
        }
        line
    }
    pub fn defaultFill(paper: &str, ink: &str) -> String {
        let mut pc: Rc<RefCell<EVGColor>> = EVGColor::parse(paper);
        if !paper.is_empty() {
            if pc.borrow().is_set && EVGColor::alpha(&pc) > 0.5 {
                return MdContainer::hexOf(&EVGColor::rgba(
                    EVGColor::red(&pc),
                    EVGColor::green(&pc),
                    EVGColor::blue(&pc),
                    0.85
                )).clone();
            }
        }
        // no colour of its own: the plate the ink was written to stand on
        if MdContainer::luminance(ink) > 0.4 {
            return "#000000B3".to_string();
        }
        "#FFFFFFD9".to_string()
    }
    /// The text colour that reads on `fill`: `ink` when it does, else the
    /// better of near-black and white.
    pub fn inkOn(fill: &str, ink: &str) -> String {
        if MdContainer::worstContrast(ink, fill) >= 4.5 {
            return String::new();
        }
        let dark: f64 = MdContainer::worstContrast("#111111", fill);
        let light: f64 = MdContainer::worstContrast("#FFFFFF", fill);
        if dark >= light {
            return "#111111".to_string();
        }
        "#FFFFFF".to_string()
    }
    /// …and when no ink reads on the plate whatever is behind it (a faint
    /// tint such as `#00000010`, see-through to anything), the page's own
    /// colour is what is behind it: judged on the plate laid over the paper.
    /// Taken at its worst, near-black and white both came out at 1.1:1 and
    /// white won the tie, so a grey card on a white slide had white text.
    pub fn inkOnPaper(fill: &str, ink: &str, paper: &str) -> String {
        let worst: String = MdContainer::inkOn(fill, ink);
        if worst.is_empty() || paper.is_empty() {
            return worst;
        }
        let mut pc: Rc<RefCell<EVGColor>> = EVGColor::parse(paper);
        if false == pc.borrow().is_set || EVGColor::alpha(&pc) < 0.99 {
            return worst;
        }
        let dark: f64 = MdContainer::worstContrast("#111111", fill);
        let light: f64 = MdContainer::worstContrast("#FFFFFF", fill);
        if dark >= 4.5 || light >= 4.5 {
            return worst;
        }
        let under: String = MdContainer::overColour(fill, paper);
        if MdContainer::contrast(ink, &under) >= 4.5 {
            return String::new();
        }
        if MdContainer::contrast("#111111", &under) >= MdContainer::contrast("#FFFFFF", &under) {
            return "#111111".to_string();
        }
        "#FFFFFF".to_string()
    }
    /// `fill` laid over an opaque `paper`, as `#RRGGBB`
    pub fn overColour(fill: &str, paper: &str) -> String {
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(fill);
        let mut u: Rc<RefCell<EVGColor>> = EVGColor::parse(paper);
        if false == c.borrow().is_set {
            return MdContainer::hexOf(&EVGColor::rgb(
                EVGColor::red(&u),
                EVGColor::green(&u),
                EVGColor::blue(&u)
            )).clone();
        }
        let a: f64 = EVGColor::alpha(&c);
        let r: i64 = ((((EVGColor::red(&c)) as f64) * a + ((EVGColor::red(&u)) as f64) * (1.0 - a)) + 0.5).floor() as i64;
        let g: i64 = ((((EVGColor::green(&c)) as f64) * a + ((EVGColor::green(&u)) as f64) * (1.0 - a)) + 0.5).floor() as i64;
        let b: i64 = ((((EVGColor::blue(&c)) as f64) * a + ((EVGColor::blue(&u)) as f64) * (1.0 - a)) + 0.5).floor() as i64;
        MdContainer::hexOf(&EVGColor::rgb(r, g, b))
    }
    /// The contrast of `ink` on `fill` whatever is behind the plate: laid over
    /// black and over white, the lower of the two.
    pub fn worstContrast(ink: &str, fill: &str) -> f64 {
        let a: f64 = MdContainer::contrast(ink, &MdContainer::over(fill, 0));
        let b: f64 = MdContainer::contrast(ink, &MdContainer::over(fill, 255));
        if a < b {
            return a;
        }
        b
    }
    /// `fill` laid over a grey `under` (0 black … 255 white), as `#RRGGBB`
    pub fn over(fill: &str, under: i64) -> String {
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(fill);
        if false == c.borrow().is_set {
            return MdContainer::hexOf(&EVGColor::rgb(under, under, under));
        }
        let a: f64 = EVGColor::alpha(&c);
        let u: f64 = under as f64;
        let r: i64 = ((((EVGColor::red(&c)) as f64) * a + u * (1.0 - a)) + 0.5).floor() as i64;
        let g: i64 = ((((EVGColor::green(&c)) as f64) * a + u * (1.0 - a)) + 0.5).floor() as i64;
        let b: i64 = ((((EVGColor::blue(&c)) as f64) * a + u * (1.0 - a)) + 0.5).floor() as i64;
        MdContainer::hexOf(&EVGColor::rgb(r, g, b))
    }
    pub fn contrast(a: &str, b: &str) -> f64 {
        let la: f64 = MdContainer::luminance(a);
        let lb: f64 = MdContainer::luminance(b);
        if la < lb {
            return (lb + 0.05) / (la + 0.05);
        }
        (la + 0.05) / (lb + 0.05)
    }
    pub fn luminance(hex: &str) -> f64 {
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(hex);
        let r: f64 = MdContainer::channel((EVGColor::red(&c)) as f64);
        let g: f64 = MdContainer::channel((EVGColor::green(&c)) as f64);
        let b: f64 = MdContainer::channel((EVGColor::blue(&c)) as f64);
        (0.2126 * r + 0.7152 * g) + 0.0722 * b
    }
    pub fn channel(v: f64) -> f64 {
        let x: f64 = v / 255.0;
        if x <= 0.04045 {
            return x / 12.92;
        }
        let y: f64 = (x + 0.055) / 1.055;
        let q: f64 = y.sqrt().sqrt();
        ((y * y) * q) * q.sqrt()
    }
    pub(crate) fn hexOf(c: &Rc<RefCell<EVGColor>>) -> String {
        let hex: String = EVGColor::to_hex_string(&c);
        let a: f64 = EVGColor::alpha(&c);
        if a >= 0.999 {
            return hex;
        }
        let digits: String = "0123456789ABCDEF".to_string();
        let v: i64 = (a * 255.0 + 0.5).floor() as i64;
        let hi: i64 = v / 16;
        let lo: i64 = v % 16;
        format!("{}{}{}", hex, char::from_u32((MdText::char_at(&digits, hi)) as u32).unwrap_or('\0'), char::from_u32((MdText::char_at(&digits, lo)) as u32).unwrap_or('\0'))
    }
    /// ----------------------------------------------------------------------
    /// Fences that closed nothing
    /// ----------------------------------------------------------------------
    ///
    /// A `:::` closes the container it is at least as long as, the OUTER one
    /// first (as markdown-it-container does). `::: columns` round `::: col`
    /// blocks with as many colons is closed by the first column's `:::`, and
    /// the last `:::` is left over as a paragraph — shown on the page as
    /// colons. The source offsets of the paragraphs (anywhere in `doc`) with
    /// a line of nothing but three or more colons, for a note to the writer:
    /// the outer container wants more colons (`:::: columns` … `::::`).
    pub fn strayFences(doc: &Rc<RefCell<MdNode>>, src: &str) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        MdContainer::collectStray(doc, src, &mut out);
        out
    }
    pub fn collectStray(node: &Rc<RefCell<MdNode>>, src: &str, mut out: &mut Vec<i64>) {
        if node.borrow().kind == "paragraph" {
            let a: i64 = node.borrow().srcStart;
            let mut b: i64 = node.borrow().srcEnd;
            if b > (MdText::str_len(&src) as i64) {
                b = MdText::str_len(&src) as i64;
            }
            if a >= 0 && a < b {
                let mut lines: Vec<String> = MdText::substr(&src, a, b)
                    .split("\n")
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();
                for ln in lines.iter().cloned() {
                    if MdContainer::onlyColons(&ln.trim().to_string()) {
                        out.push(a);
                        return;
                    }
                }
            }
            return;
        }
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            MdContainer::collectStray(&node.borrow().childAt(i), src, &mut out);
            i += 1;
        }
    }
    pub fn onlyColons(s: &str) -> bool {
        let n: i64 = MdText::str_len(&s) as i64;
        if n < 3 {
            return false;
        }
        let mut i: i64 = 0;
        while i < n {
            if MdText::char_at(&s, i) != 58 {
                return false;
            }
            i += 1;
        }
        true
    }
    pub fn lower(s: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&s) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            if (c >= 65 && c <= 90) {
                out = format!("{}{}", out, char::from_u32((c + 32) as u32).unwrap_or('\0'));
            } else {
                // the unit itself (`strfromcode` of a byte breaks "ä" on Go)
                out = format!("{}{}", out, MdText::substr(&s, i, i + 1));
            }
            i += 1;
        }
        out
    }
}
