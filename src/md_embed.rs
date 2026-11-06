// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;

use crate::md_anchor::MdAnchor;
use crate::md_css::MdCss;
use crate::md_embed_kinds::MdEmbedKinds;
use crate::md_node::MdNode;
use crate::rgr::MdText;
use crate::rgr::{EVGElement};
use crate::rgr::{CssComputed, CssNode};

/// ============================================================================
/// MdEmbed.rgr — the slot a fenced diagram fills, and what fills it.
/// ============================================================================
///
/// A ```mermaid fence is not code and it is not an image: it is a drawing that
/// has to be MEASURED before the page can be laid out around it, and DRAWN
/// afterwards. Those are two different moments, so what sits between them is
/// this: a small store, keyed by the fence's own text.
///
///   MdDiagram.prepare   ──► fills the store (this is the only file in the
///                           module that knows RangerFlow exists)
///   MdLayout            ──► asks it how tall the slot is
///   MdToEvg             ──► asks it for the elements to put in the slot
///
/// Neither of the last two imports a diagram library, and the store itself
/// holds only `EVGElement` — which both of them already speak. That is what
/// keeps a markdown layout from depending on a graph editor.
///
/// The key is the fence's text and the width it was rendered at. Typing a
/// sentence three paragraphs below a diagram must not re-run a graph layout,
/// and re-flowing the same document at the same width must not either.
/// ============================================================================
pub struct MdEmbedEntry {
    pub kind: String,
    pub source: String,
    pub atWidth: f64,
    pub width: f64,
    pub height: f64,
    pub ok: bool,
    /// Why it is not drawn, when it is not. A reader is better served by
    /// "a gantt diagram — recognised, and nothing draws one here yet" than by
    /// an empty rectangle.
    pub note: String,
    /// what is wrong with a drawing that was drawn (a chart's field its data
    /// does not have), for a host's report
    pub warnings: Vec<String>,
    /// Which pass last wanted this entry. See `MdEmbedCache.endPass`.
    pub gen: i64,
    /// drawn under defaults (a chart's config, a figure's style) that have
    /// changed since: still shown, and drawn again when a pass gets to it
    /// (`MdEmbedCache.redrawBudget`)
    pub stale: bool,
    /// not drawn yet: its slide is out of view (`MdEmbedCache.lazy`). The
    /// layout gives it `height`, an estimate, and nothing is painted in it.
    pub deferred: bool,
    /// where its fence starts in the source, as of the last pass
    pub at: i64,
    /// Where in its column a drawing narrower than the column sits: "" (the
    /// start), "center" or "end". A chart says it in its spec
    /// (`"usermeta": {"presAlign": "center"}`), so a host's chart rule and a
    /// heading's own reach it the way the rest of the chart defaults do.
    pub align: String,
    /// the room a figure was fitted to (`MdEmbedCache.figureRoom`); another
    /// room draws it again
    pub room: f64,
    /// the height a slide's layout gave a chart or figure that was drawn
    /// taller than its room (the text written over and under it took the
    /// rest), 0 for none: it is drawn again that high (`MdEmbedCache.askRoom`)
    pub fitH: f64,
    pub(crate) root: Option<Rc<RefCell<EVGElement>>>,
}

impl MdEmbedEntry {
    pub fn new() -> Self {
        Self {
            kind: String::new(),
            source: String::new(),
            atWidth: 0.0,
            width: 0.0,
            height: 0.0,
            ok: false,
            note: String::new(),
            warnings: Vec::new(),
            gen: 0,
            stale: false,
            deferred: false,
            at: -1,
            align: String::new(),
            room: -1.0,
            fitH: 0.0,
            root: None,
        }
    }
    /// How far right of its column's start the drawing goes, in a column `w`
    /// wide.
    pub fn shiftIn(&self, w: f64) -> f64 {
        if self.width <= 0.0 || self.width >= w {
            return 0.0;
        }
        if self.align == "center" {
            return (w - self.width) * 0.5;
        }
        if self.align == "end" {
            return w - self.width;
        }
        0.0
    }
    /// How wide the drawing is in a column `w` wide: its own width when it is
    /// narrower, else the column. A drawing set at `shiftIn` and given the
    /// rest of the column was stretched to the column's end (an app's plate
    /// drawn wide and off to the right).
    pub fn widthIn(&self, w: f64) -> f64 {
        if self.width <= 0.0 || self.width >= w {
            return w;
        }
        self.width
    }
}

