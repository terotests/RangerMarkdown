// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_node::MdNode;
use crate::rgr::MdText;

/// ============================================================================
/// MdColumns.rgr — blocks side by side: which block goes in which column, and
/// how wide each column is.
/// ============================================================================
///
/// ````markdown
/// ::: columns
/// ### Ennen
/// - Käsin tehdyt kalvot
///
/// ### Jälkeen
/// - Markdown + tekoäly
/// :::
///
/// ## Tulokset {layout=image-right}
///
/// - Kasvu jatkui
///
/// ![Kaavio](media/kasvu.png)
/// ````
///
/// A `::: columns` container (or `::: cols`) puts what is in it side by side.
/// Where one column ends and the next begins is read off what is there, the
/// first rule that applies:
///
///   1. `::: col` / `::: column` containers inside it: one column each (a
///      `{width=40%}` after a column's closing line is its width). Written
///      without a `columns` around them, side by side `::: col` blocks are a
///      row of columns too.
///   2. thematic breaks (`---`) inside it: the breaks are where it is cut.
///   3. headings: a column starts at each heading of the highest level there;
///      what comes before the first of them goes across, above the columns.
///   4. pictures, charts and galleries with other blocks: the pictures in
///      one column and the rest in the other, on the side the first block
///      is on;
///   5. otherwise each block is a column of its own (up to four; more blocks
///      than that are halved into two columns).
///
/// A heading's `{layout=…}` puts its SECTION (the blocks under it, up to the
/// next heading of its level or above) in columns the same way: `columns`,
/// `comparison`, `two-column`, and `image-right` / `image-left`, which put
/// the pictures, charts, diagrams and galleries in one column at that side
/// and everything else in the other. `MdColumns.wrapLayouts` wraps such a
/// section in a `columns` container when the document is read, so every
/// reader of the tree sees the same thing.
///
/// WIDTHS. `{widths="60 40"}` on the container (or the heading) says them;
/// else a column holding one block that says `{width=N%}` is that wide (the
/// block then fills its column), and the other columns share the rest.
/// Geometry depends only on the width it is given (the gap is a fixed share
/// of it) — `MdDiagram` prepares charts at the column's width before layout,
/// and the layout must ask for the very same width.
///
/// Nothing here measures or draws: `MdLayout.columnsBlock` places the blocks,
/// `tests/MdColumnsTest.rgr` checks the rules.
/// ============================================================================
#[derive(Clone)]
pub struct MdColumnsPlan {
    /// children laid out across the whole width, above the columns
    pub lead: Vec<i64>,
    /// per column, the children in it, in order
    pub groups: Vec<Vec<i64>>,
    /// per column, its share of the width left after the gaps (sum 1)
    pub fracs: Vec<f64>,
    /// children whose `width=` made their column's width: laid out at the
    /// column's full width, their own `width=` read as done
    pub taken: Vec<i64>,
}

impl MdColumnsPlan {
    pub fn new() -> Self {
        Self {
            lead: Vec::new(),
            groups: Vec::new(),
            fracs: Vec::new(),
            taken: Vec::new(),
        }
    }
    pub fn count(&self) -> i64 {
        self.groups.len() as i64
    }
    pub fn isTaken(&self, child: i64) -> bool {
        let mut i: i64 = 0;
        while i < (self.taken.len() as i64) {
            if self.taken[i as usize] == child {
                return true;
            }
            i += 1;
        }
        false
    }
    /// the column `child` is in, -1 when it goes across (or nowhere)
    pub fn columnOf(&self, child: i64) -> i64 {
        let mut g: i64 = 0;
        while g < (self.groups.len() as i64) {
            let mut grp: Vec<i64> = self.groups[g as usize].clone();
            let mut k: i64 = 0;
            while k < (grp.len() as i64) {
                if grp[k as usize] == child {
                    return g;
                }
                k += 1;
            }
            g += 1;
        }
        -1
    }
    pub fn widths(&self, w: f64) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        let n: i64 = self.groups.len() as i64;
        if n == 0 {
            return out;
        }
        let mut room: f64 = w - MdColumns::gapFor(w) * ((n - 1) as f64);
        if room < 0.0 {
            room = 0.0;
        }
        let mut i: i64 = 0;
        while i < n {
            out.push(room * self.fracs[i as usize]);
            i += 1;
        }
        out
    }
    /// where each column starts, from `x`
    pub fn lefts(&self, x: f64, w: f64) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        let mut ws: Vec<f64> = self.widths(w);
        let gap: f64 = MdColumns::gapFor(w);
        let mut at: f64 = x;
        let mut i: i64 = 0;
        while i < (ws.len() as i64) {
            out.push(at);
            at = (at + ws[i as usize]) + gap;
            i += 1;
        }
        out
    }
}

#[derive(Clone)]
pub struct MdColumns {
}

impl MdColumns {
    pub fn new() -> Self {
        Self {
        }
    }
    /// the space between two columns: a share of the width, never a style
    /// number (see the header)
    pub fn gapFor(w: f64) -> f64 {
        let mut g: f64 = w * 0.04;
        if g < 6.0 {
            g = 6.0;
        }
        g
    }
    pub fn isRow(node: &Rc<RefCell<MdNode>>) -> bool {
        if node.borrow().kind != "container" {
            return false;
        }
        let nm: String = MdChar::lower(&node.borrow().info.clone());
        nm == "columns" || nm == "cols"
    }
    pub fn isCell(node: &Rc<RefCell<MdNode>>) -> bool {
        if node.borrow().kind != "container" {
            return false;
        }
        let nm: String = MdChar::lower(&node.borrow().info.clone());
        nm == "col" || nm == "column"
    }
    /// a heading's `layout=` when it is one that puts its section in columns
    pub fn columnLayout(node: &Rc<RefCell<MdNode>>) -> String {
        if node.borrow().kind != "heading" {
            return String::new();
        }
        let w: String = MdChar::lower(&node.borrow().attrValue("layout").trim().to_string());
        if w == "columns" || w == "comparison" {
            return w;
        }
        if w == "two-column" || w == "two-columns" {
            return w;
        }
        if w == "image-right" || w == "image-left" {
            return w;
        }
        String::new()
    }
    /// ----------------------------------------------------------------------
    /// Reading the document
    /// ----------------------------------------------------------------------
    /// Every heading with a columns `layout=` gets its section wrapped in a
    /// `columns` container, and side-by-side `::: col` blocks without one
    /// get one too. Run once on a parsed document (MdBlock.run).
    pub fn wrapLayouts(node: &Rc<RefCell<MdNode>>) {
        let mut kept: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let n: i64 = node.borrow().childCount();
        // a heading's `{widths="30 70"}` with no `layout=`: the widths of the
        // first columns in its section (a `::: columns` of its own, or
        // `::: col` blocks side by side) that say none of their own
        let mut held_widths: String = String::new();
        let mut held_level: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            kept.push(c.clone());
            if (c.borrow().kind == "heading" && !held_widths.is_empty()) && c.borrow().level <= held_level {
                held_widths = String::new();
            }
            if (c.borrow().kind == "heading" && c.borrow().hasAttr("widths")) && (MdText::str_len(&MdColumns::columnLayout(&c)) as i64) == 0 {
                held_widths = c.borrow().attrValue("widths");
                held_level = c.borrow().level;
            }
            if !held_widths.is_empty() && MdColumns::isRow(&c) {
                if !c.borrow().hasAttr("widths") {
                    c.borrow_mut().setAttr("widths", &held_widths);
                }
                held_widths = String::new();
            }
            let word: String = MdColumns::columnLayout(&c);
            if !word.is_empty() {
                let mut j: i64 = i + 1;
                let mut going: bool = true;
                while going && j < n {
                    let mut d: Rc<RefCell<MdNode>> = node.borrow().childAt(j);
                    if d.borrow().kind == "heading" && d.borrow().level <= c.borrow().level {
                        going = false;
                    }
                    if d.borrow().kind == "front_matter" {
                        going = false;
                    }
                    if d.borrow().hasClass("slide") {
                        going = false;
                    }
                    if going {
                        j += 1;
                    }
                }
                if j > i + 1 {
                    let mut r#box: Rc<RefCell<MdNode>> = MdColumns::wrapper(i + 1, j, node);
                    r#box.borrow_mut().setAttr("layout", &word);
                    if c.borrow().hasAttr("widths") {
                        r#box.borrow_mut().setAttr("widths", &c.borrow().attrValue("widths"));
                    }
                    kept.push(r#box.clone());
                    i = j - 1;
                }
            } else {
                if MdColumns::isCell(&c) && false == MdColumns::isRow(node) {
                    let mut k: i64 = i + 1;
                    while k < n && MdColumns::isCell(&node.borrow().childAt(k)) {
                        k += 1;
                    }
                    if k > i + 1 {
                        kept.pop();
                        let mut row: Rc<RefCell<MdNode>> = MdColumns::wrapper(i, k, node);
                        if !held_widths.is_empty() {
                            row.borrow_mut().setAttr("widths", &held_widths);
                            held_widths = String::new();
                        }
                        kept.push(row.clone());
                        i = k - 1;
                    }
                }
            }
            i += 1;
        }
        node.borrow_mut().children = kept.clone();
        let mut m: i64 = 0;
        while m < (kept.len() as i64) {
            let mut k2: Rc<RefCell<MdNode>> = kept[m as usize].clone();
            if k2.borrow().kind != "heading" {
                MdColumns::wrapLayouts(&k2);
            }
            m += 1;
        }
    }
    /// In a deck, a heading's section ends where the next slide opens. A
    /// `# Cover {layout=image-right}` read as a document owns every `##`
    /// under it (wrapLayouts), but with `##` opening slides that is the whole
    /// deck in two columns, its title over every slide. Run once the split
    /// level is known (MdLayout.applySlideMeta): a section wrapped for a
    /// heading's `layout=` is cut at its first heading that opens a slide, and
    /// the rest goes back to the document after it. A `::: columns` the deck
    /// wrote itself is kept as written.
    pub fn endAtSlides(doc: &Rc<RefCell<MdNode>>, split_level: i64) {
        let n: i64 = doc.borrow().childCount();
        let mut kept: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let mut cut: bool = false;
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
            kept.push(c.clone());
            if (MdColumns::isRow(&c) && c.borrow().hasAttr("layout")) && i > 0 {
                let mut h: Rc<RefCell<MdNode>> = doc.borrow().childAt(i - 1);
                if h.borrow().kind == "heading" && MdColumns::columnLayout(&h) == c.borrow().attrValue("layout") {
                    let m: i64 = c.borrow().childCount();
                    let mut at: i64 = -1;
                    let mut k: i64 = 0;
                    while k < m && at < 0 {
                        let mut d: Rc<RefCell<MdNode>> = c.borrow().childAt(k);
                        if d.borrow().kind == "heading" && d.borrow().level <= split_level {
                            at = k;
                        }
                        k += 1;
                    }
                    if at >= 0 {
                        cut = true;
                        let mut own: Vec<Rc<RefCell<MdNode>>> = Vec::new();
                        let mut rest: Vec<Rc<RefCell<MdNode>>> = Vec::new();
                        let mut j: i64 = 0;
                        while j < m {
                            let mut d2: Rc<RefCell<MdNode>> = c.borrow().childAt(j);
                            if j < at {
                                own.push(d2.clone());
                            } else {
                                rest.push(d2.clone());
                            }
                            j += 1;
                        }
                        if !own.is_empty() {
                            c.borrow_mut().children = own.clone();
                            let mut last_own: Rc<RefCell<MdNode>> = own[((own.len() as i64) - 1) as usize].clone();
                            c.borrow_mut().srcEnd = last_own.borrow().srcEnd;
                        } else {
                            // nothing of its own left: no container
                            kept.pop();
                        }
                        let mut r: i64 = 0;
                        while r < (rest.len() as i64) {
                            kept.push(rest[r as usize].clone());
                            r += 1;
                        }
                    }
                }
            }
            i += 1;
        }
        if cut {
            doc.borrow_mut().children = kept.clone();
            // what came back may hold a cut of its own
            MdColumns::endAtSlides(doc, split_level);
        }
    }
    /// children from..to-1 of `parent` in a new `columns` container
    pub fn wrapper(from: i64, to: i64, parent: &Rc<RefCell<MdNode>>) -> Rc<RefCell<MdNode>> {
        let mut r#box: Rc<RefCell<MdNode>> = MdNode::create("container".to_string());
        r#box.borrow_mut().info = "columns".to_string();
        r#box.borrow_mut().addClass("columns");
        let mut first: Rc<RefCell<MdNode>> = parent.borrow().childAt(from);
        let mut last: Rc<RefCell<MdNode>> = parent.borrow().childAt(to - 1);
        r#box.borrow_mut().srcStart = first.borrow().srcStart;
        r#box.borrow_mut().srcEnd = last.borrow().srcEnd;
        r#box.borrow_mut().open = false;
        let mut i: i64 = from;
        while i < to {
            r#box.borrow_mut().add(parent.borrow().childAt(i));
            i += 1;
        }
        r#box.clone()
    }
    /// ----------------------------------------------------------------------
    /// The plan
    /// ----------------------------------------------------------------------
    pub fn plan(node: &Rc<RefCell<MdNode>>) -> Rc<RefCell<MdColumnsPlan>> {
        let mut p: Rc<RefCell<MdColumnsPlan>> = Rc::new(RefCell::new(MdColumnsPlan::new()));
        let n: i64 = node.borrow().childCount();
        let word: String = MdChar::lower(&node.borrow().attrValue("layout").trim().to_string());
        let mut any_cell: bool = false;
        let mut any_break: bool = false;
        let mut top: i64 = 7;
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if MdColumns::isCell(&c) {
                any_cell = true;
            }
            if c.borrow().kind == "thematic_break" {
                any_break = true;
            }
            if c.borrow().kind == "heading" && c.borrow().level < top {
                top = c.borrow().level;
            }
            i += 1;
        }
        if any_cell {
            MdColumns::byCells(node, &p);
        } else {
            if any_break {
                MdColumns::byBreaks(node, &p);
            } else {
                if word == "image-right" || word == "image-left" {
                    MdColumns::byMedia(node, &p, word == "image-left");
                } else {
                    if top < 7 {
                        MdColumns::byHeadings(node, &p, top);
                    } else {
                        MdColumns::byKind(node, &p);
                    }
                }
            }
        }
        MdColumns::share(node, &p);
        p
    }
    pub fn byCells(node: &Rc<RefCell<MdNode>>, p: &Rc<RefCell<MdColumnsPlan>>) {
        let mut cur: Vec<i64> = Vec::new();
        let mut open: bool = false;
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if MdColumns::isCell(&c) {
                if open {
                    p.borrow_mut().groups.push(cur.clone());
                }
                let mut fresh: Vec<i64> = Vec::new();
                cur = fresh.clone();
                cur.push(i);
                open = true;
            } else {
                if open {
                    cur.push(i);
                } else {
                    p.borrow_mut().lead.push(i);
                }
            }
            i += 1;
        }
        if open {
            p.borrow_mut().groups.push(cur.clone());
        }
    }
    pub fn byBreaks(node: &Rc<RefCell<MdNode>>, p: &Rc<RefCell<MdColumnsPlan>>) {
        let mut cur: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if c.borrow().kind == "thematic_break" {
                if !cur.is_empty() {
                    p.borrow_mut().groups.push(cur.clone());
                }
                let mut fresh: Vec<i64> = Vec::new();
                cur = fresh.clone();
            } else {
                cur.push(i);
            }
            i += 1;
        }
        if !cur.is_empty() {
            p.borrow_mut().groups.push(cur.clone());
        }
    }
    pub fn byHeadings(node: &Rc<RefCell<MdNode>>, p: &Rc<RefCell<MdColumnsPlan>>, level: i64) {
        let mut cur: Vec<i64> = Vec::new();
        let mut open: bool = false;
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if c.borrow().kind == "heading" && c.borrow().level == level {
                if open {
                    p.borrow_mut().groups.push(cur.clone());
                }
                let mut fresh: Vec<i64> = Vec::new();
                cur = fresh.clone();
                open = true;
            }
            if open {
                cur.push(i);
            } else {
                p.borrow_mut().lead.push(i);
            }
            i += 1;
        }
        if open {
            p.borrow_mut().groups.push(cur.clone());
        }
    }
    pub fn byMedia(node: &Rc<RefCell<MdNode>>, p: &Rc<RefCell<MdColumnsPlan>>, media_first: bool) {
        let mut media: Vec<i64> = Vec::new();
        let mut text: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            if MdColumns::isMedia(&node.borrow().childAt(i)) {
                media.push(i);
            } else {
                text.push(i);
            }
            i += 1;
        }
        if media.is_empty() {
            MdColumns::byBlocks(node, p);
            return;
        }
        if text.is_empty() {
            p.borrow_mut().groups.push(media.clone());
            return;
        }
        if media_first {
            p.borrow_mut().groups.push(media.clone());
            p.borrow_mut().groups.push(text.clone());
        } else {
            p.borrow_mut().groups.push(text.clone());
            p.borrow_mut().groups.push(media.clone());
        }
    }
    /// Pictures and charts with text: the two side by side, the side the
    /// first block is on first. All of one kind: a block a column.
    pub fn byKind(node: &Rc<RefCell<MdNode>>, p: &Rc<RefCell<MdColumnsPlan>>) {
        let mut media: i64 = 0;
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            if MdColumns::isMedia(&node.borrow().childAt(i)) {
                media += 1;
            }
            i += 1;
        }
        if media > 0 && media < node.borrow().childCount() {
            MdColumns::byMedia(node, p, MdColumns::isMedia(&node.borrow().childAt(0)));
            return;
        }
        MdColumns::byBlocks(node, p);
    }
    pub fn byBlocks(node: &Rc<RefCell<MdNode>>, p: &Rc<RefCell<MdColumnsPlan>>) {
        let n: i64 = node.borrow().childCount();
        if n <= 4 {
            let mut i: i64 = 0;
            while i < n {
                let mut one: Vec<i64> = Vec::new();
                one.push(i);
                p.borrow_mut().groups.push(one.clone());
                i += 1;
            }
            return;
        }
        let half: i64 = (n + 1) / 2;
        let mut a: Vec<i64> = Vec::new();
        let mut b: Vec<i64> = Vec::new();
        let mut k: i64 = 0;
        while k < n {
            if k < half {
                a.push(k);
            } else {
                b.push(k);
            }
            k += 1;
        }
        p.borrow_mut().groups.push(a);
        p.borrow_mut().groups.push(b);
    }
    /// A picture on its own, a chart or diagram, a gallery.
    pub fn isMedia(c: &Rc<RefCell<MdNode>>) -> bool {
        if c.borrow().kind == "embed" {
            return true;
        }
        if c.borrow().kind == "code_block" {
            let info: String = MdChar::lower(&c.borrow().info.trim().to_string());
            return MdText::index_of(&info, "gallery") == 0;
        }
        if c.borrow().kind != "paragraph" {
            return false;
        }
        let mut found: bool = false;
        let mut i: i64 = 0;
        while i < c.borrow().childCount() {
            let mut k: Rc<RefCell<MdNode>> = c.borrow().childAt(i);
            if k.borrow().kind == "image" {
                if found {
                    return false;
                }
                found = true;
            } else {
                if k.borrow().kind == "text" {
                    if !k.borrow().literal.trim().to_string().is_empty() {
                        return false;
                    }
                } else {
                    if k.borrow().kind != "softbreak" {
                        return false;
                    }
                }
            }
            i += 1;
        }
        found
    }
    /// A `width=` as a share of the whole: `40%` → 0.4, a plain number of
    /// points is not a share (0).
    pub fn shareOf(v0: &str) -> f64 {
        let v: String = v0.trim().to_string();
        let n: i64 = MdText::str_len(&v) as i64;
        if n < 2 {
            return 0.0;
        }
        if MdText::char_at(&v, n - 1) != 37 {
            return 0.0;
        }
        let d: Option<f64> = MdText::substr(&v, 0, n - 1).trim().to_string().parse::<f64>().ok();
        let f: f64 = (if d.is_some() { d.unwrap() } else { 0.0 }) / 100.0;
        if f <= 0.0 || f >= 1.0 {
            return 0.0;
        }
        f
    }
    /// The width a column's block (or `::: col`) asks for, 0 for none.
    pub fn askedOf(c: &Rc<RefCell<MdNode>>) -> f64 {
        let f: f64 = MdColumns::shareOf(&c.borrow().attrValue("width"));
        if f > 0.0 {
            return f;
        }
        if c.borrow().kind == "paragraph" {
            // `![…](…){width=40%}` on the picture itself
            let mut i: i64 = 0;
            while i < c.borrow().childCount() {
                let mut k: Rc<RefCell<MdNode>> = c.borrow().childAt(i);
                if k.borrow().kind == "image" {
                    let g: f64 = MdColumns::shareOf(&k.borrow().attrValue("width"));
                    if g > 0.0 {
                        return g;
                    }
                }
                i += 1;
            }
        }
        0.0
    }
    pub fn share(node: &Rc<RefCell<MdNode>>, p: &Rc<RefCell<MdColumnsPlan>>) {
        let n: i64 = p.borrow().groups.len() as i64;
        if n == 0 {
            return;
        }
        let mut said: Vec<f64> = MdColumns::numbers(&node.borrow().attrValue("widths"));
        if (said.len() as i64) == n {
            let mut total: f64 = 0.0;
            let mut i: i64 = 0;
            while i < n {
                total += said[i as usize];
                i += 1;
            }
            if total > 0.0 {
                let mut k: i64 = 0;
                while k < n {
                    p.borrow_mut().fracs.push(said[k as usize] / total);
                    k += 1;
                }
                return;
            }
        }
        let mut asked: Vec<f64> = Vec::new();
        let mut fixed: f64 = 0.0;
        let mut free: i64 = 0;
        let mut g: i64 = 0;
        while g < n {
            let mut grp: Vec<i64> = p.borrow().groups[g as usize].clone();
            let mut a: f64 = 0.0;
            if (grp.len() as i64) == 1 {
                a = MdColumns::askedOf(&node.borrow().childAt(grp[0]));
                if a > 0.0 {
                    p.borrow_mut().taken.push(grp[0]);
                }
            }
            asked.push(a);
            fixed += a;
            if a <= 0.0 {
                free += 1;
            }
            g += 1;
        }
        // asked for more than there is (or all of it with columns left
        // over): the asked shares give way in proportion
        let mut rest: f64 = 1.0 - fixed;
        let least: f64 = 0.15 * (free as f64);
        let mut scale: f64 = 1.0;
        if rest < least && fixed > 0.0 {
            scale = (1.0 - least) / fixed;
            rest = least;
        }
        if free == 0 && fixed > 0.0 {
            scale = 1.0 / fixed;
            rest = 0.0;
        }
        let mut h: i64 = 0;
        while h < n {
            let a2: f64 = asked[h as usize];
            if a2 > 0.0 {
                p.borrow_mut().fracs.push(a2 * scale);
            } else {
                p.borrow_mut().fracs.push(rest / (free as f64));
            }
            h += 1;
        }
    }
    /// "60 40", "60/40", "60%,40%" → [60 40]
    pub fn numbers(v: &str) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        let mut cur: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&v) as i64;
        while i <= n {
            let mut c: i64 = 32;
            if i < n {
                c = MdText::char_at(&v, i);
            }
            let digit: bool = (c >= 48 && c <= 57) || c == 46;
            if digit {
                cur = format!("{}{}", cur, MdText::substr(&v, i, i + 1));
            } else {
                if !cur.is_empty() {
                    let d: Option<f64> = cur.parse::<f64>().ok();
                    out.push(if d.is_some() { d.unwrap() } else { 0.0 });
                    cur = String::new();
                }
            }
            i += 1;
        }
        out
    }
}