/// How the figures (```process, ```swot, ```timeline, ```cards — MdFigure) are coloured
/// and set: the host fills this from its theme. A change draws them again.
#[derive(Clone)]
pub struct MdFigureStyle {
    /// the series, in order; more items than colours come round lighter
    pub colors: Vec<String>,
    /// text on the page, the quieter text, and the page under the figure
    pub ink: String,
    pub muted: String,
    pub paper: String,
    pub font: String,
    /// the headings' font and colour, for the titles of ```cards ("": the
    /// text font, and the series' first colour)
    pub titleFont: String,
    pub heading: String,
    /// the cards under a figure's text (```stats, ```cards, ```swot, a
    /// timeline's): "" tints each from its colour, a colour sets them all
    /// (white cards), and `cardShadow` ("": none) is the colour of a soft
    /// shadow under each
    pub cardFill: String,
    pub cardShadow: String,
    /// the title size in layout units; descriptions are set smaller
    pub size: f64,
    /// false: a host that keeps figures for some users (Sliqtly: PRO) leaves
    /// them undrawn, and the slot says `lockedNote`
    pub allowed: bool,
    pub lockedNote: String,
    /// a height a grid of cards (```swot) stretches its rows to, 0: none
    pub fillH: f64,
    /// how far down a figure that does not stretch (```process, ```timeline)
    /// starts, to sit in the middle of `fillH` (MdFigure.sceneFit)
    pub shiftY: f64,
    /// how ```process and ```timeline go: 0 decide from the room (across
    /// while every text fits its slot, else down), 1 across, 2 down
    pub stack: i64,
    /// cards to a row in ```cards: 0 decide from the room (as many as fit,
    /// MdFigure.cardCols), else this many
    pub cols: i64,
    /// set by the drawing: some text was set smaller than asked to fit its
    /// box (a word longer than its column), and the smallest size any text
    /// came out at (0: none drawn yet)
    pub squeezed: bool,
    pub leastText: f64,
    /// set by a strip (```process, ```timeline across): some text did not
    /// fit its slot even at the smallest size (cut, or a word wider than
    /// its column), or was set far under the size asked
    pub cramped: bool,
}

impl MdFigureStyle {
    pub fn new() -> Self {
        let mut me = Self {
            colors: Vec::new(),
            ink: "#1f2937".to_string(),
            muted: "#6b7280".to_string(),
            paper: "#ffffff".to_string(),
            font: String::new(),
            titleFont: String::new(),
            heading: String::new(),
            cardFill: String::new(),
            cardShadow: String::new(),
            size: 18.0,
            allowed: true,
            lockedNote: "this layout is not available here".to_string(),
            fillH: 0.0,
            shiftY: 0.0,
            stack: 0,
            cols: 0,
            squeezed: false,
            leastText: 0.0,
            cramped: false,
        };
        let mut c_3: Vec<String> = vec!["#1f6feb".to_string(), "#0f9d8a".to_string(), "#7c4dff".to_string(), "#f08c00".to_string()];
        me.colors = c_3.clone();
        me
    }
    /// The same style at another size, for `MdFigure.sceneFit`.
    pub fn sized(&self, sz: f64) -> Rc<RefCell<MdFigureStyle>> {
        let mut o: Rc<RefCell<MdFigureStyle>> = Rc::new(RefCell::new(MdFigureStyle::new()));
        o.borrow_mut().colors = self.colors.clone();
        o.borrow_mut().ink = self.ink.clone();
        o.borrow_mut().muted = self.muted.clone();
        o.borrow_mut().paper = self.paper.clone();
        o.borrow_mut().font = self.font.clone();
        o.borrow_mut().titleFont = self.titleFont.clone();
        o.borrow_mut().heading = self.heading.clone();
        o.borrow_mut().cardFill = self.cardFill.clone();
        o.borrow_mut().cardShadow = self.cardShadow.clone();
        o.borrow_mut().size = sz;
        o.borrow_mut().allowed = self.allowed;
        o.borrow_mut().lockedNote = self.lockedNote.clone();
        o.borrow_mut().fillH = self.fillH;
        o.borrow_mut().shiftY = self.shiftY;
        o.borrow_mut().stack = self.stack;
        o.borrow_mut().cols = self.cols;
        o
    }
    pub fn key(&self) -> String {
        let mut k: String = format!("{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}", self.ink, self.muted, self.paper, self.font, self.size, self.allowed, self.stack, self.cols, self.titleFont, self.heading, self.cardFill, self.cardShadow);
        let mut i: i64 = 0;
        while i < (self.colors.len() as i64) {
            k = format!("{}|{}", k, self.colors[i as usize].clone());
            i += 1;
        }
        k
    }
}

#[derive(Clone)]
pub struct MdEmbedCache {
    pub entries: Vec<Rc<RefCell<MdEmbedEntry>>>,
    pub figure: Rc<RefCell<MdFigureStyle>>,
    pub figureKeyDrawn: String,
    /// Defaults for every chart (```vega-lite), as a JSON object the chart's
    /// own spec is laid over: {"padding": 10, "config": {"title": {…}}}.
    /// `chartScopes[i]` is a heading's anchor (MdAnchor.slug) and
    /// `chartScopeConfig[i]` the defaults of the charts under that heading,
    /// over the document's. The host sets these; "" means none. A change
    /// drops the charts already drawn (MdDiagram.prepare).
    pub chartConfig: String,
    pub chartScopes: Vec<String>,
    pub chartScopeConfig: Vec<String>,
    pub chartKeyDrawn: String,
    /// A chart narrower than its room drawn larger, this many times at most
    /// (0: as it asks). A slide wants its charts to fill it; a report does not.
    pub chartGrow: f64,
    /// and the tallest a grown chart may get (0: no limit) — a slide's page
    /// clips what runs over its bottom edge
    pub chartMaxH: f64,
    /// What a slide keeps over and under a grown chart or figure (its heading
    /// and the gaps); -1: nothing, and `chartMaxH` is the host's own number.
    /// Set, the room comes from the page being laid out (`fitPage`), so a
    /// turned or resized page is not drawn with the room of the page before.
    pub chartRoomKept: f64,
    /// a layout gave a drawing another room than it was drawn for (askRoom)
    pub roomAsked: bool,
    /// Files the charts read (`"data": {"url": "data/cars.json"}`): the host
    /// fetches them and hands them over, url and text, and a chart's url data
    /// is its rows (MdVegaRender.withFiles). A file arriving draws the charts
    /// again.
    pub dataUrls: Vec<String>,
    pub dataTexts: Vec<String>,
    /// each data text as chartKey last saw it, and its digest: a text the
    /// host hands over again unchanged is not read through again
    pub sigTexts: Vec<String>,
    pub sigs: Vec<String>,
    /// A list drawn as a figure: `{list-style=swot}` under it, or a rule of
    /// the layout's sheet (`.swot { list-style: swot }`, `ul { … }`), or the
    /// host's per-heading word — `listScopes[i]` an anchor (MdAnchor.slug),
    /// `listScopeStyles[i]` the list-style of the lists under it.
    pub listSheet: Rc<RefCell<MdCss>>,
    pub listScopes: Vec<String>,
    pub listScopeStyles: Vec<String>,
    /// the anchors of the headings a walk is under, by level (1..6)
    pub headingSlugs: Vec<String>,
    /// …their `{#id}`s ("" for none), and the headings themselves, so a
    /// sheet's `#intro list { list-style: process }` matches (MdCss.scopeOf)
    pub headingIds: Vec<String>,
    pub headingNodes: Vec<Rc<RefCell<MdNode>>>,
    /// The number of the pass now running. An entry that is not wanted during a
    /// pass is not wanted at all — see `endPass`.
    pub pass: i64,
    /// How many stale drawings one pass draws again; -1 (the default) is all
    /// of them, as before. A host that lays out on every keystroke sets a
    /// few, so a change to `chart { … }` on a deck of forty charts is not
    /// forty charts in one frame: the rest are drawn by the passes after,
    /// and until then shown as they were.
    pub redrawBudget: i64,
    pub redrawLeft: i64,
    /// The source offsets drawn again first, whatever the budget (the slide
    /// on the screen); -1 for none.
    pub focusFrom: i64,
    pub focusTo: i64,
    /// The stale drawings the last pass left for a later one.
    pub pending: i64,
    /// Drawing only what is in view. With `lazy` set, a pass draws a chart
    /// or figure only when its fence starts inside the focus or inside
    /// viewFrom..viewTo (the slides on screen and the next ones); the others
    /// get an entry with an estimated height and no drawing, and a stale
    /// drawing out of view is kept as it is. A deck of 200 slides then opens
    /// at the cost of the slides shown, and the host lays out again when
    /// `undrawnIn` says a slide coming into view has something to draw.
    pub lazy: bool,
    pub viewFrom: i64,
    pub viewTo: i64,
    /// fences the last pass left undrawn, or drawn under old defaults,
    /// because they were out of view
    pub deferred: i64,
    /// A heading of `level` read by the walk: its anchor, and the deeper
    /// levels' cleared.
    /// The level slides break at (the layout's `splitLevel`), set by a host
    /// that lays the document out as slides; 0: not known, and every figure's
    /// room is the page's (`chartMaxH`).
    pub splitLevel: i64,
    /// The slides that keep a strip at their side (MdLayout.asideAt, the
    /// source offsets of the blocks that want it) lay their body out
    /// `asideCut` narrower, and a diagram on one is drawn for that width
    /// (MdDiagram.prepare). Set by the host from the layout's own.
    pub asideAt: Vec<i64>,
    pub asideCut: f64,
    /// the level of the heading the walk is under, 0: none yet
    pub headLevel: i64,
}

impl MdEmbedCache {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            figure: Rc::new(RefCell::new(MdFigureStyle::new())),
            figureKeyDrawn: String::new(),
            chartConfig: String::new(),
            chartScopes: Vec::new(),
            chartScopeConfig: Vec::new(),
            chartKeyDrawn: String::new(),
            chartGrow: 0.0,
            chartMaxH: 0.0,
            chartRoomKept: -1.0,
            roomAsked: false,
            dataUrls: Vec::new(),
            dataTexts: Vec::new(),
            sigTexts: Vec::new(),
            sigs: Vec::new(),
            listSheet: Rc::new(RefCell::new(MdCss::new())),
            listScopes: Vec::new(),
            listScopeStyles: Vec::new(),
            headingSlugs: Vec::new(),
            headingIds: Vec::new(),
            headingNodes: Vec::new(),
            pass: 0,
            redrawBudget: -1,
            redrawLeft: 0,
            focusFrom: -1,
            focusTo: -1,
            pending: 0,
            lazy: false,
            viewFrom: -1,
            viewTo: -1,
            deferred: 0,
            splitLevel: 0,
            asideAt: Vec::new(),
            asideCut: 0.0,
            headLevel: 0,
        }
    }
    /// Two polynomial hashes over the text's units and its length; each
    /// stays below 2^53, so every target computes the same digits.
    pub fn textSig(s: &str) -> String {
        let mut a: i64 = 7;
        let mut b: i64 = 11;
        let n: i64 = MdText::str_len(&s) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            a = (a * 31 + c) % 1000000007;
            b = (b * 131 + c) % 998244353;
            i += 1;
        }
        format!("{}-{}-{}", n.to_string(), a, b)
    }
    /// ----------------------------------------------------------------------
    /// Keeping the store the size of the document
    /// ----------------------------------------------------------------------
    ///
    /// The cache outlives an edit on purpose: typing a sentence three
    /// paragraphs below a diagram must not re-run a graph layout. But typing
    /// INSIDE a fence changes the key on every keystroke, and without this a
    /// store that was meant to hold one diagram held one per character typed —
    /// each with a full element tree hanging off it — and the status line,
    /// which counts what is in here, said the document had sixty diagrams in
    /// it when it had one.
    ///
    /// So a render is a PASS: every entry the document still wants is touched
    /// during it, and what was not touched is dropped at the end. A pass in
    /// which nothing changed touches everything and drops nothing, which is
    /// the case that has to stay free.
    /// The tallest a grown chart or figure may be on a page `pageHeight`
    /// high with `margin` on each edge — the page the layout is about to use,
    /// which only the layout knows after the front matter and the reader's
    /// page size (or `orientation:`) have been applied.
    pub fn fitPage(&mut self, page_height: f64, margin: f64) {
        if self.chartRoomKept < 0.0 {
            return;
        }
        self.chartMaxH = (page_height - margin * 2.0) - self.chartRoomKept;
        if self.chartMaxH < 1.0 {
            self.chartMaxH = 1.0;
        }
    }
    /// The digest of data text `f`: a file read again (a live source, a
    /// questionnaire's counts) whose length stayed the same still draws its
    /// charts again.
    pub fn dataSig(&mut self, f: i64) -> String {
        let text: String = self.dataTexts[f as usize].clone();
        let mut i: i64 = 0;
        while i < (self.sigTexts.len() as i64) {
            if self.sigTexts[i as usize].clone() == text {
                return self.sigs[i as usize].clone();
            }
            i += 1;
        }
        let sig: String = MdEmbedCache::textSig(&text);
        // the texts of the last few calls only
        if (self.sigTexts.len() as i64) >= 64 {
            let mut none1: Vec<String> = Vec::new();
            let mut none2: Vec<String> = Vec::new();
            self.sigTexts = none1.clone();
            self.sigs = none2.clone();
        }
        self.sigTexts.push(text);
        self.sigs.push(sig.clone());
        sig
    }
    pub fn chartKey(&mut self) -> String {
        let mut k: String = format!("{}|grow:{}/{}", self.chartConfig, self.chartGrow, self.chartMaxH);
        let mut f: i64 = 0;
        while f < (self.dataUrls.len() as i64) {
            k = format!("{}|file:{}:{}", k, self.dataUrls[f as usize].clone(), self.dataSig(f));
            f += 1;
        }
        let mut i: i64 = 0;
        while i < (self.chartScopes.len() as i64) {
            k = format!("{}|{}={}", k, self.chartScopes[i as usize].clone(), self.chartScopeConfig[i as usize].clone());
            i += 1;
        }
        k
    }
    /// The room a figure at the walk's place is fitted to, `width` wide: the
    /// slide's (`chartMaxH`) under a heading that starts a slide; under a
    /// deeper one (the editorial theme's `##` on an A4 page that holds
    /// several sections) it is a part of a page, not the page, and is never
    /// taller than 0.7 of its width.
    pub fn inSection(&self) -> bool {
        self.splitLevel > 0 && self.headLevel > self.splitLevel
    }
    pub fn figureRoom(&self, width: f64) -> f64 {
        if self.inSection() {
            let cap: f64 = width * 0.7;
            if self.chartMaxH <= 0.0 || cap < self.chartMaxH {
                return cap;
            }
        }
        self.chartMaxH
    }
    /// A heading the walk passes: its anchor, its `{#id}` and itself.
    pub fn underHeadingNode(&mut self, h: &Rc<RefCell<MdNode>>) {
        let mut _tmp_1731 = h.borrow().level.clone();
        self.underHeading(_tmp_1731, &MdAnchor::slug(&h.borrow().plainText()).clone());
        if h.borrow().level < 1 || h.borrow().level > 6 {
            return;
        }
        while (self.headingIds.len() as i64) < 7 {
            self.headingIds.push(String::new().to_string());
            self.headingNodes.push(Rc::new(RefCell::new(MdNode::new())));
        }
        self.headingIds[(h.borrow().level) as usize] = h.borrow().nodeId.clone();
        self.headingNodes[(h.borrow().level) as usize] = h.clone();
        let mut d: i64 = h.borrow().level + 1;
        while d <= 6 {
            self.headingIds[d as usize] = String::new();
            self.headingNodes[d as usize] = Rc::new(RefCell::new(MdNode::new()));
            d += 1;
        }
    }
    /// whether the walk's heading at `lv` is the one `scope` names (its
    /// anchor or its `{#id}`)
    pub fn headingIs(&self, lv: i64, scope: &str) -> bool {
        if scope.is_empty() {
            return false;
        }
        if lv < (self.headingSlugs.len() as i64) && self.headingSlugs[lv as usize].clone() == scope {
            return true;
        }
        if lv < (self.headingIds.len() as i64) && self.headingIds[lv as usize].clone() == scope {
            return true;
        }
        false
    }
    pub fn underHeading(&mut self, level: i64, slug: &str) {
        if (level >= 1 && level <= 6) {
            self.headLevel = level;
        }
        while (self.headingSlugs.len() as i64) < 7 {
            self.headingSlugs.push(String::new().to_string());
        }
        if level < 1 || level > 6 {
            return;
        }
        self.headingSlugs[level as usize] = slug.to_string();
        let mut d: i64 = level + 1;
        while d <= 6 {
            self.headingSlugs[d as usize] = String::new();
            d += 1;
        }
    }
    pub fn resetHeadings(&mut self) {
        let mut none: Vec<String> = Vec::new();
        self.headingSlugs = none.clone();
        let mut no_ids: Vec<String> = Vec::new();
        self.headingIds = no_ids.clone();
        let mut no_nodes: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        self.headingNodes = no_nodes.clone();
        self.headLevel = 0;
    }
    /// The scoped defaults for where the walk is: the deepest heading that has
    /// some; "" when none does.
    pub fn scopedConfig(&self) -> String {
        let mut lv: i64 = (self.headingSlugs.len() as i64) - 1;
        while lv >= 1 {
            let mut i: i64 = 0;
            while i < (self.chartScopes.len() as i64) {
                if self.headingIs(lv, &self.chartScopes[i as usize].clone()) {
                    return self.chartScopeConfig[i as usize].clone();
                }
                i += 1;
            }
            lv -= 1;
        }
        String::new()
    }
    /// The list-style the lists under the walk's heading take; "" when none.
    pub fn scopedListStyle(&self) -> String {
        let mut lv: i64 = (self.headingSlugs.len() as i64) - 1;
        while lv >= 1 {
            let mut i: i64 = 0;
            while i < (self.listScopes.len() as i64) {
                if self.headingIs(lv, &self.listScopes[i as usize].clone()) {
                    return self.listScopeStyles[i as usize].clone();
                }
                i += 1;
            }
            lv -= 1;
        }
        String::new()
    }
    /// What a list is drawn as: its own `{list-style=…}`, then its heading's
    /// (an anchor, as specific as an id), then the sheet's rule for it.
    pub fn listStyleOf(&self, node: &Rc<RefCell<MdNode>>) -> String {
        let own: String = node.borrow().attrValue("list-style").trim().to_string();
        if !own.is_empty() {
            return own;
        }
        let scoped: String = self.scopedListStyle();
        if !scoped.is_empty() {
            return scoped;
        }
        let mut ctx: Option<Rc<RefCell<CssNode>>> = MdCss::scopeOf(&self.headingNodes.clone());
        let mut got: Rc<RefCell<CssComputed>> = self.listSheet.borrow_mut().resolveIn(node, ctx.clone());
        if CssComputed::has(&got, "list-style") {
            let v: String = CssComputed::get(&got, "list-style").trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
        // `list { list-style }`, the theme's word for every list
        let mut as_list: Rc<RefCell<CssComputed>> = self.listSheet.borrow_mut().resolveAs(node, "list".to_string(), ctx.clone());
        if CssComputed::has(&as_list, "list-style") {
            let v2: String = CssComputed::get(&as_list, "list-style").trim().to_string();
            if !v2.is_empty() {
                return v2;
            }
        }
        String::new()
    }
    pub fn beginPass(&mut self) {
        self.pass += 1;
        self.redrawLeft = self.redrawBudget;
        self.pending = 0;
        self.deferred = 0;
    }
    /// Whether a fence starting at `at` is drawn in this pass: always, unless
    /// the cache is lazy and it is neither in the focus nor in view.
    pub fn wanted(&self, at: i64) -> bool {
        if false == self.lazy {
            return true;
        }
        if self.focusFrom >= 0 && (at >= self.focusFrom && at < self.focusTo) {
            return true;
        }
        if self.viewFrom >= 0 && (at >= self.viewFrom && at < self.viewTo) {
            return true;
        }
        false
    }
    /// The entries starting in from..to that a pass would draw if they were
    /// wanted: not drawn yet, or drawn under old defaults.
    pub fn undrawnIn(&self, from: i64, to: i64) -> i64 {
        let mut n: i64 = 0;
        let mut i: i64 = 0;
        while i < (self.entries.len() as i64) {
            let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
            if (e.borrow().deferred || e.borrow().stale) && (e.borrow().at >= from && e.borrow().at < to) {
                n += 1;
            }
            i += 1;
        }
        n
    }
    /// Every drawing of a notation ("vega", "figure") is stale: its defaults
    /// changed.
    pub fn markStale(&self, notation: &str) {
        let mut i: i64 = 0;
        while i < (self.entries.len() as i64) {
            let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
            if MdEmbedKinds::notationOf(&e.borrow().kind.clone()) == notation {
                e.borrow_mut().stale = true;
            }
            i += 1;
        }
    }
    /// Whether this pass draws again a stale drawing whose fence starts at
    /// `at`: always inside the focus, otherwise while the budget lasts. A
    /// drawing left stale is counted in `pending`, unless it is out of view
    /// (`lazy`).
    pub fn mayRedraw(&mut self, at: i64) -> bool {
        // out of view: kept as drawn until it comes into view
        if false == self.wanted(at) {
            return false;
        }
        if self.redrawBudget < 0 {
            return true;
        }
        if self.focusFrom >= 0 && (at >= self.focusFrom && at < self.focusTo) {
            return true;
        }
        if self.redrawLeft > 0 {
            self.redrawLeft -= 1;
            return true;
        }
        self.pending += 1;
        false
    }
    pub fn removeAt(&mut self, i: i64) {
        let mut kept: Vec<Rc<RefCell<MdEmbedEntry>>> = Vec::new();
        let mut k: i64 = 0;
        while k < (self.entries.len() as i64) {
            if k != i {
                kept.push(self.entries[k as usize].clone());
            }
            k += 1;
        }
        self.entries = kept.clone();
    }
    pub fn touch(&self, i: i64) {
        let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
        e.borrow_mut().gen = self.pass;
    }
    pub fn endPass(&mut self) {
        let mut kept: Vec<Rc<RefCell<MdEmbedEntry>>> = Vec::new();
        let mut i: i64 = 0;
        let n: i64 = self.entries.len() as i64;
        while i < n {
            let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
            if e.borrow().gen == self.pass {
                kept.push(e.clone());
            }
            i += 1;
        }
        self.entries = kept.clone();
    }
    pub fn find(&self, kind: &str, source: &str, width: f64) -> i64 {
        let mut i: i64 = 0;
        let n: i64 = self.entries.len() as i64;
        while i < n {
            let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
            if e.borrow().kind == kind && e.borrow().source == source {
                let mut d: f64 = e.borrow().atWidth - width;
                if d < 0.0 {
                    d = 0.0 - d;
                }
                if d < 0.5 {
                    return i;
                }
            }
            i += 1;
        }
        -1
    }
    pub fn entryAt(&self, i: i64) -> Rc<RefCell<MdEmbedEntry>> {
        self.entries[i as usize].clone()
    }
    /// the box a slide's layout last gave a drawing (askRoom), 0 for none
    pub fn fitFor(&self, kind: &str, source: &str, width: f64) -> f64 {
        let i: i64 = self.find(kind, source, width);
        if i < 0 {
            return 0.0;
        }
        let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
        return e.borrow().fitH;
    }
    /// A slide's layout gave the chart or figure `kind`/`source` a box `h`
    /// high, other than the room it was drawn for: it is drawn again for it
    /// (stale) and `roomAsked` says the host should prepare and lay out once
    /// more (MdDiagram.refit). A chart grown to the slide's room ran down
    /// over the line written under it, its box cut and its drawing not.
    pub fn askRoom(&mut self, kind: &str, source: &str, width: f64, h: f64) {
        let i: i64 = self.find(kind, source, width);
        if i < 0 {
            return;
        }
        let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
        let mut d: f64 = e.borrow().fitH - h;
        if d < 0.0 {
            d = 0.0 - d;
        }
        if d <= 1.0 {
            return;
        }
        e.borrow_mut().fitH = h;
        e.borrow_mut().stale = true;
        self.roomAsked = true;
    }
    pub fn add(&mut self, e: Rc<RefCell<MdEmbedEntry>>) {
        e.borrow_mut().gen = self.pass;
        self.entries.push(e.clone());
    }
    pub fn count(&self) -> i64 {
        self.entries.len() as i64
    }
    /// The height the slot needs, or -1 when nothing has been prepared for it.
    pub fn heightFor(&self, kind: &str, source: &str, width: f64) -> f64 {
        let i: i64 = self.find(kind, source, width);
        if i < 0 {
            return 0.0 - 1.0;
        }
        let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
        if e.borrow().deferred {
            return e.borrow().height;
        }
        if false == e.borrow().ok {
            return 0.0 - 1.0;
        }
        return e.borrow().height;
    }
    pub fn noteFor(&self, kind: &str, source: &str, width: f64) -> String {
        let i: i64 = self.find(kind, source, width);
        if i < 0 {
            return String::new();
        }
        let mut e: Rc<RefCell<MdEmbedEntry>> = self.entries[i as usize].clone();
        return e.borrow().note.clone();
    }
}
