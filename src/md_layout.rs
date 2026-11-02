// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::md_attrs::MdAttrs;
use crate::md_block::MdBlock;
use crate::md_book::MdBook;
use crate::md_cell_tones::MdCellTones;
use crate::md_char::MdChar;
use crate::md_code_highlight::MdCodeHighlight;
use crate::md_code_highlight::MdHighlightedLine;
use crate::md_code_highlight::MdLanguage;
use crate::md_code_highlight::MdToken;
use crate::md_code_lines::MdCodeLine;
use crate::md_code_lines::MdCodeLines;
use crate::md_code_lines::MdCodeWrap;
use crate::md_columns::MdColumns;
use crate::md_columns::MdColumnsPlan;
use crate::md_container::MdContainer;
use crate::md_css::MdCss;
use crate::md_css_limits::MdCssLimits;
use crate::md_embed::MdEmbedCache;
use crate::md_embed::MdEmbedEntry;
use crate::md_embed_kinds::MdEmbedKinds;
use crate::md_front_matter::MdFrontMatter;
use crate::md_front_matter::MdMeta;
use crate::md_gallery::MdCellLook;
use crate::md_gallery::MdGallery;
use crate::md_gallery::MdGalleryCell;
use crate::md_gallery::MdGalleryItem;
use crate::md_gallery::MdGalleryLook;
use crate::md_html::MdHtml;
use crate::md_html::MdHtmlCell;
use crate::md_html::MdHtmlRow;
use crate::md_html::MdHtmlTable;
use crate::md_html::MdHtmlTag;
use crate::md_html_md::MdHtmlList;
use crate::md_html_md::MdHtmlMd;
use crate::md_html_md::MdHtmlStyle;
use crate::md_inline::MdInline;
use crate::md_node::MdNode;
use crate::md_slide_layout::MdSlideLayout;
use crate::md_slide_meta::MdSlideMeta;
use crate::md_style::MdStyle;
use crate::md_style::MdTypo;
use crate::md_svg::MdSvg;
use crate::md_table_split::MdTablePlan;
use crate::md_table_split::MdTableSplit;
use crate::md_url::MdUrl;
use crate::rgr::MdText;
use crate::rgr::{EVGTextEngine, EVGTextMeasurer, EVGTextMetrics, EVGUnit, Vfs, VfsStat};
use rangermath::{MathBox, MathLayout};

/// ============================================================================
/// MdLayout.rgr — the AST, laid out. Boxes with coordinates, and nothing drawn.
/// ============================================================================
///
/// This is the half the viewer actually uses. `MdToHtml` exists so the parser
/// can be scored against a specification; this exists so the document can be
/// PRINTED and DRAWN, which is a different problem with a different answer:
/// there is no DOM under it, so the position of every line is decided here.
///
///   MdNode tree ──► MdLayout ──► [MdBox] ──► MdToEvg ──► EVGElement
///                                                          │
///                                        ┌─────────────────┴───────────────┐
///                                        ▼                                 ▼
///                                EVGPDFRenderer                   EVGDisplayList
///                                    → PDF                          → WebGL
///
/// **The one thing EVG could not already do.** `EVGTextEngine.breakLines`
/// breaks a paragraph of ONE font at ONE size. A markdown paragraph is
///
///     some **bold** words and `code` and *italic*
///
/// — one line, four faces, and the break has to be found across all of them.
/// `breakRuns` below is that: the same greedy algorithm, with the accumulator
/// carrying a change of face instead of only a width, and each SEGMENT
/// measured whole so the real advance of a space inside a run is included
/// rather than guessed. A line's height and baseline are the maximum over its
/// segments, so a 9pt code span inside 11pt prose sits on the right baseline
/// instead of half a line up.
///
/// Everything is in POINTS, because the output that cannot be re-flowed later
/// is the PDF. The screen scales; the page does not.
/// ============================================================================
/// ----------------------------------------------------------------------------
/// What the document looks like. Not a stylesheet yet — the values a stylesheet
/// will set, in one place, so the layout never has a number written into it.
/// ----------------------------------------------------------------------------
#[derive(Clone)]
pub struct MdRun {
    pub text: String,
    pub family: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: String,
    /// inline code gets a tinted box behind it
    pub code: bool,
    /// a link's destination, kept so the viewer can hit-test it later
    pub link: String,
    /// a forced break — two spaces at the end of a line, or a backslash
    pub hardBreak: bool,
    /// Where this run's text came from, in the document — the span of the
    /// inline node it was made from. -1 when the node had none to give.
    ///
    /// It is carried rather than looked up because the run is where the text
    /// and the source stop being the same length: `&amp;` is one character of
    /// run text and five of source, so an offset INSIDE a run is exact only
    /// where the run is verbatim, and `srcEnd` is what stops an inexact one
    /// running off the end of the node it belongs to.
    pub srcStart: i64,
    pub srcEnd: i64,
    /// The run stands for its whole span rather than spelling it — `©` for
    /// `(c)`, 😉 for `:wink:` (`MdNode.subst`). Its offsets are the two ends
    /// of the span and nothing between. Not a style, so not copied.
    pub atom: bool,
    /// TeX math (`$…$`): the run is one unbreakable formula, its text a
    /// placeholder character, and `math` the source it is set from.
    pub math: String,
    pub mathDisplay: bool,
    /// What inline HTML (and ~~ ~~) asks of a run beyond its face: a line under
    /// or through it, a highlight or key cap behind it, and a shift off the
    /// baseline for <sub> / <sup> (-1 / +1) measured against `shiftOf`, the
    /// size of the text it sits in.
    pub underline: bool,
    pub strike: bool,
    /// drawn muted on purpose: the stand-in for a picture with no bytes
    pub quiet: bool,
    pub mark: bool,
    pub kbd: bool,
    pub shift: i64,
    pub shiftOf: f64,
    /// `letter-spacing`, in points: added after every character
    pub spacing: f64,
    /// a colour behind the text (`<span style="background-color">`), "" none
    pub fill: String,
    /// A footnote reference: the number it draws. Not part of the style, so
    /// not copied with it.
    pub noteRef: i64,
}

impl MdRun {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            family: String::new(),
            size: 11.0,
            bold: false,
            italic: false,
            color: "#1f2328".to_string(),
            code: false,
            link: String::new(),
            hardBreak: false,
            srcStart: -1,
            srcEnd: -1,
            atom: false,
            math: String::new(),
            mathDisplay: false,
            underline: false,
            strike: false,
            quiet: false,
            mark: false,
            kbd: false,
            shift: 0,
            shiftOf: 0.0,
            spacing: 0.0,
            fill: String::new(),
            noteRef: 0,
        }
    }
    /// The source span is NOT copied: a style is inherited by a child run and
    /// a position is not. Copying it is how `*a* b` would have had both runs
    /// claiming the same character.
    pub fn copyStyle(&self) -> Rc<RefCell<MdRun>> {
        let mut r: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        r.borrow_mut().family = self.family.clone();
        r.borrow_mut().size = self.size;
        r.borrow_mut().bold = self.bold;
        r.borrow_mut().italic = self.italic;
        r.borrow_mut().color = self.color.clone();
        r.borrow_mut().code = self.code;
        r.borrow_mut().link = self.link.clone();
        r.borrow_mut().underline = self.underline;
        r.borrow_mut().strike = self.strike;
        r.borrow_mut().quiet = self.quiet;
        r.borrow_mut().mark = self.mark;
        r.borrow_mut().kbd = self.kbd;
        r.borrow_mut().shift = self.shift;
        r.borrow_mut().shiftOf = self.shiftOf;
        r.borrow_mut().spacing = self.spacing;
        r.borrow_mut().fill = self.fill.clone();
        r
    }
}

/// A run, placed.
#[derive(Clone)]
pub struct MdSeg {
    pub text: String,
    pub family: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: String,
    pub code: bool,
    pub link: String,
    pub x: f64,
    pub w: f64,
    /// Where this placed piece of text starts in the document, and how much of
    /// the document it covers. This is the finest thing the layout knows, and
    /// it is what a click turns into a caret.
    pub srcStart: i64,
    pub srcLen: i64,
    /// from an atom run: the text covers srcLen characters as one
    pub atom: bool,
    pub math: String,
    pub mathDisplay: bool,
    pub underline: bool,
    pub strike: bool,
    /// drawn muted on purpose: the stand-in for a picture with no bytes
    pub quiet: bool,
    pub mark: bool,
    pub kbd: bool,
    pub shift: i64,
    pub shiftOf: f64,
    pub fill: String,
    pub noteRef: i64,
    pub spacing: f64,
}

impl MdSeg {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            family: String::new(),
            size: 11.0,
            bold: false,
            italic: false,
            color: "#1f2328".to_string(),
            code: false,
            link: String::new(),
            x: 0.0,
            w: 0.0,
            srcStart: -1,
            srcLen: 0,
            atom: false,
            math: String::new(),
            mathDisplay: false,
            underline: false,
            strike: false,
            quiet: false,
            mark: false,
            kbd: false,
            shift: 0,
            shiftOf: 0.0,
            fill: String::new(),
            noteRef: 0,
            spacing: 0.0,
        }
    }
}

#[derive(Clone)]
pub struct MdLine {
    pub segs: Vec<Rc<RefCell<MdSeg>>>,
    pub width: f64,
    pub height: f64,
    pub ascent: f64,
    /// Room made above the text for a formula that rises past the line: the
    /// text moves down by this much and the line is that much taller.
    pub extraTop: f64,
}

impl MdLine {
    pub fn new() -> Self {
        Self {
            segs: Vec::new(),
            width: 0.0,
            height: 0.0,
            ascent: 0.0,
            extraTop: 0.0,
        }
    }
}

/// A picture in a header or footer place, as the byte store has it.
#[derive(Clone)]
pub struct MdSlotPic {
    pub path: String,
    pub w: f64,
    pub h: f64,
    pub natW: i64,
    pub natH: i64,
}

impl MdSlotPic {
    pub fn new() -> Self {
        Self {
            path: String::new(),
            w: 0.0,
            h: 0.0,
            natW: 0,
            natH: 0,
        }
    }
}

/// One thing to draw. The layout produces a flat list of these and the EVG
/// bridge turns each into an element; nothing above this file knows about
/// markdown and nothing below it knows about layout.
pub struct MdBox {
    /// 0 text, 1 filled rectangle, 2 rectangle outline, 3 an embed's slot,
    /// 4 a picture, 5 a TeX formula: `embedSource` its source, `embedKind`
    /// "math" or "math-display", `size` the pixels per em it is set at; 6 an
    /// effect over a block (`fxNames`)
    pub kind: i64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub text: String,
    pub family: String,
    pub size: f64,
    pub bold: bool,
    /// `letter-spacing` in points, after every character
    pub spacing: f64,
    pub italic: bool,
    pub color: String,
    pub fill: String,
    pub stroke: String,
    pub strokeWidth: f64,
    pub link: String,
    /// the source of an embed, for whoever can draw it
    pub embedKind: String,
    pub embedSource: String,
    /// why an embed is not drawn (a box with the reason written in it), ""
    /// when it is drawn
    pub embedNote: String,
    /// A PICTURE (kind 4): where its bytes are in the byte store, and how big
    /// they are in their own pixels. The layout names the picture and does not
    /// carry it, exactly as a display list does — whoever draws it asks the
    /// same `Vfs` the layout asked.
    pub imagePath: String,
    pub imageAlt: String,
    pub imageNaturalW: i64,
    pub imageNaturalH: i64,
    pub page: i64,
    /// The character this box's block began at, in the source text. Stamped by
    /// the layout, not by the author: it is what lets a click on a drawing find
    /// a caret in the text a reader typed.
    pub srcStart: i64,
    /// …and the character THIS BOX begins at, which is a different question.
    /// `srcStart` names the block, so every box of a paragraph carries the same
    /// one and a click could name the paragraph and nothing smaller. `segSrc`
    /// is the placed run's own start, so a click inside the box can be turned
    /// into an offset by measuring across it.
    pub segSrc: i64,
    pub segLen: i64,
    /// The box's text stands for all of segSrc..segSrc+segLen at once (a
    /// typographic replacement, an emoji): a caret is at one end of it or the
    /// other, never at a character inside.
    pub segAtom: bool,
    /// the slide's title said again on a continuation (`emitSlideTitle`):
    /// laid out while a block was, but not part of it, so the plate around
    /// a box and the extent text floats beside leave it out
    pub retold: bool,
    /// 1 a header's, 2 a footer's: page furniture, not the document's text;
    /// 3 the footnotes at the foot of a page, which ARE the document's text
    /// but are placed by the page rather than where they were written
    pub furniture: i64,
    /// text struck through (`~~…~~`, `<s>`, `<del>`): drawn muted on purpose,
    /// so a contrast check leaves it alone
    pub strike: bool,
    /// drawn muted on purpose for another reason: the "[alt]" standing in for
    /// a picture the layout has no bytes for (the missing picture is the
    /// thing to report, not its stand-in's contrast), and a code block's
    /// line numbers in the gutter (`.numbers`), dim beside the code they count
    pub quiet: bool,
    /// a line set centred or to the right ("center" / "right"; "" at the
    /// left) in the column alignX..alignX+alignW, so an export can say so
    /// instead of placing the text where this layout happened to put it
    pub align: String,
    pub alignX: f64,
    pub alignW: f64,
    /// a code block's highlight that a build step reveals (`lines=3-5|8`
    /// with `{.build}`): the step among the block's own, 0 = with the block
    pub buildStep: i64,
    /// A PICTURE shown in part (a gallery's `fit=cover`): the part of its
    /// pixels, 0..1, that fills the box. A RECTANGLE's or a picture's corner
    /// radius (`cell { border-radius }`).
    pub hasCrop: bool,
    pub cropX: f64,
    pub cropY: f64,
    pub cropW: f64,
    pub cropH: f64,
    pub radius: f64,
    /// drawn past the page's edge on purpose, into the print bleed: a
    /// full-page picture, its background, an overlay band at the page's foot
    pub bleeds: bool,
    /// a `{container=bubble}` plate's tail: how far it hangs below the
    /// bottom edge, near the left
    pub tail: f64,
    /// a footnote reference's number drawn here (the superscript), 0 for none.
    /// The footnotes themselves are drawn as page furniture (`furniture` 3).
    pub noteRef: i64,
    /// set in a corner of the slide (`{float=top-right}`), out of the flow:
    /// a slide's content centred later does not move it
    pub pinned: bool,
    /// a text box: the right edge of the column it was set in (0: not known)
    pub colRight: f64,
    /// AN EFFECT (kind 6, `{fx=…}` under a block): `embedKind` its name, these
    /// its `fx-<name>=<number>` parameters
    pub fxNames: Vec<String>,
    pub fxValues: Vec<f64>,
}

impl MdBox {
    pub fn new() -> Self {
        Self {
            kind: 0,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            text: String::new(),
            family: "Open Sans".to_string(),
            size: 11.0,
            bold: false,
            spacing: 0.0,
            italic: false,
            color: "#1f2328".to_string(),
            fill: String::new(),
            stroke: String::new(),
            strokeWidth: 0.0,
            link: String::new(),
            embedKind: String::new(),
            embedSource: String::new(),
            embedNote: String::new(),
            imagePath: String::new(),
            imageAlt: String::new(),
            imageNaturalW: 0,
            imageNaturalH: 0,
            page: 0,
            srcStart: -1,
            segSrc: -1,
            segLen: 0,
            segAtom: false,
            retold: false,
            furniture: 0,
            strike: false,
            quiet: false,
            align: String::new(),
            alignX: 0.0,
            alignW: 0.0,
            buildStep: 0,
            hasCrop: false,
            cropX: 0.0,
            cropY: 0.0,
            cropW: 1.0,
            cropH: 1.0,
            radius: 0.0,
            bleeds: false,
            tail: 0.0,
            noteRef: 0,
            pinned: false,
            colRight: 0.0,
            fxNames: Vec::new(),
            fxValues: Vec::new(),
        }
    }
    pub fn textBox(t: String, x: f64, y: f64, w: f64, h: f64) -> Rc<RefCell<MdBox>> {
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 0;
        b.borrow_mut().text = t.clone();
        b.borrow_mut().x = x;
        b.borrow_mut().y = y;
        b.borrow_mut().w = w;
        b.borrow_mut().h = h;
        b
    }
    pub fn rect(x: f64, y: f64, w: f64, h: f64, fill: String) -> Rc<RefCell<MdBox>> {
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 1;
        b.borrow_mut().x = x;
        b.borrow_mut().y = y;
        b.borrow_mut().w = w;
        b.borrow_mut().h = h;
        b.borrow_mut().fill = fill.clone();
        b
    }
    pub fn outline(x: f64, y: f64, w: f64, h: f64, stroke: String, width: f64) -> Rc<RefCell<MdBox>> {
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 2;
        b.borrow_mut().x = x;
        b.borrow_mut().y = y;
        b.borrow_mut().w = w;
        b.borrow_mut().h = h;
        b.borrow_mut().stroke = stroke.clone();
        b.borrow_mut().strokeWidth = width;
        b
    }
    pub fn copy(&self) -> Rc<RefCell<MdBox>> {
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = self.kind;
        b.borrow_mut().x = self.x;
        b.borrow_mut().y = self.y;
        b.borrow_mut().w = self.w;
        b.borrow_mut().h = self.h;
        b.borrow_mut().text = self.text.clone();
        b.borrow_mut().family = self.family.clone();
        b.borrow_mut().size = self.size;
        b.borrow_mut().bold = self.bold;
        b.borrow_mut().italic = self.italic;
        b.borrow_mut().color = self.color.clone();
        b.borrow_mut().fill = self.fill.clone();
        b.borrow_mut().stroke = self.stroke.clone();
        b.borrow_mut().strokeWidth = self.strokeWidth;
        b.borrow_mut().link = self.link.clone();
        b.borrow_mut().embedKind = self.embedKind.clone();
        b.borrow_mut().embedSource = self.embedSource.clone();
        b.borrow_mut().imagePath = self.imagePath.clone();
        b.borrow_mut().imageAlt = self.imageAlt.clone();
        b.borrow_mut().imageNaturalW = self.imageNaturalW;
        b.borrow_mut().imageNaturalH = self.imageNaturalH;
        b.borrow_mut().page = self.page;
        b.borrow_mut().srcStart = self.srcStart;
        b.borrow_mut().segSrc = self.segSrc;
        b.borrow_mut().segLen = self.segLen;
        b.borrow_mut().segAtom = self.segAtom;
        b.borrow_mut().retold = self.retold;
        b.borrow_mut().furniture = self.furniture;
        b.borrow_mut().strike = self.strike;
        b.borrow_mut().quiet = self.quiet;
        b.borrow_mut().align = self.align.clone();
        b.borrow_mut().alignX = self.alignX;
        b.borrow_mut().alignW = self.alignW;
        b.borrow_mut().buildStep = self.buildStep;
        b.borrow_mut().hasCrop = self.hasCrop;
        b.borrow_mut().cropX = self.cropX;
        b.borrow_mut().cropY = self.cropY;
        b.borrow_mut().cropW = self.cropW;
        b.borrow_mut().cropH = self.cropH;
        b.borrow_mut().radius = self.radius;
        b.borrow_mut().bleeds = self.bleeds;
        b.borrow_mut().tail = self.tail;
        b.borrow_mut().noteRef = self.noteRef;
        b.borrow_mut().pinned = self.pinned;
        b.borrow_mut().colRight = self.colRight;
        b.borrow_mut().fxNames = self.fxNames.clone();
        b.borrow_mut().fxValues = self.fxValues.clone();
        b
    }
}

/// What the continuation slides said again at their tops (`MdLayout.contPage`
/// and its four companions), kept while a slide is laid out again on trial.
/// A trial that runs over notes the slide after it as a continuation of this
/// one; a trial that is then thrown away has to take that note with it, or the
/// next slide, laid out again later, says this one's title over its own.
#[derive(Clone)]
pub struct MdContinuations {
    pub page: Vec<i64>,
    pub title: Vec<String>,
    pub titleLevel: Vec<i64>,
    pub section: Vec<String>,
    pub sectionLevel: Vec<i64>,
}

impl MdContinuations {
    pub fn new() -> Self {
        Self {
            page: Vec::new(),
            title: Vec::new(),
            titleLevel: Vec::new(),
            section: Vec::new(),
            sectionLevel: Vec::new(),
        }
    }
    pub fn ints(from: &[i64]) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        for v in from.iter().copied() {
            out.push(v);
        }
        out
    }
    pub fn strs(from: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for v in from.iter().cloned() {
            out.push(v.clone());
        }
        out
    }
}

pub struct MdLayout {
    pub style: Rc<RefCell<MdStyle>>,
    /// while a code line outside the `lines=` marks is laid: its text fades
    pub codeDim: bool,
    /// While a code block is laid: its node, and where the text being laid
    /// starts in its literal (-1: nowhere), so each box knows its source
    /// (`segSrc`) and an editor's caret can stop in code too.
    pub codeNode: Option<Rc<RefCell<MdNode>>>,
    pub codeAt: i64,
    /// where the token being laid starts in its line's code
    pub codeK: i64,
    /// Zero-width boxes where a line has no text (an empty code line), for
    /// an editor's caret to stop on. Off for print and exports.
    pub caretStops: bool,
    /// With caretStops: source offsets of empty lines an editor shows between
    /// the document's blocks (an empty paragraph the writer made with Enter),
    /// in order. Each is a line of its own with a stop on it. The host says
    /// which blank lines are such (Markdown has no empty paragraph).
    pub blankStops: Vec<i64>,
    /// Built once per layout by `buildBoxIndex`: the first box of each block,
    /// by that block's source offset.
    pub boxIndexAt: HashMap<String,i64>,
    pub boxIndexReady: bool,
    /// Built on the first `boxesOnPage` after a layout: the boxes' indexes in
    /// page order (box order within a page), and where each page's run starts
    /// in it. A host that asks for every page's boxes then walks each box
    /// once, not once per page (seconds for a 200-slide deck). A host that
    /// moves a box to another page after the layout calls `forgetPages`.
    pub pageOrder: Vec<i64>,
    pub pageFrom: Vec<i64>,
    pub pageIndexReady: bool,
    pub pageIndexBoxes: i64,
    /// --- the per-block layout cache -------------------------------------------
    ///
    /// A keystroke changes ONE block and the other four hundred are laid out
    /// again for nothing. `markdown:bench` puts this repository's README at
    /// 22.7 ms to parse and 117.5 ms to lay out, so the parse is not the
    /// problem: measuring text is. The parse stays whole — it is cheap, and an
    /// incremental block parser is a second parser to keep in step with the
    /// first — and the LAYOUT is what remembers.
    ///
    /// A block's boxes are kept under the exact source text that produced them.
    /// An edit changes that text for one block and no other, so every other
    /// block hits, and a hit is an array copy with two numbers added to it: the
    /// y it now sits at, and how far its source moved.
    ///
    /// WHAT IT DOES NOT DO, stated rather than discovered:
    ///   * paged layout. A block's boxes there depend on where the page break
    ///     falls, which is not a property of the block. `armCache` refuses.
    ///   * documents with a table of contents. That is a two-pass layout whose
    ///     first pass exists to move the second, and a cache would serve the
    ///     first pass's answers to the second.
    ///   * anything below a top-level block. A list is one entry, not one per
    ///     item: the win is in the hundreds of blocks a long document has, and
    ///     the bookkeeping for nesting buys nothing on top of that.
    pub cacheOn: bool,
    pub cacheKey: Vec<String>,
    /// …and the same keys as a map. A scan over the keys is O(blocks) per
    /// block, which is O(blocks²) per layout: fine for the thirteen a fixture
    /// has and 1.1 million string compares for the thousand in ISSUES.md,
    /// where it cost more than the layout it was saving.
    pub cacheAt: HashMap<String,i64>,
    pub cacheSrc: Vec<i64>,
    pub cacheTop: Vec<f64>,
    pub cacheHeight: Vec<f64>,
    pub cacheFrom: Vec<i64>,
    pub cacheCount: Vec<i64>,
    pub cacheBoxes: Vec<Rc<RefCell<MdBox>>>,
    pub cacheHits: i64,
    pub cacheMisses: i64,
    /// Lay every block out BOTH ways and compare. A cache that disagrees with
    /// the layout it is caching shows a stale picture, which is the worst thing
    /// this feature can do and the one failure a screenshot cannot catch — so
    /// there is a mode that checks rather than trusts, and the edit suite runs
    /// with it on.
    pub cacheAudit: bool,
    pub cacheFaults: i64,
    /// How many blocks the audit actually compared. Without it a suite that
    /// never reached the cache at all reports a clean audit, which is the
    /// §6 trap: a check that counts the wrong noun says the failure is fine.
    pub cacheAudited: i64,
    pub cacheFirstFault: String,
    /// The text the tree was parsed from. The cache keys off each block's slice
    /// of it, so the layout has to be told: a tree does not carry the string it
    /// came from, only the offsets into it.
    pub docSource: String,
    /// The width every cached entry was measured at. A different one makes all
    /// of them wrong, and the cheap total answer is to throw the lot away.
    pub cacheWidth: f64,
    /// …and the style the cached boxes were measured with. A block's boxes are
    /// a function of its source AND of the style, and only the first was ever
    /// in the key — so switching the company template replayed boxes measured
    /// at the previous font size and drew the new one on top of them.
    pub cacheStyleKey: String,
    /// …and how the document was READ: the typographer, the quotes its
    /// language takes, the abbreviations it defines. Those are document-wide —
    /// `*[HTML]: …` at the end turns every earlier "HTML" into an abbr, and
    /// `lang: fi` changes every quote — so a block's source alone does not
    /// say what its inlines are (`MdNode.readingSig`).
    pub cacheReading: String,
    /// ----------------------------------------------------------------------
    /// The company template
    /// ----------------------------------------------------------------------
    ///
    /// Two questions, and they are different. `MdCss.applyTo(style)` answered
    /// the first one — how is the DOCUMENT set — before this layout began, and
    /// the answer is in `style` above. This is the second: what does the sheet
    /// say about THIS BLOCK, given its `{.classes}` and its `{#id}`.
    ///
    /// Only the layout can ask it, because only the layout is walking the
    /// blocks. It is asked once per block and the answer is thrown away, which
    /// is why the sheet is held here rather than resolved into the tree: the
    /// AST says what the document IS and must not learn what it looks like.
    pub sheet: Rc<RefCell<MdCss>>,
    pub sheetText: String,
    /// ----------------------------------------------------------------------
    /// Slides
    /// ----------------------------------------------------------------------
    ///
    /// A slide is a page with rules about where it breaks. `PLAN_SLIDES.md` §4,
    /// in this order, and the order is the whole design:
    ///
    ///   1  MANDATORY   `{.slide}`, or a heading at or above `splitLevel`
    ///   2  FORBIDDEN   `{.no-break}`
    ///   3  PREFERRED   any other heading, when the section it opens does not
    ///                  fit in the room left and does fit on a slide of its own
    ///   4  FALLBACK    overflow — which is what pagination already does
    ///
    /// …and one constraint that is not a break rule and is the thing that makes
    /// an automatic deck look hand-made: KEEP-WITH-NEXT, a heading is never the
    /// last thing on a slide.
    ///
    /// A slide that breaks goes on on a CONTINUATION, which says the slide's
    /// title again at its top — always, whatever it begins with — and, when it
    /// begins inside a section, that section's sub-heading under the title
    /// (`emitSlideTitle`).
    pub slides: bool,
    /// `#` only, at 1. `2` means `#` and `##` each start a slide.
    pub splitLevel: i64,
    /// "split" | "shrink" | "clip" — what a slide too full of one block does.
    /// Only `split` is honoured; the other two are carried and reported, which
    /// is the rule the rest of the template binding is written to.
    pub slideOverflow: String,
    /// How many times a BLOCK had to be cut across two slides. Printed rather
    /// than hidden: `book_slides.rgr` set that precedent, and a deck that
    /// silently drops the third bullet is worse than one that says it did.
    pub slideOverflows: i64,
    /// Sheets whose heading said `{header=off}`, `{footer=off}` or
    /// `{furniture=off}`: the edge is left off that sheet.
    pub headerOff: Vec<i64>,
    pub footerOff: Vec<i64>,
    /// A SLIDE'S OWN DATA, from the keys on its heading the engine does not
    /// know — `## Revenue {jira=ACME-412}` (MdSlideMeta). Three arrays rather
    /// than a map per sheet, because a trial layout has to be able to undo
    /// exactly what it added, and `removeLast` is how the rest of this file
    /// does that. It is drawn nowhere by itself; `{jira}` in a header or a
    /// footer is filled in from the sheet it is on, and a host reads the rest
    /// back per slide.
    pub metaPage: Vec<i64>,
    pub metaKey: Vec<String>,
    pub metaVal: Vec<String>,
    /// "2|transtion|transition": a key one character from an engine key, the
    /// sheet it is on, for the report
    pub metaSuspect: Vec<String>,
    /// THE DECK'S OWN DATA, from the front matter keys the engine does not
    /// read (`jira: ACME-400`). A slide's own value goes over it.
    pub deckMeta: Rc<RefCell<MdSlideMeta>>,
    /// Values a host sets while the deck runs (a program on a slide said
    /// `deck.set("score", 1200)`): they win over the slide's and the deck's
    /// own for `{score}`, and are kept across layouts until the host clears
    /// them.
    pub liveKeys: Vec<String>,
    pub liveVals: Vec<String>,
    /// GROW: a slide with room to spare is laid out again with everything on
    /// it larger — text, headings, spacing, together, so the title stays the
    /// larger thing — by at most this factor (1.0 = off). A deck has no font
    /// size of its own to ask for; a slide of three bullets at the document's
    /// body size is a slide nobody at the back can read. It never shrinks and
    /// never makes a slide spill: when the larger version does not fit, the
    /// slide stays as it was. Slides with a picture or an embed are left
    /// alone, since those were sized for the room they had.
    pub slideGrow: f64,
    /// `mode: book` and the page margins of the front matter (MdBook): a
    /// book's right-hand pages are laid out with the inside margin on the left
    pub book: Rc<RefCell<MdBook>>,
    /// How full a grown slide may get, of the room under the margins.
    pub slideGrowFill: f64,
    /// a grown slide being tried: a chart asks for no other room meanwhile
    pub growTrial: bool,
    /// Per top-level block: the slide it starts on and the one it ends on.
    pub blockFirstPage: Vec<i64>,
    pub blockLastPage: Vec<i64>,
    /// How many slides were grown, and by how much the last one was.
    pub slidesGrown: i64,
    /// slides set a little smaller so that a short spill fits (see shrinkSlide)
    pub slidesShrunk: i64,
    /// The title of the slide being laid out, repeated at the top of a
    /// continuation slide so a reader knows what they are still looking at.
    pub slideTitle: String,
    pub slideTitleLevel: i64,
    /// `{heading=hidden}` on the slide's title: it names the slide and is
    /// not drawn, here or on a continuation slide
    pub slideTitleHidden: bool,
    /// The SECTION the layout is in: the last sub-heading (deeper than
    /// `splitLevel`) under the slide's title, until the next heading. A slide
    /// a block spills onto from inside a section says that heading again
    /// under the title — the reader of the second half of "Footnotes" is
    /// told it is "Footnotes", not only "Plugins". Empty between a title and
    /// its first sub-heading, and while a heading is itself being laid out,
    /// so a continuation that begins WITH a sub-heading has the title alone
    /// over it rather than the previous section's heading.
    pub sectionTitle: String,
    pub sectionLevel: i64,
    pub sectionHidden: bool,
    /// What each continuation slide said again at its top, by slide: a slide
    /// laid out again to shrink or grow it (`shrinkSlide`, `growSlide`) says
    /// it again at the new size instead of losing it. Hidden ones are "".
    pub contPage: Vec<i64>,
    pub contTitle: Vec<String>,
    pub contTitleLevel: Vec<i64>,
    pub contSection: Vec<String>,
    pub contSectionLevel: Vec<i64>,
    /// HIDDEN HEADINGS (`## Kesä {heading=hidden}`): the page each one names
    /// and where it is in the source. They draw no box, so a host that finds
    /// a block's slide by its boxes asks these (`pageOfHidden`).
    pub hiddenSrc: Vec<i64>,
    pub hiddenPage: Vec<i64>,
    /// pages named without a heading (fullPageName)
    pub namedPage: Vec<i64>,
    pub namedText: Vec<String>,
    /// the page a hidden heading opened, so a second heading straight after
    /// it still starts a slide of its own
    pub hiddenOnPage: i64,
    /// OVERLAY PICTURES: files the host draws over the page itself (a drawing
    /// made on a slide), referenced as a picture is (`![](drawings/a.ink)`)
    /// so that deleting the line deletes the drawing. A picture whose file's
    /// content type is in `overlayTypes` (the host says which) takes no room
    /// and draws nothing here; the page it is on and where it is written are
    /// noted (`overlaySrc`, `overlayPage`, `overlayPath`, one per reference)
    /// for the host to draw it.
    pub overlayTypes: Vec<String>,
    pub overlaySrc: Vec<i64>,
    pub overlayPage: Vec<i64>,
    pub overlayPath: Vec<String>,
    /// a `layout=full` gallery's last picture took its whole slide
    pub fullPageDone: bool,
    /// the medium the sheet is read for ("screen" | "print", see
    /// `MdCss.media`); set before `setStyleSheet`
    pub media: String,
    engine: Rc<RefCell<EVGTextEngine>>,
    pub boxes: Vec<Rc<RefCell<MdBox>>>,
    /// the cursor
    pub cursorY: f64,
    pub page: i64,
    pub pageCount: i64,
    /// How many block quotes deep the walk is, which is what `baseRun` reads to
    /// decide the colour of ordinary text.
    pub quoteDepth: i64,
    /// Inside an item of a TIGHT list, where a paragraph is not a paragraph.
    pub inTightItem: bool,
    /// How many lists deep. Only the outermost one is separated from what
    /// follows it — a nested list is followed by its parent's next item, not by
    /// a new block.
    pub listDepth: i64,
    /// the column being filled
    pub contentX: f64,
    pub contentW: f64,
    pub contentTop: f64,
    pub contentBottom: f64,
    /// the column's width with nothing at the slide's side (see noteAside)
    pub fullContentW: f64,
    /// A strip at the right of a slide kept for something the host draws
    /// there (a presenter figure and its speech bubble): `asideAt` are the
    /// source offsets of the blocks that want it, `asideFrac` its width as a
    /// share of the page's width. A slide whose source holds one of the offsets lays its body out
    /// that much narrower; its title keeps the full width, since the strip
    /// is below it.
    pub asideAt: Vec<i64>,
    pub asideFrac: f64,
    pub slideAside: bool,
    /// 0 means one page as tall as the document — the screen's mode.
    pub paginate: bool,
    /// What a handler prepared for the fenced diagrams in this document. Empty
    /// unless somebody filled it: the layout can lay a document out without
    /// one, and leaves a labelled slot where a diagram would have gone.
    pub embeds: Rc<RefCell<MdEmbedCache>>,
    /// left, center or right: the alignment an HTML wrapper asked of the
    /// paragraphs and headings inside it ("" is the default, left)
    pub align: String,
    pub alignStack: Vec<String>,
    /// formulas, laid out once per source (see `mathBox`)
    mathBoxes: HashMap<String,Rc<RefCell<MathBox>>>,
    /// Where a picture's bytes come from, and which folder the document is in.
    ///
    /// Empty by default and that is the honest default: `![alt](src)` draws as
    /// its ALT TEXT when nobody has the bytes, which is the most truthful thing
    /// to draw, and is what this did for every document before there was a byte
    /// store at all.
    images: Rc<RefCell<Vfs>>,
    pub haveImages: bool,
    pub docDir: String,
    /// Pictures the document names and this layout could not find. Counted
    /// rather than hidden, like every other cost in this file.
    pub missingImages: i64,
    /// which ones, so a picture met by a measuring pass and again by the pass
    /// that places it is one picture, and a new layout starts from none
    pub missingKeys: Vec<String>,
    /// HTML the document has that is not drawn as written: a tag nobody
    /// draws, a style property nothing reads, a script handler, a link that
    /// would run script. Each said once, with where in the source it was
    /// first met, for a report to name (the MCP server's warnings).
    pub htmlNotes: Vec<String>,
    pub htmlNoteSrc: Vec<i64>,
    /// The height the document turned out to be, in continuous mode.
    pub documentHeight: f64,
    /// The block being laid out, so every box it emits can say where in the
    /// source it came from.
    pub currentSrc: i64,
    /// The headings this document has, and the page each landed on. Collected
    /// while laying out; used by the table of contents on the pass after.
    pub headText: Vec<String>,
    pub headLevel: Vec<i64>,
    pub headPage: Vec<i64>,
    pub headY: Vec<f64>,
    /// What a previous pass learned, so the table of contents can print page
    /// numbers that are right. All three, not just the pages: on the second
    /// pass the live tables above are still being filled in AS the contents
    /// print, so reading them there lists whatever has been seen so far —
    /// which is one heading.
    pub knownPages: Vec<i64>,
    pub knownText: Vec<String>,
    pub knownLevel: Vec<i64>,
    pub tocLines: i64,
    /// ----------------------------------------------------------------------
    /// A title too big for its slide
    /// ----------------------------------------------------------------------
    ///
    /// A heading set large enough wraps onto two or three lines, and whatever
    /// is under it no longer fits. A diagram or a chart cannot be cut, so it
    /// went to a slide of its own with the title said again, and the slide it
    /// left held the title and nothing else: one heading, two slides. A title
    /// with something under it on its slide now takes at most `slideTitleMax`
    /// of the room, laid out a step smaller until it does, and the block under
    /// it is given what is left (`slideTitleOnly`, read by `embed` and
    /// `pictureBlock`). A title with nothing under it — a section slide — keeps
    /// the size it was given.
    pub slideTitleMax: f64,
    /// On a slide, how tall the blocks after a lone picture are, down to the
    /// slide's end: `pictureBlock` leaves them that room (see `heightAfter`).
    pub followH: f64,
    /// …and whether it can go beside the picture (startFloat), not under it
    pub followBeside: bool,
    /// FLOAT: a picture or a drawing narrower than the column, with `{width=…}`
    /// under it, has the blocks after it set beside it, in the rest of the
    /// column, down to its bottom (`startFloat`). While on: the page it is on,
    /// where it ends, and the column the text beside it gets.
    pub floatOn: bool,
    pub floatPage: i64,
    pub floatBottom: f64,
    pub floatX: f64,
    pub floatW: f64,
    /// A picture pinned in a BOTTOM corner (pinPicture): the slide it is on,
    /// the top of the room it keeps (its top less a gap), its left and right
    /// edge, and the column beside it. A block after it that would reach down
    /// into that room is set in the column beside it instead (placeBlock).
    pub cornerOn: bool,
    pub cornerPage: i64,
    pub cornerTop: f64,
    pub cornerLeft: f64,
    pub cornerRight: f64,
    pub cornerX: f64,
    pub cornerW: f64,
    /// The slide so far holds its title and nothing else.
    pub slideTitleOnly: bool,
    /// inside a `{container=…}` plate: a block in it does not get one of its own
    pub containerDepth: i64,
    /// the padding at the top of the plates and quotes the cursor is inside,
    /// added under the title said again when one of them runs onto the next
    /// slide (emitSlideTitle): their text starts inside them there too
    pub openPad: f64,
    /// the room over and under the text of the last block put on a plate
    /// (its padding and a bubble's tail), and that block's source offset:
    /// `fitSlideTitle` sets the text smaller, which makes no padding smaller
    pub plateRoom: f64,
    pub plateRoomSrc: i64,
    /// the plate the block being laid out stands on ("" for none) and the
    /// page under it: a heading's own colour is judged on it, not only the
    /// text's (`## Title {container=box}` kept its light theme colour on a
    /// light plate)
    pub plateFill: String,
    pub platePaper: String,
    /// The typography the block being laid out inherits and adds to
    /// (`MdTypo`: weight, style, transform, spacing), and a face its class
    /// names (`.serif { font-family }`), "" for the element's own
    pub typo: Rc<RefCell<MdTypo>>,
    pub typoDepth: i64,
    pub blockFamily: String,
    /// The headings the block being laid out is under, by level (1…6), as
    /// the sheet sees them (`MdCss.scopeOf`): `#intro h2`, `.dark page`. The
    /// style's look as the sheet left it is kept in `baseLook`, and a slide's
    /// own rules are laid over it while that slide is laid out (`enterScope`).
    pub scopeHeads: Vec<Rc<RefCell<MdNode>>>,
    scopeCtx: Option<Rc<RefCell<CssNode>>>,
    pub baseLook: Vec<String>,
    pub scoped: bool,
    /// the page's colour and picture from the page a scope began on, when a
    /// slide's rules changed them (`paperOf`, `pictureOf`)
    pub paperFrom: Vec<i64>,
    pub paperColour: Vec<String>,
    pub paperPicture: Vec<String>,
    /// a mark whose page is the one its heading lands on, not yet known
    /// (a slide break starts the new page at the heading's first line)
    pub paperPending: bool,
    /// --- footnotes (see "Footnotes" below) --------------------------------
    /// the document whose `notes` are drawn; empty: none
    pub notesDoc: Rc<RefCell<MdNode>>,
    /// per footnote, the page its first drawn reference is on (-1: not yet)
    pub noteFirstPage: Vec<i64>,
    /// per page, the room its footnotes have claimed at its foot
    pub noteRoom: Vec<f64>,
    /// drawing (or measuring) footnotes: their own references claim nothing
    pub inNotes: bool,
    /// a footnote's height, by number, size and column
    pub noteHeights: HashMap<String,f64>,
    /// how much smaller footnotes are set than the text
    pub noteScale: f64,
    /// Room for `h` more, or a new sheet. Returns the y to draw at.
    /// How far past `contentBottom` a block may end and still count as fitting.
    /// A diagram or picture squeezed into the room left is given exactly
    /// `contentBottom - cursorY`, and `cursorY` plus that comes back one
    /// rounding step over `contentBottom` for some paddings: the block went to
    /// a slide of its own and left its title's slide empty. A hundredth of a
    /// point is far below anything a reader sees.
    pub fitSlack: f64,
    /// ----------------------------------------------------------------------
    /// Blocks side by side (`::: columns`, `{layout=columns}`; MdColumns)
    /// ----------------------------------------------------------------------
    ///
    /// Which block goes in which column and how wide each is is MdColumns'
    /// plan; here the columns are laid out one after the other from the same
    /// top, and the block ends under the longest. A picture or chart in a
    /// column is given the column's room less what is under it in the same
    /// column, as on a slide of its own. On a slide where the columns do not
    /// fit under what is above them, they start the next slide when they fit
    /// there whole.
    ///
    /// while > 0: inside columns (a column that runs onto the next slide puts
    /// the next column's spill under the title said there, not a second one)
    pub columnsOn: i64,
    pub colSpillPage: i64,
    pub colSpillTop: f64,
    /// the srcStart of a block whose `width=` made its column's width: it
    /// fills the column (its own `width=` is read as done)
    pub takenSrc: i64,
    /// ----------------------------------------------------------------------
    /// Where a slide's content sits (`valign=`, `layout=section|statement`;
    /// MdSlideLayout)
    /// ----------------------------------------------------------------------
    ///
    /// Each slide is laid out from the top as before; once it has been
    /// shrunk or grown, `alignSlides` moves its content down to where its
    /// title said. The slides are found by their titles' source (the pages
    /// renumber when a slide is shrunk into the one before it).
    pub deckPlace: String,
    pub placeSrc: Vec<i64>,
    pub placeWord: Vec<String>,
    /// laying out a section or statement slide: lines centred
    pub slideCenter: bool,
    /// …a statement: its text at the title's size
    pub slideStatement: bool,
    /// …a statement that is only its title
    pub slideStatementAlone: bool,
    /// ----------------------------------------------------------------------
    /// Growing a slide that has room
    /// ----------------------------------------------------------------------
    /// ----------------------------------------------------------------------
    /// Shrinking a slide that spills a little
    /// ----------------------------------------------------------------------
    ///
    /// A slide whose last block runs over by a line or an item gets a slide of
    /// its own for that remainder: the title again and one bullet. Or the
    /// last row of a table and the sentence after it: blocks that START on
    /// the continuation and end there are part of the spill too, as long as
    /// none of them would have started a slide of its own anyway.
    /// When the spill is short — at most `slideShrinkSpill` of a slide — and
    /// that continuation holds nothing else, the slide is laid out again a step
    /// smaller, down to `slideShrinkMin`, and kept at the first size where it
    /// fits. The continuation goes, and every later slide moves up one.
    pub slideShrinkMin: f64,
    pub slideShrinkSpill: f64,
    /// A TABLE that runs over is set smaller by itself — its text and cell
    /// padding, the title and the lead above it as they were — and further:
    /// ten rows on a dark theme's 20pt body spilled half a table, where 14pt
    /// holds them on one slide. Down to `tableShrinkMin` of its size, for a
    /// spill of at most `tableShrinkSpill` of a slide.
    pub tableShrinkMin: f64,
    pub tableShrinkSpill: f64,
    /// ----------------------------------------------------------------------
    /// A table cut across slides
    /// ----------------------------------------------------------------------
    ///
    /// Which slide each row group goes to is `MdTableSplit`'s answer, from
    /// heights alone; the room it is given is what is left here and what a
    /// continuation slide has under the title said again. Rows are spread
    /// evenly on a deck, filled to the bottom on a printed page.
    ///
    /// Per slide: how much more of a balanced table sits on it (its repeated
    /// header, and the rows balancing moved over) than filling the slide
    /// before it would have left there. The slide shrinker
    /// judges a spill by what it would have been (`shrinkSlide`).
    pub spillExcess: Vec<f64>,
    /// ----------------------------------------------------------------------
    /// A gallery (```gallery, MdGallery)
    /// ----------------------------------------------------------------------
    ///
    /// `layout=grid`: the cells in rows across the column, each row as tall
    /// as the slide allows for as many rows as keep a row at least
    /// `galleryMinRow` high; the rows that do not fit go on to the next slide
    /// (with its title again, as any block that spills). On a document page
    /// a row is two thirds of a column's width high. `layout=full`: one
    /// picture a slide, edge to edge.
    pub galleryMinRow: f64,
}

impl MdLayout {
    pub fn new() -> Self {
        Self {
            style: Rc::new(RefCell::new(MdStyle::new())),
            codeDim: false,
            codeNode: None,
            codeAt: -1,
            codeK: 0,
            caretStops: false,
            blankStops: Vec::new(),
            boxIndexAt: HashMap::new(),
            boxIndexReady: false,
            pageOrder: Vec::new(),
            pageFrom: Vec::new(),
            pageIndexReady: false,
            pageIndexBoxes: -1,
            cacheOn: false,
            cacheKey: Vec::new(),
            cacheAt: HashMap::new(),
            cacheSrc: Vec::new(),
            cacheTop: Vec::new(),
            cacheHeight: Vec::new(),
            cacheFrom: Vec::new(),
            cacheCount: Vec::new(),
            cacheBoxes: Vec::new(),
            cacheHits: 0,
            cacheMisses: 0,
            cacheAudit: false,
            cacheFaults: 0,
            cacheAudited: 0,
            cacheFirstFault: String::new(),
            docSource: String::new(),
            cacheWidth: 0.0 - 1.0,
            cacheStyleKey: String::new(),
            cacheReading: String::new(),
            sheet: Rc::new(RefCell::new(MdCss::new())),
            sheetText: String::new(),
            slides: false,
            splitLevel: 1,
            slideOverflow: "split".to_string(),
            slideOverflows: 0,
            headerOff: Vec::new(),
            footerOff: Vec::new(),
            metaPage: Vec::new(),
            metaKey: Vec::new(),
            metaVal: Vec::new(),
            metaSuspect: Vec::new(),
            deckMeta: Rc::new(RefCell::new(MdSlideMeta::new())),
            liveKeys: Vec::new(),
            liveVals: Vec::new(),
            slideGrow: 1.0,
            book: Rc::new(RefCell::new(MdBook::new())),
            slideGrowFill: 0.82,
            growTrial: false,
            blockFirstPage: Vec::new(),
            blockLastPage: Vec::new(),
            slidesGrown: 0,
            slidesShrunk: 0,
            slideTitle: String::new(),
            slideTitleLevel: 0,
            slideTitleHidden: false,
            sectionTitle: String::new(),
            sectionLevel: 0,
            sectionHidden: false,
            contPage: Vec::new(),
            contTitle: Vec::new(),
            contTitleLevel: Vec::new(),
            contSection: Vec::new(),
            contSectionLevel: Vec::new(),
            hiddenSrc: Vec::new(),
            hiddenPage: Vec::new(),
            namedPage: Vec::new(),
            namedText: Vec::new(),
            hiddenOnPage: -1,
            overlayTypes: Vec::new(),
            overlaySrc: Vec::new(),
            overlayPage: Vec::new(),
            overlayPath: Vec::new(),
            fullPageDone: false,
            media: "screen".to_string(),
            engine: Rc::new(RefCell::new(EVGTextEngine::new())),
            boxes: Vec::new(),
            cursorY: 0.0,
            page: 0,
            pageCount: 1,
            quoteDepth: 0,
            inTightItem: false,
            listDepth: 0,
            contentX: 0.0,
            contentW: 0.0,
            contentTop: 0.0,
            contentBottom: 0.0,
            fullContentW: 0.0,
            asideAt: Vec::new(),
            asideFrac: 0.0,
            slideAside: false,
            paginate: false,
            embeds: Rc::new(RefCell::new(MdEmbedCache::new())),
            align: String::new(),
            alignStack: Vec::new(),
            mathBoxes: HashMap::new(),
            images: Rc::new(RefCell::new(Vfs::new())),
            haveImages: false,
            docDir: "/".to_string(),
            missingImages: 0,
            missingKeys: Vec::new(),
            htmlNotes: Vec::new(),
            htmlNoteSrc: Vec::new(),
            documentHeight: 0.0,
            currentSrc: -1,
            headText: Vec::new(),
            headLevel: Vec::new(),
            headPage: Vec::new(),
            headY: Vec::new(),
            knownPages: Vec::new(),
            knownText: Vec::new(),
            knownLevel: Vec::new(),
            tocLines: 0,
            slideTitleMax: 0.6,
            followH: 0.0,
            followBeside: false,
            floatOn: false,
            floatPage: 0,
            floatBottom: 0.0,
            floatX: 0.0,
            floatW: 0.0,
            cornerOn: false,
            cornerPage: 0,
            cornerTop: 0.0,
            cornerLeft: 0.0,
            cornerRight: 0.0,
            cornerX: 0.0,
            cornerW: 0.0,
            slideTitleOnly: false,
            containerDepth: 0,
            openPad: 0.0,
            plateRoom: 0.0,
            plateRoomSrc: -1,
            plateFill: String::new(),
            platePaper: String::new(),
            typo: Rc::new(RefCell::new(MdTypo::new())),
            typoDepth: 0,
            blockFamily: String::new(),
            scopeHeads: Vec::new(),
            scopeCtx: None,
            baseLook: Vec::new(),
            scoped: false,
            paperFrom: Vec::new(),
            paperColour: Vec::new(),
            paperPicture: Vec::new(),
            paperPending: false,
            notesDoc: Rc::new(RefCell::new(MdNode::new())),
            noteFirstPage: Vec::new(),
            noteRoom: Vec::new(),
            inNotes: false,
            noteHeights: HashMap::new(),
            noteScale: 0.78,
            fitSlack: 0.01,
            columnsOn: 0,
            colSpillPage: -1,
            colSpillTop: 0.0,
            takenSrc: -2,
            deckPlace: String::new(),
            placeSrc: Vec::new(),
            placeWord: Vec::new(),
            slideCenter: false,
            slideStatement: false,
            slideStatementAlone: false,
            slideShrinkMin: 0.8,
            slideShrinkSpill: 0.3,
            tableShrinkMin: 0.6,
            tableShrinkSpill: 0.8,
            spillExcess: Vec::new(),
            galleryMinRow: 140.0,
        }
    }
    pub fn coversPageWidth(b: &Rc<RefCell<MdBox>>, pw: f64) -> bool {
        return b.borrow().x <= 0.5 && b.borrow().x + b.borrow().w >= pw - 0.5;
    }
    pub fn anySlot(slots: &[String]) -> bool {
        let mut i: i64 = 0;
        while i < (slots.len() as i64) {
            if (MdText::str_len(&slots[i as usize].clone().trim().to_string()) as i64) > 0 {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn skips(spec: &str, p: i64, pages: i64) -> bool {
        let mut words: Vec<String> = MdLayout::replaceAll(spec, ",", " ")
            .split(" ")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        let mut i: i64 = 0;
        while i < (words.len() as i64) {
            let w: String = words[i as usize].clone().trim().to_string();
            if w == "first" && p == 0 {
                return true;
            }
            if w == "last" && p == pages - 1 {
                return true;
            }
            if !w.is_empty() {
                let n: Option<i64> = w.parse::<i64>().ok();
                if n.is_some() {
                    if n.unwrap() == p + 1 {
                        return true;
                    }
                }
            }
            i += 1;
        }
        false
    }
    pub fn isOff(v: &str) -> bool {
        let t: String = MdChar::lower(&v.trim().to_string());
        ((t == "off" || t == "none") || t == "false") || t == "no"
    }
    /// `![alt](path)` in a place: the picture's path, or "".
    pub fn slotImage(slot: &str) -> String {
        let a: i64 = MdText::index_of(&slot, "![");
        if a < 0 {
            return String::new();
        }
        let m: i64 = MdText::index_of(&slot, "](");
        if m < a {
            return String::new();
        }
        let rest: String = MdText::substr(&slot, m + 2, (MdText::str_len(&slot) as i64));
        let e: i64 = MdText::index_of(&rest, ")");
        if e < 0 {
            return String::new();
        }
        MdText::substr(&rest, 0, e).trim().to_string()
    }
    /// The place without its picture, and whether the picture came first.
    pub fn slotText(slot: &str) -> String {
        let a: i64 = MdText::index_of(&slot, "![");
        let m: i64 = MdText::index_of(&slot, "](");
        if a < 0 || m < a {
            return slot.trim().to_string();
        }
        let rest: String = MdText::substr(&slot, m + 2, (MdText::str_len(&slot) as i64));
        let e: i64 = MdText::index_of(&rest, ")");
        if e < 0 {
            return slot.trim().to_string();
        }
        let before: String = MdText::substr(&slot, 0, a);
        let after: String = MdText::substr(&rest, e + 1, (MdText::str_len(&rest) as i64));
        format!("{} {}", before.trim().to_string(), after.trim().to_string()).trim().to_string()
    }
    pub fn imageFirst(slot: &str) -> bool {
        let a: i64 = MdText::index_of(&slot, "![");
        if a < 0 {
            return true;
        }
        (MdText::str_len(&MdText::substr(&slot, 0, a).trim().to_string()) as i64) == 0
    }
    pub fn contrast(a: &str, b: &str) -> f64 {
        let la: f64 = MdLayout::luminance(a);
        let lb: f64 = MdLayout::luminance(b);
        if la < lb {
            return (lb + 0.05) / (la + 0.05);
        }
        (la + 0.05) / (lb + 0.05)
    }
    pub fn luminance(hex: &str) -> f64 {
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(hex);
        let r: f64 = MdLayout::channel((EVGColor::red(&c)) as f64);
        let g: f64 = MdLayout::channel((EVGColor::green(&c)) as f64);
        let b: f64 = MdLayout::channel((EVGColor::blue(&c)) as f64);
        (0.2126 * r + 0.7152 * g) + 0.0722 * b
    }
    pub fn channel(v: f64) -> f64 {
        let x: f64 = v / 255.0;
        if x <= 0.04045 {
            return x / 12.92;
        }
        // y^2.4, as y^2.375 = y² · y^¼ · y^⅛ (close enough for a threshold)
        let y: f64 = (x + 0.055) / 1.055;
        let q: f64 = y.sqrt().sqrt();
        ((y * y) * q) * q.sqrt()
    }
    pub fn minD(a: f64, b: f64) -> f64 {
        if a < b {
            return a;
        }
        b
    }
    /// A place whose data was missing: `"{jira} · {page}"` on a slide without
    /// a ticket would read " · 1". The separator left hanging goes with it.
    pub fn tidySlot(text: &str) -> String {
        let mut out: String = text.to_string();
        out = MdLayout::replaceAll(&out, "  ", " ");
        out = out.trim().to_string();
        let mut seps: Vec<String> = vec!["·".to_string(), "-".to_string(), "–".to_string(), "|".to_string(), "/".to_string(), ",".to_string()];
        let mut i: i64 = 0;
        while i < (seps.len() as i64) {
            let sep: String = seps[i as usize].clone();
            let n: i64 = MdText::str_len(&sep) as i64;
            if (MdText::str_len(&out) as i64) > n {
                if MdText::substr(&out, 0, n) == sep {
                    out = MdText::substr(&out, n, (MdText::str_len(&out) as i64)).trim().to_string();
                }
            }
            if (MdText::str_len(&out) as i64) > n {
                if MdText::substr(&out, (MdText::str_len(&out) as i64) - n, (MdText::str_len(&out) as i64)) == sep {
                    out = MdText::substr(&out, 0, (MdText::str_len(&out) as i64) - n).trim().to_string();
                }
            }
            i += 1;
        }
        out
    }
    /// {page}, {pages} and {title} in a header or footer.
    pub fn fillTokens(text: &str, page: i64, pages: i64, title: &str) -> String {
        let mut out: String = text.to_string();
        out = MdLayout::replaceAll(&out, "{page}", &page.to_string());
        out = MdLayout::replaceAll(&out, "{pages}", &pages.to_string());
        out = MdLayout::replaceAll(&out, "{title}", title);
        out
    }
    pub fn replaceAll(text: &str, what: &str, with: &str) -> String {
        let mut out: String = String::new();
        let mut rest: String = text.to_string();
        let mut at: i64 = MdText::index_of(&rest, what);
        while at >= 0 {
            out = format!("{}{}{}", out, MdText::substr(&rest, 0, at), with);
            rest = MdText::substr(&rest, at + (MdText::str_len(&what) as i64), (MdText::str_len(&rest) as i64));
            at = MdText::index_of(&rest, what);
        }
        format!("{}{}", out, rest)
    }
    pub fn besideable(node: &Rc<RefCell<MdNode>>, split_level: i64) -> bool {
        let mut __re_gx1: Rc<RefCell<MdNode>> = MdLayout::loneImage(node);
        if node.borrow().kind == "paragraph" && __re_gx1.borrow().kind != "image" {
            return true;
        }
        if node.borrow().kind == "list" {
            return true;
        }
        if node.borrow().kind == "block_quote" {
            return true;
        }
        // a heading starts something new: under the figure, full width
        false
    }
    /// Whether the block after `i` stays on the slide `i` starts, rather than
    /// starting one of its own.
    pub fn bodyFollows(doc: &Rc<RefCell<MdNode>>, i: i64, split_level: i64) -> bool {
        if i + 1 >= doc.borrow().childCount() {
            return false;
        }
        let mut next: Rc<RefCell<MdNode>> = doc.borrow().childAt(i + 1);
        if next.borrow().kind == "front_matter" {
            return false;
        }
        if next.borrow().hasClass("no-break") {
            return true;
        }
        if next.borrow().hasClass("slide") {
            return false;
        }
        if next.borrow().kind == "heading" && next.borrow().level <= split_level {
            return false;
        }
        true
    }
    pub fn countHeadings(doc: &Rc<RefCell<MdNode>>) -> i64 {
        let mut c: i64 = 0;
        let mut i: i64 = 0;
        let n: i64 = doc.borrow().childCount();
        while i < n {
            let mut child: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
            if child.borrow().kind == "heading" {
                if child.borrow().level <= 3 {
                    c += 1;
                }
            }
            i += 1;
        }
        c
    }
    /// `{heading=hidden}` (also `hidden` and `none`) on a heading.
    pub fn hiddenHeading(node: &Rc<RefCell<MdNode>>) -> bool {
        MdAttrs::headingHidden(node)
    }
    /// The footnotes `n` refers to (a note may have notes), not yet placed.
    pub fn notesWithin(node: &Rc<RefCell<MdNode>>, mut first: &mut Vec<i64>, p: i64, mut list: &mut Vec<i64>) {
        if node.borrow().kind == "footnote_ref" {
            let m: i64 = node.borrow().noteNumber;
            if (m >= 1) && (m <= first.len() as i64) {
                if first[(m - 1) as usize] < 0 {
                    first[(m - 1) as usize] = p;
                    list.push(m);
                }
            }
        }
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            MdLayout::notesWithin(&node.borrow().childAt(i), &mut first, p, &mut list);
            i += 1;
        }
    }
    /// The numbers of the footnotes `node` refers to, for the layout cache.
    pub fn noteSig(node: &Rc<RefCell<MdNode>>) -> String {
        if node.borrow().kind == "footnote_ref" {
            return format!("{},", node.borrow().noteNumber.to_string());
        }
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            out = format!("{}{}", out, MdLayout::noteSig(&node.borrow().childAt(i)));
            i += 1;
        }
        out
    }
    /// A colour a sheet gives a property, as `#RRGGBB(AA)`; "" for none.
    fn sheetColour(got: &Rc<RefCell<CssComputed>>, name: &str) -> String {
        if false == CssComputed::has(&got, name) {
            return String::new();
        }
        let mut c: Rc<RefCell<EVGColor>> = CssValue::color(&CssComputed::get(&got, name));
        if false == c.borrow().is_set {
            return String::new();
        }
        MdContainer::hexOf(&c)
    }
    pub fn maxD(a: f64, b: f64) -> f64 {
        if a > b {
            return a;
        }
        b
    }
    /// Whether a rule gives its block a box: a solid background, a border or
    /// padding (a gradient is no colour and gives none).
    fn boxLook(got: &Rc<RefCell<CssComputed>>) -> bool {
        if (MdText::str_len(&MdLayout::sheetColour(got, "background-color")) as i64) > 0 {
            return true;
        }
        if (MdText::str_len(&MdLayout::sheetColour(got, "background")) as i64) > 0 {
            return true;
        }
        if (MdText::str_len(&MdCss::borderColour(got)) as i64) > 0 {
            return true;
        }
        let mut pads: Vec<f64> = MdCss::padding(got);
        for pd in pads.iter().copied() {
            if pd > 0.0 {
                return true;
            }
        }
        false
    }
    /// MdCss.alignWord's answer as the layout's: "center", "right", "left"
    pub fn alignOfWord(w: &str) -> String {
        if w == "center" {
            return "center".to_string();
        }
        if w == "end" {
            return "right".to_string();
        }
        if w == "start" {
            return "left".to_string();
        }
        String::new()
    }
    /// Only a block whose children are themselves blocks. A paragraph in three
    /// columns would be a line flow and is a different feature; saying so is
    /// better than cutting a sentence into thirds by accident.
    pub fn canColumn(kind: &str) -> bool {
        if kind == "list" {
            return true;
        }
        if kind == "block_quote" {
            return true;
        }
        false
    }
    /// where the slide that title `i` opens ends in the source: the next
    /// slide title's start, -1 at the end of the document
    pub fn slideEndSrc(doc: &Rc<RefCell<MdNode>>, i: i64, split_level: i64) -> i64 {
        let mut j: i64 = i + 1;
        let n: i64 = doc.borrow().childCount();
        while j < n {
            let mut c: Rc<RefCell<MdNode>> = doc.borrow().childAt(j);
            if !c.borrow().hasClass("no-break") {
                if c.borrow().hasClass("slide") || c.borrow()
                    .kind == "heading" && c.borrow()
                    .level <= split_level {
                    return c.borrow().srcStart;
                }
            }
            j += 1;
        }
        -1
    }
    pub fn wordCount(t: &str) -> i64 {
        let mut n: i64 = 0;
        let mut parts: Vec<String> = t.trim()
            .to_string()
            .split(" ")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        for w in parts.iter().cloned() {
            if !w.trim().to_string().is_empty() {
                n += 1;
            }
        }
        n
    }
    /// How many lines of heading-sized text boxes from..to hold: distinct
    /// baselines among text larger than `minSize`.
    pub fn headingLines(list: &Vec<Rc<RefCell<MdBox>>>, from: i64, to: i64, min_size: f64) -> i64 {
        let mut ys: Vec<f64> = Vec::new();
        let mut i: i64 = from;
        while i < to {
            let mut b: Rc<RefCell<MdBox>> = list[i as usize].clone();
            if (b.borrow().kind == 0 && b.borrow().size > min_size) && !b.borrow().text.is_empty() {
                let mut seen: bool = false;
                let mut k: i64 = 0;
                while k < (ys.len() as i64) {
                    if ((ys[k as usize] - b.borrow().y).abs()) < 1.0 {
                        seen = true;
                    }
                    k += 1;
                }
                if false == seen {
                    ys.push(b.borrow().y);
                }
            }
            i += 1;
        }
        ys.len() as i64
    }
    /// A level-`level` heading of size `was` on a slide grown by `s`: a title
    /// (level up to `splitLevel`) as it was, any other at most `titleSize`.
    pub fn grownHeading(was: f64, s: f64, level: i64, split_level: i64, title_size: f64) -> f64 {
        if level <= split_level {
            return was;
        }
        let g: f64 = was * s;
        if g > title_size && was <= title_size {
            return title_size;
        }
        if g < was {
            return was;
        }
        g
    }
    /// The direct child whose source span holds `off`, or -1.
    pub fn itemOf(node: &Rc<RefCell<MdNode>>, kids: i64, off: i64) -> i64 {
        if off < 0 {
            return -1;
        }
        let mut i: i64 = 0;
        while i < kids {
            let mut k: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if k.borrow().srcStart >= 0 {
                if k.borrow().srcStart <= off && off < k.borrow().srcEnd {
                    return i;
                }
            }
            i += 1;
        }
        -1
    }
    /// 0, each wrap, and the end of the drawn text: row r is [r, r+1)
    pub fn rowStarts(ln: &Rc<RefCell<MdCodeLine>>) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        out.push(0);
        let __it_o = ln.borrow().wraps.clone();
        for o in __it_o.iter().copied() {
            out.push(o);
        }
        if ln.borrow().kind == 3 || ln.borrow().kind == 4 {
            out.push((MdText::str_len(&ln.borrow().text) as i64));
        } else {
            out.push((MdText::str_len(&ln.borrow().code) as i64));
        }
        out
    }
    /// a colour t of the way from a to b (a when either does not parse)
    pub fn tint(a: &str, b: &str, t: f64) -> String {
        let mut ca: Rc<RefCell<EVGColor>> = EVGColor::parse(a);
        let mut cb: Rc<RefCell<EVGColor>> = EVGColor::parse(b);
        if !ca.borrow().is_set || !cb.borrow().is_set {
            return a.to_string();
        }
        let r: f64 = ((EVGColor::red(&ca)) as f64) + (((EVGColor::red(&cb)) as f64) - ((EVGColor::red(&ca)) as f64)) * t;
        let g: f64 = ((EVGColor::green(&ca)) as f64) + (((EVGColor::green(&cb)) as f64) - ((EVGColor::green(&ca)) as f64)) * t;
        let bl: f64 = ((EVGColor::blue(&ca)) as f64) + (((EVGColor::blue(&cb)) as f64) - ((EVGColor::blue(&ca)) as f64)) * t;
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::create(r, g, bl, 1.0);
        return EVGColor::to_hex_string(&c);
    }
    /// `el`'s box made the largest one of the proportions natW : natH that fits
    /// inside it, centred where it was.
    fn containIn(el: &Rc<RefCell<EVGElement>>, nat_w: f64, nat_h: f64) {
        let x: f64 = el.borrow().left.as_ref().unwrap().borrow().pixels;
        let y: f64 = el.borrow().top.as_ref().unwrap().borrow().pixels;
        let w: f64 = el.borrow().width.as_ref().unwrap().borrow().pixels;
        let h: f64 = el.borrow().height.as_ref().unwrap().borrow().pixels;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let k: f64 = nat_w / nat_h;
        let mut fw: f64 = w;
        let mut fh: f64 = w / k;
        if fh > h {
            fh = h;
            fw = h * k;
        }
        el.borrow_mut().left = Some(EVGUnit::px(x + (w - fw) * 0.5));
        el.borrow_mut().top = Some(EVGUnit::px(y + (h - fh) * 0.5));
        el.borrow_mut().width = Some(EVGUnit::px(fw));
        el.borrow_mut().height = Some(EVGUnit::px(fh));
    }
    /// What stands in front of an item: a bullet, a number, or a task box. One
    /// place, because the measuring pass and the drawing pass have to agree —
    /// they did not have to before, when the column was a constant.
    pub fn markerFor(list: &Rc<RefCell<MdNode>>, item: &Rc<RefCell<MdNode>>, number: i64) -> String {
        if item.borrow().taskState >= 0 {
            return if (item.borrow().taskState == 1) { "☑".to_string() } else { "☐".to_string() }.clone();
        }
        if list.borrow().listOrdered {
            return format!("{}{}", number.to_string(), list.borrow().listDelim);
        }
        "•".to_string()
    }
    pub fn firstWord(info: &str) -> String {
        let n: i64 = MdText::str_len(&info) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&info, i);
            if c == 32 {
                return MdText::substr(&info, 0, i);
            }
            if c == 9 {
                return MdText::substr(&info, 0, i);
            }
            i += 1;
        }
        info.to_string()
    }
    /// `letter-spacing` in points: as written, or (negative, from `em`) a
    /// share of the size
    pub fn spacingFor(spacing: f64, size: f64) -> f64 {
        if spacing < 0.0 {
            return (0.0 - spacing) * size;
        }
        spacing
    }
    pub fn isWrapper(nm: &str) -> bool {
        MdText::index_of(&" div p center section article header footer main aside nav figure figcaption blockquote details summary span h1 h2 h3 h4 h5 h6 a font b strong i em small ".to_string(), &format!(" {} ", nm)) >= 0
    }
    pub fn headingLevelOf(nm: &str) -> i64 {
        if (MdText::str_len(&nm) as i64) != 2 {
            return 0;
        }
        if MdText::char_at(&nm, 0) != 104 {
            return 0;
        }
        let d: i64 = MdText::char_at(&nm, 1) - 48;
        if (d >= 1 && d <= 6) {
            return d;
        }
        0
    }
    pub fn oneLine(s: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&s) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            if c == 10 || c == 13 {
                out = format!("{} ", out);
            } else {
                // the unit itself: `strfromcode` of a `charAt` splits "ä"
                // into two characters on Go, whose strings are bytes
                out = format!("{}{}", out, MdText::substr(&s, i, i + 1));
            }
            i += 1;
        }
        out.trim().to_string()
    }
    pub fn dedent(s: &str) -> String {
        let mut lines: Vec<String> = s.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < (lines.len() as i64) {
            if i > 0 {
                out = format!("{}\n", out);
            }
            let ln: String = lines[i as usize].clone();
            let mut k: i64 = 0;
            while k < (MdText::str_len(&ln) as i64) && (MdText::char_at(&ln, k) == 32 || MdText::char_at(&ln, k) == 9) {
                k += 1;
            }
            out = format!("{}{}", out, MdText::substr(&ln, k, (MdText::str_len(&ln) as i64)));
            i += 1;
        }
        out
    }
    /// every node of `n` placed at start..end of the source
    pub fn pinSrc(n: &Rc<RefCell<MdNode>>, start: i64, end: i64) {
        n.borrow_mut().srcStart = start;
        n.borrow_mut().srcEnd = end;
        let mut i: i64 = 0;
        while i < n.borrow().childCount() {
            MdLayout::pinSrc(&n.borrow().childAt(i), start, end);
            i += 1;
        }
    }
    pub fn remapSrc(n: &Rc<RefCell<MdNode>>, outer: &Rc<RefCell<MdNode>>, base: i64) {
        if n.borrow().srcStart >= 0 {
            let srcStart_v = outer.borrow().srcOfLiteral(base + n.borrow().srcStart);
            n.borrow_mut().srcStart = srcStart_v;
        }
        if n.borrow().srcEnd >= 0 {
            let srcEnd_v = outer.borrow().srcOfLiteral(base + n.borrow().srcEnd);
            n.borrow_mut().srcEnd = srcEnd_v;
        }
        let mut i: i64 = 0;
        while i < n.borrow().childCount() {
            MdLayout::remapSrc(&n.borrow().childAt(i), outer, base);
            i += 1;
        }
    }
    pub fn stripTags(s: &str) -> String {
        let mut out: String = String::new();
        let mut in_tag: bool = false;
        let mut i: i64 = 0;
        while i < (MdText::str_len(&s) as i64) {
            let c: i64 = MdText::char_at(&s, i);
            if c == 60 {
                in_tag = true;
            } else {
                if c == 62 {
                    in_tag = false;
                } else {
                    if !in_tag {
                        if c == 10 {
                            out = format!("{} ", out);
                        } else {
                            out = format!("{}{}", out, MdText::substr(&s, i, i + 1));
                        }
                    }
                }
            }
            i += 1;
        }
        out.trim().to_string()
    }
    /// The markdown table's width algorithm over given min- and max-content
    /// widths: as written when it fits, else each column at least its minimum
    /// and the room left shared in proportion to what each still wants.
    pub fn shareWidths(natural: &[f64], minimum: &[f64], w: f64) -> Vec<f64> {
        let mut widths: Vec<f64> = Vec::new();
        let cols: i64 = natural.len() as i64;
        let mut total: f64 = 0.0;
        let mut least_total: f64 = 0.0;
        let mut k: i64 = 0;
        while k < cols {
            total += natural[k as usize];
            least_total += minimum[k as usize];
            k += 1;
        }
        let mut j: i64 = 0;
        while j < cols {
            let lo: f64 = minimum[j as usize];
            let hi: f64 = natural[j as usize];
            if total <= w {
                widths.push(hi);
            } else {
                if least_total >= w {
                    widths.push(lo);
                } else {
                    let want: f64 = total - least_total;
                    let mut share: f64 = 0.0;
                    if want > 0.0 {
                        share = ((hi - lo) / want) * (w - least_total);
                    }
                    widths.push(lo + share);
                }
            }
            j += 1;
        }
        widths
    }
    /// the elements that have no closing tag
    pub fn isVoidTag(nm: &str) -> bool {
        MdText::index_of(&" img input hr source embed meta link area col param track base ".to_string(), &format!(" {} ", nm)) >= 0
    }
    /// what a slide cannot show at all: moving pictures and other pages
    pub fn isMediaTag(nm: &str) -> bool {
        MdText::index_of(&" video audio iframe object embed canvas frame frameset applet ".to_string(), &format!(" {} ", nm)) >= 0
    }
    pub fn isInlineTag(nm: &str) -> bool {
        MdText::index_of(&" b strong i em cite var dfn u ins s strike del mark code tt samp kbd sub sup small big abbr acronym a span font q br wbr ul ol li ".to_string(), &format!(" {} ", nm)) >= 0
    }
    /// ----------------------------------------------------------------------
    /// TeX math
    /// ----------------------------------------------------------------------
    ///
    /// A formula is set by `MathLayout` in ems and drawn at `mathPx` pixels per
    /// em: a little larger than the text around it, because the Computer
    /// Modern faces the formulas are cut from are small on the body — the same
    /// 1.21 KaTeX uses. Boxes are kept per source, since a line is measured
    /// several times while it is being broken.
    pub fn mathPx(size: f64) -> f64 {
        size * 1.21
    }
    pub fn leadingSpaces(v: &str) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if false == MdChar::isSpaceOrTab(c) {
                return i;
            }
            i += 1;
        }
        n
    }
    pub fn trailingSpaces(v: &str, n: i64, lead: i64) -> i64 {
        let mut i: i64 = n;
        let mut going: bool = true;
        while going {
            if i <= lead {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&v, i - 1);
                if MdChar::isSpaceOrTab(c) {
                    i -= 1;
                } else {
                    going = false;
                }
            }
        }
        n - i
    }
    pub fn trimTrailing(v: &str) -> String {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut i: i64 = n;
        let mut going: bool = true;
        while going {
            if i <= 0 {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&v, i - 1);
                if MdChar::isSpaceOrTab(c) {
                    i -= 1;
                } else {
                    going = false;
                }
            }
        }
        MdText::substr(&v, 0, i)
    }
    pub fn isHeadRow(row: &Rc<RefCell<MdNode>>) -> bool {
        if row.borrow().childCount() == 0 {
            return false;
        }
        let mut c: Rc<RefCell<MdNode>> = row.borrow().childAt(0);
        return c.borrow().isHeader;
    }
    pub fn shiftAfter(mut pages: &mut Vec<i64>, p: i64) {
        let mut i: i64 = 0;
        while i < (pages.len() as i64) {
            let v: i64 = pages[i as usize];
            if v > p {
                pages[i as usize] = v - 1;
            }
            i += 1;
        }
    }
    /// How much a `w`×`h` picture is set larger to fill a room `roomW` wide
    /// and `roomH` high, at most `most` times; 1 when it is as large already
    /// or the room is too low to show it larger (then the slide's own rules
    /// shrink it or move it on).
    pub fn pictureScale(w: f64, h: f64, room_w: f64, room_h: f64, most: f64) -> f64 {
        if w <= 0.0 || h <= 0.0 {
            return 1.0;
        }
        let mut s: f64 = room_w / w;
        if room_h / h < s {
            s = room_h / h;
        }
        if s > most {
            s = most;
        }
        if s < 1.0 {
            return 1.0;
        }
        s
    }
    /// The one image a paragraph holds, when that is ALL it holds. A caption
    /// beside the picture makes it a sentence with a picture in it, and a
    /// sentence is laid out as a sentence.
    pub fn loneImage(node: &Rc<RefCell<MdNode>>) -> Rc<RefCell<MdNode>> {
        let mut found: Rc<RefCell<MdNode>> = Rc::new(RefCell::new(MdNode::new()));
        let mut i: i64 = 0;
        let n: i64 = node.borrow().childCount();
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if c.borrow().kind == "image" {
                if found.borrow().kind == "image" {
                    return Rc::new(RefCell::new(MdNode::new()));
                }
                found = c.clone();
            } else {
                if c.borrow().kind == "text" {
                    if !c.borrow().literal.trim().to_string().is_empty() {
                        return Rc::new(RefCell::new(MdNode::new()));
                    }
                } else {
                    if c.borrow().kind != "softbreak" {
                        return Rc::new(RefCell::new(MdNode::new()));
                    }
                }
            }
            i += 1;
        }
        found
    }
    fn setMeasurer(&mut self, m: Rc<RefCell<EVGTextMeasurer>>) {
        EVGTextEngine::setMeasurer(&self.engine.clone(), m.clone());
        // Everything cached was measured through the old one.
        self.dropCache();
    }
    /// ----------------------------------------------------------------------
    /// Entry points
    /// ----------------------------------------------------------------------
    /// Front matter into the look of the page. Called before either layout;
    /// a document with no front matter is unchanged by it.
    pub fn applyMeta(&mut self, doc: &Rc<RefCell<MdNode>>) {
        let mut meta: Rc<RefCell<MdMeta>> = MdFrontMatter::of(doc);
        let mut _tmp_2344 = meta.borrow().keys.clone();
        let _tmp_2346 = MdSlideMeta::ofFrontMatter(&_tmp_2344, &meta.borrow().values.clone());
        self.deckMeta = _tmp_2346;
        if meta.borrow().count() == 0 {
            return;
        }
        let page_1: String = meta.borrow().get("page", "");
        let plen: i64 = MdText::str_len(&page_1) as i64;
        if plen > 0 {
            let pw: f64 = self.style.borrow().pageWidth;
            let ph: f64 = self.style.borrow().pageHeight;
            self.style.borrow_mut().pageWidth = MdFrontMatter::pageW(&page_1, pw, ph);
            self.style.borrow_mut().pageHeight = MdFrontMatter::pageH(&page_1, ph, pw);
        }
        let margin: String = meta.borrow().get("margin", "");
        let mlen: i64 = MdText::str_len(&margin) as i64;
        if mlen > 0 {
            let _tmp_2350 = MdFrontMatter::points(&margin, self.style.borrow().margin.clone());
            self.style.borrow_mut().margin = _tmp_2350;
        }
        self.style.borrow_mut().title = meta.borrow().get("title", "");
        self.style.borrow_mut().runningHead = MdChar::lower(&meta.borrow().get("runninghead", ""));
        self.style.borrow_mut().pageNumbers = MdChar::lower(&meta.borrow().get("pagenumbers", ""));
        // the master's header and footer: `header: ACME Oy`, `footer: {page} /
        // {pages}`, `header-image: media/logo.png`, `header-align: right`
        if meta.borrow().has("header") {
            self.style.borrow_mut().headerText = meta.borrow().get("header", "");
        }
        if meta.borrow().has("footer") {
            self.style.borrow_mut().footerText = meta.borrow().get("footer", "");
        }
        if meta.borrow().has("header-image") {
            self.style.borrow_mut()
                .headerImage = meta.borrow().get("header-image", "");
        }
        if meta.borrow().has("footer-image") {
            self.style.borrow_mut()
                .footerImage = meta.borrow().get("footer-image", "");
        }
        if meta.borrow().has("header-align") {
            self.style.borrow_mut().headerAlign = MdCss::alignWord(&meta.borrow().get("header-align", ""));
        }
        if meta.borrow().has("footer-align") {
            self.style.borrow_mut().footerAlign = MdCss::alignWord(&meta.borrow().get("footer-align", ""));
        }
        // three places on each edge, and how the edge is set
        if meta.borrow().has("header-left") {
            self.style.borrow_mut().headerLeft = meta.borrow().get("header-left", "");
        }
        if meta.borrow().has("header-center") {
            self.style.borrow_mut()
                .headerCenter = meta.borrow().get("header-center", "");
        }
        if meta.borrow().has("header-right") {
            self.style.borrow_mut()
                .headerRight = meta.borrow().get("header-right", "");
        }
        if meta.borrow().has("footer-left") {
            self.style.borrow_mut().footerLeft = meta.borrow().get("footer-left", "");
        }
        if meta.borrow().has("footer-center") {
            self.style.borrow_mut()
                .footerCenter = meta.borrow().get("footer-center", "");
        }
        if meta.borrow().has("footer-right") {
            self.style.borrow_mut()
                .footerRight = meta.borrow().get("footer-right", "");
        }
        if meta.borrow().has("header-color") {
            self.style.borrow_mut()
                .headerColor = meta.borrow().get("header-color", "");
        }
        if meta.borrow().has("footer-color") {
            self.style.borrow_mut()
                .footerColor = meta.borrow().get("footer-color", "");
        }
        if meta.borrow().has("header-background") {
            self.style.borrow_mut()
                .headerBackground = meta.borrow().get("header-background", "");
        }
        if meta.borrow().has("footer-background") {
            self.style.borrow_mut()
                .footerBackground = meta.borrow().get("footer-background", "");
        }
        if meta.borrow().has("header-size") {
            let _tmp_2356 = MdFrontMatter::points(&meta.borrow().get("header-size", ""), self.style.borrow().headerSize.clone());
            self.style.borrow_mut().headerSize = _tmp_2356;
        }
        if meta.borrow().has("footer-size") {
            let _tmp_2358 = MdFrontMatter::points(&meta.borrow().get("footer-size", ""), self.style.borrow().footerSize.clone());
            self.style.borrow_mut().footerSize = _tmp_2358;
        }
        if meta.borrow().has("header-image-height") {
            let _tmp_2360 = MdFrontMatter::points(&meta.borrow().get("header-image-height", ""), self.style.borrow().headerImageH.clone());
            self.style.borrow_mut().headerImageH = _tmp_2360;
        }
        if meta.borrow().has("footer-image-height") {
            let _tmp_2362 = MdFrontMatter::points(&meta.borrow().get("footer-image-height", ""), self.style.borrow().footerImageH.clone());
            self.style.borrow_mut().footerImageH = _tmp_2362;
        }
        if meta.borrow().has("header-skip") {
            self.style.borrow_mut().headerSkip = MdChar::lower(&meta.borrow().get("header-skip", ""));
        }
        if meta.borrow().has("footer-skip") {
            self.style.borrow_mut().footerSkip = MdChar::lower(&meta.borrow().get("footer-skip", ""));
        }
        self.style.borrow_mut().wantToc = meta.borrow().flag("toc", false);
        self.style.borrow_mut().breakOnH1 = meta.borrow().flag("breakonh1", false);
        if meta.borrow().has("highlight") {
            self.style.borrow_mut().highlightCode = meta.borrow().flag("highlight", true);
        }
    }
    /// One page, as tall as it needs to be. What the canvas scrolls.
    /// The template, as text. Parsed once here rather than on every layout.
    /// The DOCUMENT-level properties are applied by the caller, before this —
    /// see `MarkdownEdit.ensureLayout` for the order and why it is that order.
    /// Where pictures come from. Handed the store and the folder the document
    /// lives in, because `![](logo.png)` means "beside this document".
    fn setImages(&mut self, v: Rc<RefCell<Vfs>>, dir: String) {
        self.images = v.clone();
        self.docDir = dir.clone();
        self.haveImages = true;
        self.disarmCache();
    }
    pub fn setStyleSheet(&mut self, css: String) {
        self.sheetText = css.clone();
        self.sheet = Rc::new(RefCell::new(MdCss::new()));
        self.sheet.borrow_mut().media = self.media.clone();
        self.sheet.clone().borrow_mut().addStyleSheet(&css);
        // the same sheet says which lists are figures (`list-style: swot`)
        self.embeds.borrow_mut().listSheet = self.sheet.clone();
        self.dropCache();
    }
    /// What the sheet says about one block. Empty when there is no sheet,
    /// which is the common case and costs one selector match. Under a slide
    /// or section, as being under it (`#intro .lead`).
    fn styleOf(&self, node: &Rc<RefCell<MdNode>>) -> Rc<RefCell<CssComputed>> {
        return self.sheet.borrow_mut().resolveIn(node, self.scopeCtx.clone());
    }
    /// …what its own classes and id say (`MdCss.resolveOwn`): the box it is on
    fn ownStyleOf(&self, node: &Rc<RefCell<MdNode>>) -> Rc<RefCell<CssComputed>> {
        return self.sheet.borrow().resolveOwn(node, self.scopeCtx.clone());
    }
    /// ----------------------------------------------------------------------
    /// A slide's own rules
    /// ----------------------------------------------------------------------
    /// At the start of a layout: no heading yet, and the style's look as the
    /// sheet and the front matter left it, to come back to.
    pub fn startScopes(&mut self) {
        let mut none: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        self.scopeHeads = none.clone();
        let mut no_ctx: Option<Rc<RefCell<CssNode>>> = None;
        self.scopeCtx = no_ctx.clone();
        self.paperFrom.clear();
        self.paperColour.clear();
        self.paperPicture.clear();
        self.paperPending = false;
        self.scoped = self.sheet.borrow().hasScoped;
        if self.scoped {
            self.baseLook = self.style.borrow().look();
        }
    }
    /// …and at its end, the look put back: a host reads the style after a
    /// layout and must not find the last slide's colours in it.
    pub fn endScopes(&self) {
        if self.scoped {
            let mut _tmp_2381 = self.baseLook.clone();
            self.style.clone().borrow_mut().setLook(&_tmp_2381);
        }
    }
    /// A heading of the document begins: the blocks after it are under it
    /// (and under the ones above it of a lower level).
    pub fn enterHeading(&mut self, h: &Rc<RefCell<MdNode>>) {
        if h.borrow().level < 1 || h.borrow().level > 6 {
            return;
        }
        while (self.scopeHeads.len() as i64) < 7 {
            self.scopeHeads.push(Rc::new(RefCell::new(MdNode::new())));
        }
        self.scopeHeads[(h.borrow().level) as usize] = h.clone();
        let mut d: i64 = h.borrow().level + 1;
        while d <= 6 {
            self.scopeHeads[d as usize] = Rc::new(RefCell::new(MdNode::new()));
            d += 1;
        }
        self.enterScope();
    }
    /// The headings block `i` of `doc` is under, found by walking back: for a
    /// block laid out again on its own (a slide shrunk to fit).
    pub fn scopeAtBlock(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64) {
        let mut heads: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        while (heads.len() as i64) < 7 {
            heads.push(Rc::new(RefCell::new(MdNode::new())));
        }
        let mut limit: i64 = 7;
        let mut j: i64 = i;
        while j >= 0 && limit > 1 {
            let mut c: Rc<RefCell<MdNode>> = doc.borrow().childAt(j);
            if c.borrow().kind == "heading" && c.borrow().level < limit {
                heads[(c.borrow().level) as usize] = c.clone();
                limit = c.borrow().level;
            }
            j -= 1;
        }
        self.scopeHeads = heads.clone();
        self.enterScope();
    }
    pub fn enterScope(&mut self) {
        let _tmp_2386 = MdCss::scopeOf(&self.scopeHeads.clone());
        self.scopeCtx = _tmp_2386;
        if !self.scoped {
            return;
        }
        let mut _tmp_2388 = self.baseLook.clone();
        self.style.clone().borrow_mut().setLook(&_tmp_2388);
        let mut _tmp_2391 = self.style.clone();
        let mut _tmp_2392 = self.scopeCtx.clone();
        self.sheet.clone().borrow_mut().applyLook(&_tmp_2391, _tmp_2392);
        // the page's own colour from here on, when it is not the sheet's
        let n: i64 = self.paperFrom.len() as i64;
        let last: String = format!("{}|{}", self.style.borrow().pageBackground, self.style.borrow().pageBackgroundImage);
        let mut was: String = String::new();
        if n > 0 {
            was = format!("{}|{}", self.paperColour[(n - 1) as usize].clone(), self.paperPicture[(n - 1) as usize].clone());
        } else {
            was = format!("{}|{}", self.baseLook[22].clone(), self.baseLook[23].clone());
        }
        if last != was {
            if self.paperPending && n > 0 {
                self.paperColour[(n - 1) as usize] = self.style.borrow()
                    .pageBackground.clone();
                self.paperPicture[(n - 1) as usize] = self.style.borrow()
                    .pageBackgroundImage.clone();
            } else {
                self.paperFrom.push(self.page);
                self.paperColour.push(self.style.borrow().pageBackground.clone());
                self.paperPicture.push(self.style.borrow().pageBackgroundImage.clone());
            }
            self.paperPending = true;
        }
    }
    /// The heading that began a scope is on page `p`: so is the scope's
    /// colour, from there on.
    pub fn settlePaper(&mut self, p: i64) {
        if !self.paperPending {
            return;
        }
        self.paperPending = false;
        let n: i64 = self.paperFrom.len() as i64;
        if n == 0 {
            return;
        }
        self.paperFrom[(n - 1) as usize] = p;
        // one before it on the same page is overruled by it
        if n > 1 && self.paperFrom[(n - 2) as usize] == p {
            let keep_c: String = self.paperColour[(n - 1) as usize].clone();
            let keep_p: String = self.paperPicture[(n - 1) as usize].clone();
            self.paperFrom.remove((n - 1) as usize);
            self.paperColour.remove((n - 1) as usize);
            self.paperPicture.remove((n - 1) as usize);
            self.paperColour[(n - 2) as usize] = keep_c.clone();
            self.paperPicture[(n - 2) as usize] = keep_p.clone();
        }
    }
    /// The colour of page `p`: the sheet's, or the one a slide's own rules
    /// gave it (`.dark page { background-color }`).
    pub fn paperOf(&self, p: i64) -> String {
        let mut out: String = self.style.borrow().pageBackground.clone();
        let mut i: i64 = 0;
        while i < (self.paperFrom.len() as i64) {
            if self.paperFrom[i as usize] <= p {
                out = self.paperColour[i as usize].clone();
            }
            i += 1;
        }
        out
    }
    /// …and its picture
    pub fn pictureOf(&self, p: i64) -> String {
        let mut out: String = self.style.borrow().pageBackgroundImage.clone();
        let mut i: i64 = 0;
        while i < (self.paperFrom.len() as i64) {
            if self.paperFrom[i as usize] <= p {
                out = self.paperPicture[i as usize].clone();
            }
            i += 1;
        }
        out
    }
    /// Boxes measured with another style are boxes of another document — and
    /// the sheet is part of the style, because a `{.c3}` a sheet gives three
    /// columns to is different boxes from the same list under a sheet that
    /// does not.
    pub fn checkStyle(&mut self) {
        // Every number the sheet, the front matter and the reader gave, into
        // its safety range: this is the one place they have all arrived by,
        // and the last before anything is measured (`MdCssLimits`).
        MdCssLimits::clampStyle(&self.style.clone());
        let key: String = format!("{}{}", self.style.borrow().fingerprint(), self.sheetText);
        if key != self.cacheStyleKey {
            self.dropCache();
            self.cacheStyleKey = key.clone();
        }
    }
    pub fn layoutContinuous(&mut self, doc: Rc<RefCell<MdNode>>, width: f64) {
        if ((width - self.cacheWidth).abs()) > 0.01 {
            self.dropCache();
            self.cacheWidth = width;
        }
        self.checkStyle();
        self.boxes.clear();
        self.htmlNotes.clear();
        self.htmlNoteSrc.clear();
        self.missingKeys.clear();
        self.missingImages = 0;
        self.boxIndexReady = false;
        self.pageIndexReady = false;
        self.resetHeadings();
        self.startScopes();
        self.paginate = false;
        self.page = 0;
        self.pageCount = 1;
        self.contentX = self.style.borrow().margin;
        self.contentW = width - self.style.borrow().margin * 2.0;
        self.contentTop = self.style.borrow().margin;
        self.contentBottom = 0.0;
        self.cursorY = self.contentTop;
        self.startNotes(doc.clone());
        self.layoutBody(&doc);
        self.placeNotesAtEnd();
        self.endScopes();
        self.documentHeight = self.cursorY + self.style.borrow().margin;
    }
    /// The same sheets, with the breaks a DECK wants rather than the ones a
    /// document wants. Everything below `bodyPass` is `layoutPaged`'s; what is
    /// different is `slideBreakBefore`, which is asked between blocks.
    pub fn layoutSlides(&mut self, doc: Rc<RefCell<MdNode>>) {
        self.slides = true;
        self.slideOverflows = 0;
        self.slideTitle = String::new();
        self.slideTitleLevel = 0;
        self.clearSection();
        self.contPage.clear();
        self.contTitle.clear();
        self.contTitleLevel.clear();
        self.contSection.clear();
        self.contSectionLevel.clear();
        self.layoutPaged(doc.clone());
        self.slides = false;
    }
    /// What the document and the template said about where a slide starts. The
    /// document wins, because it is closer to the deck than the company is.
    pub fn applySlideMeta(&mut self, doc: &Rc<RefCell<MdNode>>) {
        if self.sheet.borrow().splitLevel > 0 {
            self.splitLevel = self.sheet.borrow().splitLevel;
        }
        if !self.sheet.borrow().overflow.is_empty() {
            self.slideOverflow = self.sheet.borrow().overflow.clone();
        }
        let mut meta: Rc<RefCell<MdMeta>> = MdFrontMatter::of(doc);
        self.book = MdBook::of(&meta);
        if meta.borrow().has("slide-split-level") {
            let n: Option<i64> = meta.borrow().get("slide-split-level", "1")
                .trim()
                .to_string()
                .parse::<i64>()
                .ok();
            let lvl: i64 = if n.is_some() { n.unwrap() } else { 1 };
            if lvl > 0 {
                self.splitLevel = lvl;
            }
        }
        // `valign: center` / `bottom`: every slide's content under its title
        // (MdSlideLayout), unless the slide says otherwise
        if meta.borrow().has("valign") {
            self.deckPlace = MdSlideLayout::valignOf(&meta.borrow().get("valign", ""));
        }
        if meta.borrow().has("slide-overflow") {
            self.slideOverflow = MdChar::lower(&meta.borrow().get("slide-overflow", "split").trim().to_string());
        }
        // `slide-grow: 1.5` lets a slide with room grow by up to half again;
        // `slide-grow: off` keeps every slide at the document's sizes
        if meta.borrow().has("slide-grow") {
            let g: String = MdChar::lower(&meta.borrow().get("slide-grow", "").trim().to_string());
            if g == "off" || g == "none" {
                self.slideGrow = 1.0;
            } else {
                let gv: Option<f64> = g.parse::<f64>().ok();
                let gd: f64 = if gv.is_some() { gv.unwrap() } else { 1.0 };
                if gd >= 1.0 {
                    self.slideGrow = gd;
                }
            }
        }
        // a heading's `layout=` holds its slide only, not the slides after it
        MdColumns::endAtSlides(doc, self.splitLevel.clone());
    }
    /// Sheets of a stated size, with the breaks decided here. What is printed.
    pub fn layoutPaged(&mut self, doc: Rc<RefCell<MdNode>>) {
        self.checkStyle();
        self.disarmCache();
        self.boxes.clear();
        self.htmlNotes.clear();
        self.htmlNoteSrc.clear();
        self.missingKeys.clear();
        self.missingImages = 0;
        self.boxIndexReady = false;
        self.pageIndexReady = false;
        self.resetHeadings();
        self.startScopes();
        self.paginate = true;
        self.page = 0;
        self.pageCount = 1;
        self.contentX = self.style.borrow().margin;
        let _tmp_2408 = self.style.borrow()
            .pageWidth - self.style.borrow()
            .margin * 2.0;
        self.contentW = _tmp_2408;
        self.contentTop = self.style.borrow().margin;
        let _tmp_2410 = self.style.borrow()
            .pageHeight - self.style.borrow()
            .margin;
        self.contentBottom = _tmp_2410;
        if self.slides {
            self.applyBookMargins();
        }
        self.headerOff.clear();
        self.footerOff.clear();
        self.metaPage.clear();
        self.metaKey.clear();
        self.metaVal.clear();
        self.metaSuspect.clear();
        self.makeEdgeRoom();
        self.fullContentW = self.contentW;
        self.slideAside = false;
        self.cursorY = self.contentTop;
        self.blockFirstPage.clear();
        self.blockLastPage.clear();
        self.placeSrc.clear();
        self.placeWord.clear();
        self.slideCenter = false;
        self.slideStatement = false;
        self.slidesGrown = 0;
        self.slidesShrunk = 0;
        self.slideTitleOnly = false;
        self.spillExcess.clear();
        self.startNotes(doc.clone());
        self.layoutBody(&doc);
        self.slideTitleOnly = false;
        if self.slides && self.tocLines == 0 {
            self.shrinkSlides(&doc);
        }
        if (self.slides && self.slideGrow > 1.01) && self.tocLines == 0 {
            self.growSlides(&doc);
        }
        if self.slides {
            self.alignSlides();
        }
        self.slideCenter = false;
        self.slideStatement = false;
        self.placeNotesPaged();
        self.endScopes();
        self.emitFurniture();
        if self.slides {
            self.mirrorBook();
        }
        self.documentHeight = self.style.borrow().pageHeight;
    }
    /// The front matter's margins (MdBook), each edge over the page's own.
    /// Every page of a book is laid out as a LEFT-hand page — the outside
    /// margin on the left — and `mirrorBook` moves the right-hand pages over
    /// afterwards. The text column is as wide on both sides of a spread, so a
    /// page laid out once is right on either side once it is moved; laying a
    /// page out against its own side would need the side known before the
    /// block that breaks onto it, which a block cut in two does not know.
    pub fn applyBookMargins(&mut self) {
        let pw: f64 = self.style.borrow().pageWidth;
        let ph: f64 = self.style.borrow().pageHeight;
        let mut left: f64 = self.book.borrow().inside(self.style.borrow().margin.clone(), pw);
        let mut right: f64 = self.book.borrow().outside(self.style.borrow().margin.clone(), pw);
        if self.book.borrow().isBook() {
            left = self.book.borrow().outside(self.style.borrow().margin, pw);
            right = self.book.borrow().inside(self.style.borrow().margin, pw);
        }
        self.contentX = left;
        self.contentW = (pw - left) - right;
        let _tmp_2421 = self.book.borrow().top(self.style.borrow().margin.clone(), ph);
        self.contentTop = _tmp_2421;
        let _tmp_2424 = ph - self.book.borrow().bottom(self.style.borrow().margin.clone(), ph);
        self.contentBottom = _tmp_2424;
    }
    /// A book's right-hand pages, moved from the left-hand layout to their
    /// own: everything on them by (inside - outside), but not what is drawn
    /// to the page's edges on purpose (a full-page picture, a band into the
    /// bleed), which belongs to the sheet and not to its text column.
    pub fn mirrorBook(&self) {
        if !self.book.borrow().isBook() {
            return;
        }
        let pw: f64 = self.style.borrow().pageWidth;
        let mut _tmp_2427 = self.style.borrow().margin.clone();
        let dx: f64 = self.book.borrow().inside(_tmp_2427, pw) - self.book.borrow().outside(self.style.borrow().margin.clone(), pw);
        if (dx.abs()) < 0.01 {
            return;
        }
        let mut i: i64 = 0;
        let n: i64 = self.boxes.len() as i64;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if (self.book.borrow().isRecto(b.borrow().page.clone()) && !b.borrow().bleeds) && !MdLayout::coversPageWidth(&b, pw) {
                let x_v = b.borrow().x + dx;
                b.borrow_mut().x = x_v;
                if b.borrow().alignW > 0.0 {
                    let alignX_v = b.borrow().alignX + dx;
                    b.borrow_mut().alignX = alignX_v;
                }
            }
            i += 1;
        }
    }
    /// ----------------------------------------------------------------------
    /// A table of contents, and what sits in the margins
    /// ----------------------------------------------------------------------
    ///
    /// The contents are laid out TWICE, and the reason is the oldest one in
    /// typesetting: the page numbers depend on where the blocks fall, and where
    /// the blocks fall depends on how tall the contents are. So the first pass
    /// reserves exactly the height the list will need — a number that does not
    /// depend on the page numbers at all, only on how many headings there are —
    /// and the second pass fills the reserved space in. Nothing moves between
    /// the two, so the numbers are right rather than nearly right.
    pub fn tocHeight(&self) -> f64 {
        if self.tocLines == 0 {
            return 0.0;
        }
        let lh: f64 = self.style.borrow()
            .bodySize * self.style.borrow()
            .lineHeight;
        let head: f64 = self.style.borrow()
            .h2Size * self.style.borrow()
            .lineHeight + self.style.borrow()
            .headingSpaceAfter;
        ((head + lh * (self.tocLines as f64)) + self.style.borrow().headingSpaceBefore) + self.style.borrow()
            .paraSpacing
    }
    pub fn emitToc(&mut self, x: f64, w: f64) {
        if self.tocLines == 0 {
            return;
        }
        let lh: f64 = self.style.borrow()
            .bodySize * self.style.borrow()
            .lineHeight;
        self.cursorY += self.style.borrow().headingSpaceBefore;
        let h2_lh: f64 = self.style.borrow().h2Size * self.style.borrow().lineHeight;
        let hy: f64 = self.reserve(h2_lh);
        let mut h: Rc<RefCell<MdBox>> = MdBox::textBox(
            "Contents".to_string(),
            x,
            hy,
            w,
            self.style.borrow().h2Size * self.style.borrow().lineHeight
        );
        h.borrow_mut().family = self.style.borrow().headingFamily.clone();
        h.borrow_mut().size = self.style.borrow().h2Size;
        h.borrow_mut().bold = true;
        h.borrow_mut().color = self.style.borrow().textColor.clone();
        self.emit(h.clone());
        let _tmp_2437 = (hy + self.style.borrow().h2Size * self.style.borrow().lineHeight) + self.style.borrow()
            .headingSpaceAfter;
        self.cursorY = _tmp_2437;
        let mut i: i64 = 0;
        let n: i64 = self.knownPages.len() as i64;
        while i < n {
            let level: i64 = self.knownLevel[i as usize];
            if level <= 3 {
                let y: f64 = self.reserve(lh);
                let indent: f64 = ((level - 1) as f64) * 14.0;
                let mut label: Rc<RefCell<MdBox>> = MdBox::textBox(
                    self.knownText[i as usize].clone(),
                    x + indent,
                    y,
                    w - indent,
                    lh
                );
                label.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                label.borrow_mut().size = self.style.borrow().bodySize;
                label.borrow_mut().color = self.style.borrow().textColor.clone();
                self.emit(label.clone());
                // The page number, right-aligned against the column's edge.
                let num: String = (self.knownPages[i as usize] + 1).to_string();
                let mut probe: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
                probe.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                probe.borrow_mut().size = self.style.borrow().bodySize;
                let nw: f64 = self.measureRun(&num, &probe);
                let mut nb: Rc<RefCell<MdBox>> = MdBox::textBox(num.clone(), (x + w) - nw, y, nw, lh);
                nb.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                nb.borrow_mut().size = self.style.borrow().bodySize;
                nb.borrow_mut().color = self.style.borrow().mutedColor.clone();
                self.emit(nb.clone());
                self.cursorY = y + lh;
            }
            i += 1;
        }
        self.cursorY += self.style.borrow().paraSpacing;
    }
    /// The running head and the folio, on every sheet. Emitted after the
    /// content, because until then nobody knows how many sheets there are.
    pub fn emitFurniture(&mut self) {
        if false == self.paginate {
            return;
        }
        self.emitHeadFoot();
        let mut want_head: bool = !self.style.borrow().runningHead.is_empty();
        let mut want_num: bool = !self.style.borrow().pageNumbers.is_empty();
        if false == want_head && false == want_num {
            return;
        }
        if self.style.borrow().runningHead == "none" {
            want_head = false;
        }
        if self.style.borrow().pageNumbers == "none" {
            want_num = false;
        }
        let saved_page: i64 = self.page;
        let x: f64 = self.contentX;
        let w: f64 = self.contentW;
        let lh: f64 = self.style.borrow().furnitureSize * 1.3;
        let mut p: i64 = 0;
        while p < self.pageCount {
            self.page = p;
            if want_head {
                let text: String = self.headFor(p);
                let tlen: i64 = MdText::str_len(&text) as i64;
                if tlen > 0 {
                    let hy: f64 = (self.contentTop - lh) - 6.0;
                    let mut b: Rc<RefCell<MdBox>> = MdBox::textBox(text.clone(), x, hy, w, lh);
                    b.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                    b.borrow_mut().size = self.style.borrow().furnitureSize;
                    b.borrow_mut().color = self.style.borrow().mutedColor.clone();
                    self.emit(b.clone());
                    let arg_0 = MdBox::rect(
                        x,
                        self.contentTop - 5.0,
                        w,
                        0.5,
                        self.style.borrow().ruleColor.clone()
                    ).clone();
                    self.emit(arg_0);
                }
            }
            if want_num {
                let num: String = (p + 1).to_string();
                let mut probe: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
                probe.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                probe.borrow_mut().size = self.style.borrow().furnitureSize;
                let nw: f64 = self.measureRun(&num, &probe);
                let ny: f64 = self.contentBottom + 10.0;
                let mut nx: f64 = (x + w / 2.0) - nw / 2.0;
                if self.style.borrow().pageNumbers == "bottom-right" {
                    nx = (x + w) - nw;
                }
                if self.style.borrow().pageNumbers == "bottom-outer" {
                    // Outer is right on a recto and left on a verso, counting
                    // the first sheet as page one.
                    let mut recto: bool = p % 2 == 0;
                    if self.slides && self.book.borrow().isBook() {
                        recto = self.book.borrow().isRecto(p);
                    }
                    if recto {
                        nx = (x + w) - nw;
                    } else {
                        nx = x;
                    }
                }
                let mut nb: Rc<RefCell<MdBox>> = MdBox::textBox(num.clone(), nx, ny, nw, lh);
                nb.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                nb.borrow_mut().size = self.style.borrow().furnitureSize;
                nb.borrow_mut().color = self.style.borrow().mutedColor.clone();
                self.emit(nb.clone());
            }
            p += 1;
        }
        self.page = saved_page;
    }
    /// The header and the footer on every sheet: up to three places on each
    /// edge (left, centre, right), each a picture and a line of text, in the
    /// top and the bottom margin. `header: …` with `header-align` is the place
    /// that alignment names.
    pub fn emitHeadFoot(&mut self) {
        let mut top: Vec<String> = self.edgeSlots(true);
        let mut bottom: Vec<String> = self.edgeSlots(false);
        let want_top: bool = MdLayout::anySlot(&top);
        let want_bottom: bool = MdLayout::anySlot(&bottom);
        if false == want_top && false == want_bottom {
            return;
        }
        let saved_page: i64 = self.page;
        let mut p: i64 = 0;
        while p < self.pageCount {
            self.page = p;
            if want_top && self.edgeShown(p, true) {
                self.edgeRow(&top, p, true);
            }
            if want_bottom && self.edgeShown(p, false) {
                self.edgeRow(&bottom, p, false);
            }
            p += 1;
        }
        self.page = saved_page;
    }
    /// [left, centre, right] of one edge, the single `header:` folded into
    /// the place its alignment names.
    pub fn edgeSlots(&self, top: bool) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let text: String = if top { self.style.borrow().headerText.clone() } else { self.style.borrow().footerText.clone() };
        let image: String = if top { self.style.borrow().headerImage.clone() } else { self.style.borrow().footerImage.clone() };
        let align_1: String = if top { self.style.borrow().headerAlign.clone() } else { self.style.borrow().footerAlign.clone() };
        out.push(if top { self.style.borrow().headerLeft.clone() } else { self.style.borrow().footerLeft.clone() });
        out.push(if top { self.style.borrow().headerCenter.clone() } else { self.style.borrow().footerCenter.clone() });
        out.push(if top { self.style.borrow().headerRight.clone() } else { self.style.borrow().footerRight.clone() });
        let mut one: String = text.clone();
        if !image.is_empty() {
            one = format!("![]({}) {}", image, text).trim().to_string();
        }
        if !one.is_empty() {
            let mut at: i64 = 0;
            if align_1 == "center" {
                at = 1;
            }
            if align_1 == "end" {
                at = 2;
            }
            let was: String = out[at as usize].clone();
            if was.is_empty() {
                out[at as usize] = one.clone();
            } else {
                out[at as usize] = format!("{} {}", was, one);
            }
        }
        out
    }
    /// Whether an edge is drawn on sheet p: not where `header-skip` names the
    /// sheet (first, last, its number) nor where its heading turned it off.
    pub fn edgeShown(&self, p: i64, top: bool) -> bool {
        let mut off: Vec<i64> = if top { self.headerOff.clone() } else { self.footerOff.clone() };
        let mut i: i64 = 0;
        while i < (off.len() as i64) {
            if off[i as usize] == p {
                return false;
            }
            i += 1;
        }
        let skip: String = if top { self.style.borrow().headerSkip.clone() } else { self.style.borrow().footerSkip.clone() };
        !MdLayout::skips(&skip, p, self.pageCount.clone())
    }
    /// `{header=off}`, `{footer=off}` or `{furniture=off}` on a heading.
    pub fn readEdgesOff(&mut self, node: &Rc<RefCell<MdNode>>) {
        let both: bool = MdLayout::isOff(&node.borrow().attrValue("furniture"));
        if both || MdLayout::isOff(&node.borrow().attrValue("header")) {
            self.headerOff.push(self.page);
        }
        if both || MdLayout::isOff(&node.borrow().attrValue("footer")) {
            self.footerOff.push(self.page);
        }
    }
    /// `{jira=ACME-412 owner=Tero}` on a heading: the slide's own data.
    /// A slide laid out again (a sheet that grew, a block that spilled) reads
    /// its heading again, so the same key is written once per sheet rather
    /// than once per pass.
    pub fn readSlideMeta(&mut self, node: &Rc<RefCell<MdNode>>) {
        let mut m: Rc<RefCell<MdSlideMeta>> = MdSlideMeta::of(node);
        let mut i: i64 = 0;
        while i < m.borrow().count() {
            let key: String = m.borrow().keys[i as usize].clone();
            let at: i64 = self.metaAt(self.page.clone(), &key);
            if at >= 0 {
                self.metaVal[at as usize] = m.borrow().values[i as usize].clone();
            } else {
                self.metaPage.push(self.page);
                self.metaKey.push(key.clone());
                self.metaVal.push(m.borrow().values[i as usize].clone());
            }
            i += 1;
        }
        let mut j: i64 = 0;
        while j < (m.borrow().suspect.len() as i64) {
            let mut pair: Vec<String> = m.borrow()
                .suspect[j as usize].clone()
                .split("~")
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            if (pair.len() as i64) == 2 {
                let line: String = format!("{}|{}|{}", self.page.to_string(), pair[0].clone(), pair[1].clone());
                if (self.metaSuspect.iter().position( |__r| __r.clone() == line.clone() ).map(|__i| __i as i64).unwrap_or(-1)) < 0 {
                    self.metaSuspect.push(line.clone());
                }
            }
            j += 1;
        }
    }
    pub fn metaAt(&self, p: i64, key: &str) -> i64 {
        let mut i: i64 = 0;
        while i < (self.metaPage.len() as i64) {
            if self.metaPage[i as usize] == p && self.metaKey[i as usize].clone() == key {
                return i;
            }
            i += 1;
        }
        -1
    }
    /// What sheet p's heading said. Empty when it said nothing.
    pub fn slideMeta(&self, p: i64) -> Rc<RefCell<MdSlideMeta>> {
        let mut out: Rc<RefCell<MdSlideMeta>> = Rc::new(RefCell::new(MdSlideMeta::new()));
        let mut i: i64 = 0;
        while i < (self.metaPage.len() as i64) {
            if self.metaPage[i as usize] == p {
                out.borrow_mut().put(&self.metaKey[i as usize].clone(), &self.metaVal[i as usize].clone());
            }
            i += 1;
        }
        out
    }
    pub fn metaValue(&self, p: i64, key: &str) -> String {
        let want: String = MdChar::lower(&key.trim().to_string());
        let mut i: i64 = 0;
        while i < (self.metaPage.len() as i64) {
            if self.metaPage[i as usize] == p && self.metaKey[i as usize].clone() == want {
                return self.metaVal[i as usize].clone();
            }
            i += 1;
        }
        String::new()
    }
    /// One line for the deck's data and one per slide that carries any, for a
    /// host's report.
    pub fn slideMetaNotes(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if self.deckMeta.borrow().count() > 0 {
            out.push(format!("Presentation: {}", self.deckMeta.borrow().asText()));
        }
        let mut p: i64 = 0;
        while p < self.pageCount {
            let mut m: Rc<RefCell<MdSlideMeta>> = self.slideMeta(p);
            if m.borrow().count() > 0 {
                let name: String = self.pageTitle(p);
                let mut head: String = format!("Slide {}", p + 1);
                if !name.is_empty() {
                    head = format!("{} \"{}\"", head, name);
                }
                out.push(format!("{}: {}", head, m.borrow().asText()));
            }
            p += 1;
        }
        out
    }
    /// A value the host sets while the deck runs (`liveKeys`); "" as the
    /// key clears them all. True when it changed anything.
    pub fn setLive(&mut self, key0: &str, value: &str) -> bool {
        let key: String = MdChar::lower(&key0.trim().to_string());
        if key.is_empty() {
            let none: bool = self.liveKeys.is_empty();
            let mut k: Vec<String> = Vec::new();
            let mut v: Vec<String> = Vec::new();
            self.liveKeys = k.clone();
            self.liveVals = v.clone();
            return !none;
        }
        let i: i64 = self.liveKeys.iter()
            .position( |__r| __r.clone() == key.clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        if i >= 0 {
            if self.liveVals[i as usize].clone() == value {
                return false;
            }
            self.liveVals[i as usize] = value.to_string();
            return true;
        }
        self.liveKeys.push(key);
        self.liveVals.push(value.to_string());
        true
    }
    /// The value `{key}` prints on sheet p: the slide's, else the deck's.
    pub fn dataValue(&self, p: i64, key: &str) -> String {
        let want: String = MdChar::lower(&key.trim().to_string());
        let li: i64 = self.liveKeys.iter()
            .position( |__r| __r.clone() == want.clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        if li >= 0 {
            return self.liveVals[li as usize].clone();
        }
        if self.metaAt(p, &want) >= 0 {
            return self.metaValue(p, &want);
        }
        return self.deckMeta.borrow().value(&want);
    }
    /// A key one character from an engine key: kept as data, and said so.
    pub fn slideMetaWarnings(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut w: i64 = 0;
        while w < (self.deckMeta.borrow().suspect.len() as i64) {
            let mut pair: Vec<String> = self.deckMeta.borrow()
                .suspect[w as usize].clone()
                .split("~")
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            if (pair.len() as i64) == 2 {
                out.push(format!("Front matter: {} is not a key Sliqtly knows, so it is kept as the presentation's own data. Did you mean {}?", pair[0].clone(), pair[1].clone()));
            }
            w += 1;
        }
        let mut i: i64 = 0;
        while i < (self.metaSuspect.len() as i64) {
            let mut parts: Vec<String> = self.metaSuspect[i as usize].clone()
                .split("|")
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            if (parts.len() as i64) == 3 {
                let pno: Option<i64> = parts[0].clone().parse::<i64>().ok();
                let p: i64 = if pno.is_some() { pno.unwrap() } else { 0 };
                out.push(format!("Slide {}: {} is not a key Sliqtly knows, so it is kept as the slide's own data. Did you mean {}?", p + 1, parts[1].clone(), parts[2].clone()));
            }
            i += 1;
        }
        out
    }
    /// The heading a sheet was given, or the name a page without one got.
    pub fn pageTitle(&self, p: i64) -> String {
        let mut i: i64 = 0;
        let mut out: String = String::new();
        while i < (self.headPage.len() as i64) {
            if self.headPage[i as usize] == p {
                if out.is_empty() {
                    out = self.headText[i as usize].clone();
                }
            }
            i += 1;
        }
        if out.is_empty() {
            out = self.pageName(p);
        }
        out
    }
    /// The text size of an edge.
    pub fn edgeSize(&self, top: bool) -> f64 {
        let mut size: f64 = if top { self.style.borrow().headerSize } else { self.style.borrow().footerSize };
        if size <= 0.0 {
            size = self.style.borrow().furnitureSize;
            if self.slides {
                size = self.style.borrow().bodySize * 0.6;
            }
        }
        size
    }
    /// How tall an edge's row is: its line, or its tallest picture.
    pub fn edgeRowH(&self, slots: &[String], top: bool) -> f64 {
        let lh: f64 = self.edgeSize(top) * 1.3;
        let img_h: f64 = if top { self.style.borrow().headerImageH } else { self.style.borrow().footerImageH };
        let mut h: f64 = lh;
        let mut i: i64 = 0;
        while i < (slots.len() as i64) {
            if (MdText::str_len(&MdLayout::slotImage(&slots[i as usize].clone())) as i64) > 0 && img_h > h {
                h = img_h;
            }
            i += 1;
        }
        h
    }
    /// A margin too small for its edge is widened, so a tall logo pushes the
    /// content down rather than sitting on it.
    pub fn makeEdgeRoom(&mut self) {
        let mut top: Vec<String> = self.edgeSlots(true);
        if MdLayout::anySlot(&top) {
            let need: f64 = self.edgeRowH(&top, true) + 14.0;
            if self.contentTop < need {
                self.contentTop = need;
            }
        }
        let mut bottom: Vec<String> = self.edgeSlots(false);
        if MdLayout::anySlot(&bottom) {
            let need_b: f64 = self.edgeRowH(&bottom, false) + 14.0;
            if self.style.borrow()
                .pageHeight - self.contentBottom < need_b {
                self.contentBottom = self.style.borrow().pageHeight - need_b;
            }
        }
    }
    /// One edge on sheet p: the band and the hairline, then each place.
    pub fn edgeRow(&mut self, slots: &[String], p: i64, top: bool) {
        let size: f64 = self.edgeSize(top);
        // unused:  let lh: f64 = size * 1.3;
        let row_h: f64 = self.edgeRowH(slots, top);
        let mut y: f64 = (self.contentTop - row_h) - 8.0;
        if !top {
            y = self.contentBottom + 8.0;
        }
        if y < 2.0 {
            y = 2.0;
        }
        let mark: i64 = if top { 1 } else { 2 };
        let band: String = if top { self.style.borrow().headerBackground.clone() } else { self.style.borrow().footerBackground.clone() };
        let have_band: bool = !band.is_empty() && band != "transparent";
        if have_band {
            let mut by: f64 = 0.0;
            let mut bh: f64 = (y + row_h) + 8.0;
            if !top {
                by = y - 8.0;
                bh = self.style.borrow().pageHeight - by;
            }
            let mut bb: Rc<RefCell<MdBox>> = MdBox::rect(0.0, by, self.style.borrow().pageWidth.clone(), bh, band.clone());
            bb.borrow_mut().furniture = mark;
            self.emit(bb.clone());
        }
        let rule: String = if top { self.style.borrow().headerRule.clone() } else { self.style.borrow().footerRule.clone() };
        if !rule.is_empty() && rule != "transparent" {
            let mut rx: f64 = self.contentX;
            let mut rw: f64 = self.contentW;
            if have_band {
                rx = 0.0;
                rw = self.style.borrow().pageWidth;
            }
            let mut ry: f64 = (y + row_h) + 7.0;
            if !top {
                ry = y - 8.0;
            }
            let mut rb: Rc<RefCell<MdBox>> = MdBox::rect(rx, ry, rw, 0.75, rule.clone());
            rb.borrow_mut().furniture = mark;
            self.emit(rb.clone());
        }
        // each place's natural width, then the room it gets
        let mut widths: Vec<f64> = Vec::new();
        let mut i: i64 = 0;
        while i < 3 {
            widths.push(self.slotWidth(&slots[i as usize].clone(), p, top));
            i += 1;
        }
        let gap: f64 = size * 1.5;
        let wl: f64 = widths[0];
        let mut wc: f64 = widths[1];
        let wr: f64 = widths[2];
        let mut side: f64 = self.contentW;
        if wc > 0.0 {
            side = (self.contentW - wc) * 0.5 - gap;
        }
        if wc > self.contentW {
            wc = self.contentW;
        }
        let mut room_l: f64 = side;
        let mut room_r: f64 = side;
        if wc <= 0.0 {
            if wr > 0.0 {
                room_l = (self.contentW - wr) - gap;
            }
            if wl > 0.0 {
                room_r = (self.contentW - wl) - gap;
            }
        }
        if wl > 0.0 {
            let mut _tmp_2463 = self.contentX.clone();
            self.slotAt(&slots[0].clone(), _tmp_2463, y, row_h, MdLayout::minD(wl, room_l), p, top);
        }
        if wc > 0.0 {
            self.slotAt(&slots[1].clone(), self.contentX + (self.contentW - wc) * 0.5, y, row_h, wc, p, top);
        }
        if wr > 0.0 {
            let w2: f64 = MdLayout::minD(wr, room_r);
            self.slotAt(&slots[2].clone(), (self.contentX + self.contentW) - w2, y, row_h, w2, p, top);
        }
    }
    /// An edge's text colour when nobody named one: the muted colour, unless
    /// it is too faint on the sheet to read at that size (WCAG 4.5:1); then
    /// the text colour.
    pub fn edgeInk(&self) -> String {
        let mut paper: String = self.style.borrow().pageBackground.clone();
        if paper.is_empty() {
            paper = "#ffffff".to_string();
        }
        if MdLayout::contrast(&self.style.borrow().mutedColor.clone(), &paper) >= 4.5 {
            return self.style.borrow().mutedColor.clone();
        }
        self.style.borrow().textColor.clone()
    }
    /// A place's picture as the byte store has it; `path` empty when it has not.
    pub fn slotPicture(&self, slot: &str, top: bool) -> Rc<RefCell<MdSlotPic>> {
        let mut out: Rc<RefCell<MdSlotPic>> = Rc::new(RefCell::new(MdSlotPic::new()));
        let image: String = MdLayout::slotImage(slot);
        if image.is_empty() || !self.haveImages {
            return out;
        }
        let resolved: String = Vfs::resolve(&self.images, &image, &self.docDir.clone());
        if resolved.is_empty() {
            return out;
        }
        let mut st: Rc<RefCell<VfsStat>> = Vfs::stat(&self.images, &resolved);
        if !st.borrow().found {
            return out;
        }
        out.borrow_mut().h = if top { self.style.borrow().headerImageH } else { self.style.borrow().footerImageH };
        let w_v = out.borrow().h;
        out.borrow_mut().w = w_v;
        if st.borrow().pixelH > 0 {
            let w_v = out.borrow().h * ((st.borrow().pixelW as f64) / (st.borrow().pixelH as f64));
            out.borrow_mut().w = w_v;
        }
        out.borrow_mut().path = resolved.clone();
        out.borrow_mut().natW = st.borrow().pixelW;
        out.borrow_mut().natH = st.borrow().pixelH;
        out
    }
    pub fn slotRun(&self, top: bool) -> Rc<RefCell<MdRun>> {
        let mut probe: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        probe.borrow_mut().family = self.style.borrow().bodyFamily.clone();
        probe.borrow_mut().size = self.edgeSize(top);
        probe.borrow_mut().bold = if top { self.style.borrow().headerBold } else { self.style.borrow().footerBold };
        probe
    }
    pub fn slotWidth(&mut self, slot: &str, p: i64, top: bool) -> f64 {
        if slot.trim().to_string().is_empty() {
            return 0.0;
        }
        let text: String = self.slotFilled(slot, p);
        let mut tw: f64 = 0.0;
        if !text.is_empty() {
            tw = self.measureRun(&text, &self.slotRun(top));
        }
        let iw: f64 = self.slotPicture(slot, top).borrow().w;
        if iw > 0.0 && tw > 0.0 {
            return (iw + tw) + self.edgeSize(top) * 0.8;
        }
        iw + tw
    }
    /// One place, from x, in w at most.
    pub fn slotAt(&mut self, slot: &str, x: f64, y: f64, row_h: f64, w: f64, p: i64, top: bool) {
        let size: f64 = self.edgeSize(top);
        let lh: f64 = size * 1.3;
        let mut colour: String = if top { self.style.borrow().headerColor.clone() } else { self.style.borrow().footerColor.clone() };
        if colour.is_empty() {
            colour = self.edgeInk();
        }
        let text: String = self.slotFilled(slot, p);
        let mut pic: Rc<RefCell<MdSlotPic>> = self.slotPicture(slot, top);
        let mut iw: f64 = pic.borrow().w;
        let ih: f64 = pic.borrow().h;
        let mut gap: f64 = 0.0;
        if iw > 0.0 && !text.is_empty() {
            gap = size * 0.8;
        }
        if iw > w {
            iw = w;
        }
        let tw: f64 = (w - iw) - gap;
        let first: bool = MdLayout::imageFirst(slot);
        let mut ix: f64 = x;
        let mut tx: f64 = (x + iw) + gap;
        if !first {
            tx = x;
            ix = (x + w) - iw;
        }
        let mark: i64 = if top { 1 } else { 2 };
        if iw > 0.0 {
            let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
            b.borrow_mut().kind = 4;
            b.borrow_mut().x = ix;
            b.borrow_mut().y = y + (row_h - ih) * 0.5;
            b.borrow_mut().w = iw;
            b.borrow_mut().h = ih;
            b.borrow_mut().imagePath = pic.borrow().path.clone();
            b.borrow_mut().imageNaturalW = pic.borrow().natW;
            b.borrow_mut().imageNaturalH = pic.borrow().natH;
            b.borrow_mut().furniture = mark;
            self.emit(b.clone());
        }
        if tw > 0.0 && !text.is_empty() {
            let mut t: Rc<RefCell<MdBox>> = MdBox::textBox(
                text.clone(),
                tx,
                y + (row_h - lh) * 0.5,
                tw,
                lh
            );
            t.borrow_mut().family = self.style.borrow().bodyFamily.clone();
            t.borrow_mut().size = size;
            t.borrow_mut().bold = if top { self.style.borrow().headerBold } else { self.style.borrow().footerBold };
            t.borrow_mut().color = colour.clone();
            t.borrow_mut().furniture = mark;
            self.emit(t.clone());
        }
    }
    /// One place's text with every token filled in: `{page}`, `{pages}`,
    /// `{title}` — and the slide's own data, so a deck whose slides carry a
    /// ticket can print it (`footer-right: "{jira}"`). A key the slide does
    /// not have leaves nothing behind rather than printing its own name,
    /// because a slide without a ticket should show no ticket.
    pub fn slotFilled(&self, slot: &str, p: i64) -> String {
        let mut _tmp_2481 = self.pageCount.clone();
        let mut _tmp_2482 = self.style.borrow().title.clone();
        let out: String = MdLayout::fillTokens(&MdLayout::slotText(slot), p + 1, _tmp_2481, &_tmp_2482);
        self.fillMeta(&out, p)
    }
    pub fn fillMeta(&self, text: &str, p: i64) -> String {
        if MdText::index_of(&text, "{") < 0 {
            return text.to_string();
        }
        let mut out: String = text.to_string();
        let mut lv: i64 = 0;
        while lv < (self.liveKeys.len() as i64) {
            out = MdLayout::replaceAll(
                &out,
                &format!("{}{}{}", "{".to_string(), self.liveKeys[lv as usize].clone(), "}".to_string()),
                &self.liveVals[lv as usize].clone()
            );
            lv += 1;
        }
        let mut m: Rc<RefCell<MdSlideMeta>> = self.slideMeta(p);
        let mut i: i64 = 0;
        while i < m.borrow().count() {
            out = MdLayout::replaceAll(
                &out,
                &format!("{}{}{}", "{".to_string(), m.borrow().keys[i as usize].clone(), "}".to_string()),
                &m.borrow().values[i as usize].clone()
            );
            i += 1;
        }
        // then the deck's, for a key this slide does not say otherwise
        let mut d: i64 = 0;
        while d < self.deckMeta.borrow().count() {
            out = MdLayout::replaceAll(
                &out,
                &format!("{}{}{}", "{".to_string(), self.deckMeta.borrow().keys[d as usize].clone(), "}".to_string()),
                &self.deckMeta.borrow().values[d as usize].clone()
            );
            d += 1;
        }
        // A key SOME slide in the deck carries, on a slide that does not:
        // the place is left empty rather than printing `{jira}` at the foot
        // of every slide without a ticket.
        let mut j: i64 = 0;
        let mut emptied: bool = false;
        while j < (self.metaKey.len() as i64) {
            let token: String = format!("{}{}{}", "{".to_string(), self.metaKey[j as usize].clone(), "}".to_string());
            if MdText::index_of(&out, &token) >= 0 {
                out = MdLayout::replaceAll(&out, &token, "");
                emptied = true;
            }
            j += 1;
        }
        if emptied {
            out = MdLayout::tidySlot(&out);
        }
        out
    }
    /// The sheets a suspect key was noted on move up with their slides.
    pub fn shiftSuspect(&mut self, p: i64) {
        let mut i: i64 = 0;
        while i < (self.metaSuspect.len() as i64) {
            let mut parts: Vec<String> = self.metaSuspect[i as usize].clone()
                .split("|")
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            if (parts.len() as i64) == 3 {
                let pno: Option<i64> = parts[0].clone().parse::<i64>().ok();
                let at: i64 = if pno.is_some() { pno.unwrap() } else { 0 };
                if at > p {
                    self.metaSuspect[i as usize] = format!("{}|{}|{}", (at - 1).to_string(), parts[1].clone(), parts[2].clone());
                }
            }
            i += 1;
        }
    }
    /// What the running head says on that sheet.
    pub fn headFor(&self, p: i64) -> String {
        if self.style.borrow().runningHead == "title" {
            return self.style.borrow().title.clone();
        }
        if self.style.borrow().runningHead != "h1" {
            return String::new();
        }
        let mut best: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = self.headText.len() as i64;
        while i < n {
            if self.headLevel[i as usize] == 1 {
                if self.headPage[i as usize] <= p {
                    best = self.headText[i as usize].clone();
                }
            }
            i += 1;
        }
        best
    }
    /// The body, in one pass or in two.
    ///
    /// Two when the document asked for a table of contents, because the page
    /// numbers in it depend on where the blocks fall and where the blocks fall
    /// depends on how tall it is. The first pass leaves a gap of exactly the
    /// right height and learns the pages; the second fills the gap in. Nothing
    /// moves between them.
    pub fn layoutBody(&mut self, doc: &Rc<RefCell<MdNode>>) {
        self.tocLines = 0;
        if self.style.borrow().wantToc {
            self.tocLines = MdLayout::countHeadings(doc);
        }
        if self.tocLines == 0 {
            self.bodyPass(doc, 0);
            return;
        }
        self.bodyPass(doc, 1);
        self.knownPages.clear();
        self.knownText.clear();
        self.knownLevel.clear();
        let mut i: i64 = 0;
        let n: i64 = self.headPage.len() as i64;
        while i < n {
            self.knownPages.push(self.headPage[i as usize]);
            self.knownText.push(self.headText[i as usize].clone());
            self.knownLevel.push(self.headLevel[i as usize]);
            i += 1;
        }
        self.boxes.clear();
        self.boxIndexReady = false;
        self.pageIndexReady = false;
        self.resetHeadings();
        self.endScopes();
        self.startScopes();
        self.page = 0;
        self.pageCount = 1;
        self.cursorY = self.contentTop;
        self.bodyPass(doc, 2);
    }
    /// `mode`: 0 no contents, 1 reserve the space, 2 fill it in.
    /// ----------------------------------------------------------------------
    /// The per-block layout cache
    /// ----------------------------------------------------------------------
    /// Turn it on for this document, if this document is one it can serve.
    /// Answers whether it did, so a caller can say so rather than assume.
    pub fn setDocSource(&mut self, text: String) {
        self.docSource = text.clone();
    }
    pub fn armCache(&mut self, doc: &Rc<RefCell<MdNode>>) -> bool {
        self.cacheOn = false;
        if self.paginate {
            return false;
        }
        // a block's boxes depend on the slide it is under, which its own
        // source does not say
        if self.sheet.borrow().hasScoped {
            return false;
        }
        if self.style.borrow().wantToc {
            return false;
        }
        if self.docSource.is_empty() {
            return false;
        }
        if doc.borrow().readingSig != self.cacheReading {
            self.dropCache();
            self.cacheReading = doc.borrow().readingSig.clone();
        }
        self.cacheOn = true;
        true
    }
    pub fn disarmCache(&mut self) {
        self.cacheOn = false;
    }
    /// Everything measured is measured at a width and through a measurer, so a
    /// change to either makes every entry wrong. The cheap and total answer is
    /// to throw the lot away: it costs one document's layout, once.
    pub fn dropCache(&mut self) {
        self.cacheAt = HashMap::new();
        self.cacheKey.clear();
        self.cacheSrc.clear();
        self.cacheTop.clear();
        self.cacheHeight.clear();
        self.cacheFrom.clear();
        self.cacheCount.clear();
        self.cacheBoxes.clear();
        self.cacheHits = 0;
        self.cacheMisses = 0;
    }
    /// The text that produced this block, which is the only thing its boxes
    /// depend on — plus the width it was measured at, because the same
    /// paragraph breaks in different places in a different column.
    pub fn cacheKeyOf(&self, node: &Rc<RefCell<MdNode>>, src: &str, w: f64) -> String {
        let mut a: i64 = node.borrow().srcStart;
        let mut b: i64 = node.borrow().srcEnd;
        let n: i64 = MdText::str_len(&src) as i64;
        if a < 0 {
            a = 0;
        }
        if b > n {
            b = n;
        }
        if b < a {
            b = a;
        }
        let mut key: String = format!("{}{}", ((w * 16.0).floor() as i64).to_string(), MdText::substr(&src, a, b));
        // a footnote's number is decided by the blocks BEFORE this one, so
        // the same text draws another number when one is added above it
        if self.notesDoc.borrow().noteCount() > 0 {
            key = format!("{}{}", key, MdLayout::noteSig(node));
        }
        key
    }
    pub fn cacheFind(&self, key: &str) -> i64 {
        if self.cacheAt.contains_key(key) {
            return self.cacheAt.get(key).cloned().unwrap();
        }
        -1
    }
    /// ONE cached box, put where it goes. Both the replay and the audit go
    /// through this, and that is the point of it being a function.
    ///
    /// It was not, and the two disagreed: the replay shifted a box's source
    /// offset by the DIFFERENCE between where the block was and where it is,
    /// having already stored it relative to where it was — subtracting the base
    /// twice — while the audit re-derived the right answer and passed. So a
    /// block that moved got boxes claiming to come from the wrong part of the
    /// file, the caret's line table came out as [[0,10],[0,16],[0,10],[43,63]],
    /// and Up from the end of the document went to the end of the first line.
    ///
    /// A check that recomputes the answer is not checking the code. It is
    /// checking that two people can do the same arithmetic, and here they could
    /// not.
    pub fn cacheBoxAt(&self, slot: i64, i: i64, node: &Rc<RefCell<MdNode>>, base_y: f64) -> Rc<RefCell<MdBox>> {
        let mut src: Rc<RefCell<MdBox>> = self.cacheBoxes[(self.cacheFrom[slot as usize] + i) as usize].clone();
        let mut b: Rc<RefCell<MdBox>> = src.borrow().copy();
        let y_v = b.borrow().y + base_y;
        b.borrow_mut().y = y_v;
        if b.borrow().srcStart >= 0 {
            let srcStart_v = b.borrow().srcStart + node.borrow().srcStart;
            b.borrow_mut().srcStart = srcStart_v;
        }
        if b.borrow().segSrc >= 0 {
            let segSrc_v = b.borrow().segSrc + node.borrow().srcStart;
            b.borrow_mut().segSrc = segSrc_v;
        }
        b
    }
    /// A hit: the boxes again, moved to where this block now sits and stamped
    /// with where its source now is.
    pub fn cacheReplay(&mut self, slot: i64, node: &Rc<RefCell<MdNode>>) {
        let cnt: i64 = self.cacheCount[slot as usize];
        let mut i: i64 = 0;
        while i < cnt {
            let mut _tmp_2495 = self.cacheBoxAt(slot, i, node, self.cursorY.clone());
            self.boxes.push(_tmp_2495);
            i += 1;
        }
        self.cursorY += self.cacheHeight[slot as usize];
        self.cacheHits += 1;
    }
    pub fn cacheStore(&mut self, key: &str, node: &Rc<RefCell<MdNode>>, from: i64, top: f64) {
        let n: i64 = self.boxes.len() as i64;
        let at: i64 = self.cacheBoxes.len() as i64;
        let mut i: i64 = from;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            let mut keep: Rc<RefCell<MdBox>> = b.borrow().copy();
            let y_v = keep.borrow().y - top;
            keep.borrow_mut().y = y_v;
            if keep.borrow().srcStart >= 0 {
                let srcStart_v = keep.borrow().srcStart - node.borrow().srcStart;
                keep.borrow_mut().srcStart = srcStart_v;
            }
            if keep.borrow().segSrc >= 0 {
                let segSrc_v = keep.borrow().segSrc - node.borrow().srcStart;
                keep.borrow_mut().segSrc = segSrc_v;
            }
            self.cacheBoxes.push(keep.clone());
            i += 1;
        }
        let slot: i64 = self.cacheKey.len() as i64;
        self.cacheKey.push(key.to_string());
        if false == self.cacheAt.contains_key(key) {
            self.cacheAt.insert(key.to_string(), slot);
        }
        // Zero, like `cacheTop`: the boxes were stored RELATIVE to the block,
        // so what they are relative TO is nothing. Storing the block's own
        // start here and subtracting it again on replay is the bug above.
        self.cacheSrc.push(0);
        self.cacheTop.push(0.0);
        self.cacheHeight.push(self.cursorY - top);
        self.cacheFrom.push(at);
        self.cacheCount.push(n - from);
        self.cacheMisses += 1;
    }
    /// One top-level block, from the cache when it can be and from the layout
    /// when it cannot.
    pub fn blockCached(&mut self, node: Rc<RefCell<MdNode>>, src: &str, x: f64, w: f64) {
        // …and never while PAGINATING. A cached block is replayed by copying
        // its boxes and advancing the cursor by its height — `block` is not
        // called, so `reserve` is not called, so the page never breaks. A
        // four-hundred-paragraph document came out as two pages with every
        // paragraph drawn on top of the last.
        //
        // `armCache` refuses a paginating layout, but it is asked BEFORE
        // `layoutPaged` sets `paginate` — so the flag could be left on from
        // the continuous layout before it. Asked here it cannot be.
        if false == self.cacheOn || self.paginate {
            self.block(node.clone(), x, w);
            return;
        }
        if node.borrow().srcStart < 0 {
            self.block(node.clone(), x, w);
            return;
        }
        // A span that is empty does not identify a block, and two blocks that
        // share a key share each other's boxes. That is not hypothetical: a
        // one-line HTML block used to come out as [32,32], so a document with
        // two comments in it drew one of them twice and the other not at all.
        // The parser is fixed and this is the guard that makes the fix
        // unnecessary rather than load-bearing.
        if node.borrow().srcEnd <= node.borrow().srcStart {
            self.block(node.clone(), x, w);
            return;
        }
        let key: String = self.cacheKeyOf(&node, src, w);
        let slot: i64 = self.cacheFind(&key);
        if slot >= 0 {
            if self.cacheAudit {
                self.auditBlock(slot, node.clone(), x, w);
                return;
            }
            self.cacheReplay(slot, &node);
            return;
        }
        let from: i64 = self.boxes.len() as i64;
        let top: f64 = self.cursorY;
        self.block(node.clone(), x, w);
        self.cacheStore(&key, &node, from, top);
    }
    /// Lay the block out for real, then check the cache would have said the
    /// same thing. Slower than either on its own, which is the point: it is a
    /// gate, not a mode anyone ships with.
    pub fn auditBlock(&mut self, slot: i64, node: Rc<RefCell<MdNode>>, x: f64, w: f64) {
        self.cacheAudited += 1;
        let from: i64 = self.boxes.len() as i64;
        let top: f64 = self.cursorY;
        self.block(node.clone(), x, w);
        let real_count: i64 = (self.boxes.len() as i64) - from;
        let real_height: f64 = self.cursorY - top;
        let cnt: i64 = self.cacheCount[slot as usize];
        if real_count != cnt {
            self.cacheFault(format!("box count {} vs cached {} for {}", real_count, cnt, node.borrow().kind).clone());
            return;
        }
        if ((real_height - self.cacheHeight[slot as usize]).abs()) > 0.01 {
            self.cacheFault(format!("height {}", real_height));
            return;
        }
        let mut i: i64 = 0;
        while i < cnt {
            let mut got: Rc<RefCell<MdBox>> = self.boxes[(from + i) as usize].clone();
            // What the CACHE would have handed back — built by the same
            // function the replay uses, not by doing its arithmetic again.
            let mut want: Rc<RefCell<MdBox>> = self.cacheBoxAt(slot, i, &node, top);
            if got.borrow().text != want.borrow().text {
                self.cacheFault(format!("text [{}] vs [{}]", got.borrow().text, want.borrow().text).clone());
                i = cnt;
            } else {
                if ((got.borrow().y - want.borrow().y).abs()) > 0.01 {
                    self.cacheFault(format!("y {}", got.borrow().y).clone());
                    i = cnt;
                } else {
                    if ((got.borrow().x - want.borrow().x).abs()) > 0.01 {
                        self.cacheFault(format!("x {}", got.borrow().x).clone());
                        i = cnt;
                    } else {
                        if got.borrow().segSrc != want.borrow().segSrc {
                            self.cacheFault(format!("segSrc {} vs {}", got.borrow().segSrc, want.borrow().segSrc).clone());
                            i = cnt;
                        } else {
                            if got.borrow().srcStart != want.borrow().srcStart {
                                self.cacheFault(format!("srcStart {} vs {}", got.borrow().srcStart, want.borrow().srcStart).clone());
                                i = cnt;
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn cacheFault(&mut self, what: String) {
        self.cacheFaults += 1;
        if self.cacheFirstFault.is_empty() {
            self.cacheFirstFault = what.clone();
        }
    }
    pub fn bodyPass(&mut self, doc: &Rc<RefCell<MdNode>>, mode: i64) {
        self.floatOn = false;
        self.cornerOn = false;
        self.openPad = 0.0;
        // a full-page gallery ending the last layout must not break this one
        self.fullPageDone = false;
        let mut placed: bool = mode == 0;
        let mut i: i64 = 0;
        let n: i64 = doc.borrow().childCount();
        while i < n {
            let mut child: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
            if self.slides {
                // a `layout=full` gallery filled its last slide edge to edge
                if self.fullPageDone {
                    self.fullPageDone = false;
                    self.nextSlide();
                }
                if child.borrow().kind == "heading" {
                    // a heading closes the section it was in; a sub-heading
                    // opens its own once it has been laid out (below)
                    self.clearSection();
                }
                if self.slideBreakBefore(doc, &child, i) {
                    self.nextSlide();
                    // a sub-heading moved to the next slide is the slide
                    // going on: the title again over it, as any continuation
                    if self.continuesSlide(&child) {
                        self.emitSlideTitle();
                    }
                }
                self.keepWithNext(&child, i, n, doc);
                if child.borrow().kind == "heading" {
                    if child.borrow().level <= self.splitLevel {
                        // `plainText`, not `literal`: a heading's literal is
                        // gone by the time the inline parser has finished with
                        // it — it became the text nodes underneath — so the
                        // continuation slide carried an empty title.
                        self.slideTitle = child.borrow().plainText();
                        self.slideTitleLevel = child.borrow().level;
                        self.slideTitleHidden = MdLayout::hiddenHeading(&child);
                        self.notePlace(&child, MdLayout::bodyFollows(doc, i, self.splitLevel));
                        self.noteAside(doc, i);
                    }
                }
            }
            if child.borrow().kind == "heading" {
                self.enterHeading(&child);
            }
            // Whether this block opens its slide: a heading that does is the
            // slide's title, and what comes right under it is fitted to it
            // (see `fitSlideTitle`) rather than spilled onto a slide of its own.
            let opens_slide: bool = self.slides && self.cursorY - self.contentTop < 0.5;
            let start_page: i64 = self.page;
            let from_box: i64 = self.boxes.len() as i64;
            let from_head: i64 = self.headText.len() as i64;
            self.blockFirstPage.push(self.page);
            self.plateRoomSrc = -1;
            if i == 0 {
                let mut _tmp_2503 = child.borrow().srcStart.clone();
                self.blankLinesIn(-1, _tmp_2503, self.contentX.clone());
            }
            self.placeBlock(doc, i, true);
            let mut up_to: i64 = -1;
            if i + 1 < n {
                let mut nx: Rc<RefCell<MdNode>> = doc.borrow().childAt(i + 1);
                up_to = nx.borrow().srcStart;
            }
            let mut _tmp_2505 = child.borrow().srcEnd.clone();
            self.blankLinesIn(_tmp_2505, up_to, self.contentX.clone());
            if (self.slides && child.borrow().kind == "heading") && child.borrow()
                .level > self.splitLevel {
                self.sectionTitle = child.borrow().plainText();
                self.sectionLevel = child.borrow().level;
                self.sectionHidden = MdLayout::hiddenHeading(&child);
            }
            if self.slides {
                let is_title: bool = opens_slide && child.borrow().kind == "heading";
                if is_title && MdLayout::bodyFollows(doc, i, self.splitLevel.clone()) {
                    self.fitSlideTitle(child.clone(), from_box, from_head, start_page);
                }
                self.slideTitleOnly = is_title && self.page == start_page;
            }
            self.blockLastPage.push(self.page);
            if false == placed {
                // After the title, which is where a reader looks for it; at
                // the very top when the document has no heading at all.
                let after_title: bool = child.borrow().kind == "heading";
                let at_top: bool = child.borrow().kind == "front_matter";
                if after_title || i == 0 {
                    if false == at_top {
                        if mode == 1 {
                            self.cursorY += self.tocHeight();
                        } else {
                            let mut _tmp_2512 = self.contentX.clone();
                            self.emitToc(_tmp_2512, self.contentW.clone());
                        }
                        placed = true;
                    }
                }
            }
            i += 1;
        }
        self.endFloat();
    }
    /// ----------------------------------------------------------------------
    /// Text beside a narrow picture or drawing (FLOAT)
    /// ----------------------------------------------------------------------
    ///
    /// `{width=60%}` under a figure leaves 40% of the column empty, and the
    /// paragraph after it went under it, so the slide was a narrow figure, a
    /// gap and a line of text below. A figure that was given a width (or
    /// that is narrower than the column anyway) and sits at a side of it
    /// now has the paragraphs, lists and quotes after it set in the rest of
    /// the column, beside it, while they start above its bottom — the way a
    /// CSS float wraps. Under the figure the column is full width again.
    /// Block `i` laid out where the body pass puts it: beside the picture,
    /// chart or table before it while that left a float open, else across
    /// the column, with the text after a picture or chart given room under
    /// it. The first pass reuses a block laid out before (`cached`); a slide
    /// laid out again to shrink or grow it (`relayoutBlock`) goes through
    /// here too, so the text that went beside a table there still does.
    pub fn placeBlock(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64, cached: bool) {
        let mut child: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
        let from_box: i64 = self.boxes.len() as i64;
        let mut bx: f64 = self.contentX;
        let mut bw: f64 = self.contentW;
        if self.floatOn {
            if self.page == self.floatPage && MdLayout::besideable(&child, self.splitLevel.clone()) {
                bx = self.floatX;
                bw = self.floatW;
            } else {
                self.endFloat();
            }
        }
        // a slide's title leaves the top corner its picture is pinned in
        if (self.slides && child.borrow().kind == "heading") && child.borrow()
            .level <= self.splitLevel {
            let inset: f64 = self.cornerInset(doc, i);
            if inset > 0.0 {
                bw = self.contentW - inset * self.contentW;
                let mut cn: Rc<RefCell<MdNode>> = self.cornerNode(doc, i);
                if MdSlideLayout::cornerOf(&cn.borrow().attrValue("float")) == "top-left" {
                    bx = self.contentX + inset * self.contentW;
                }
                let gap: f64 = self.style.borrow().bodySize * 1.2;
                bw -= gap;
                if bx > self.contentX {
                    bx += gap;
                }
                self.block(child.clone(), bx, bw);
                self.followH = 0.0;
                return;
            }
        }
        // a slide with a strip at its side: the title above it keeps the
        // full width (noteAside)
        if (self.slideAside && child.borrow().kind == "heading") && child.borrow()
            .level <= self.splitLevel {
            if !self.floatOn {
                let mut _tmp_2515 = self.contentX.clone();
                self.block(child.clone(), _tmp_2515, self.fullContentW.clone());
                self.followH = 0.0;
                return;
            }
        }
        self.followH = 0.0;
        self.followBeside = false;
        if self.slides && child.borrow().kind == "paragraph" {
            let mut __re_gx1: Rc<RefCell<MdNode>> = MdLayout::loneImage(&child);
            if __re_gx1.borrow().kind == "image" {
                self.followH = self.heightAfter(doc, i);
                if i + 1 < doc.borrow().childCount() {
                    let _tmp_2519 = MdLayout::besideable(&doc.borrow().childAt(i + 1), self.splitLevel.clone());
                    self.followBeside = _tmp_2519;
                }
            }
        }
        // a chart or diagram given the room under its heading leaves the
        // text written under it on the slide too (see `embed`)
        if self.slides && child.borrow().kind == "embed" {
            self.followH = self.heightAfter(doc, i);
            if i + 1 < doc.borrow().childCount() {
                let _tmp_2522 = MdLayout::besideable(&doc.borrow().childAt(i + 1), self.splitLevel.clone());
                self.followBeside = _tmp_2522;
            }
        }
        if self.floatOn {
            self.block(child.clone(), bx, bw);
            if self.page != self.floatPage || self.cursorY >= self.floatBottom {
                self.endFloat();
            }
        } else {
            if self.besideCorner(doc, i) {
                let mut _tmp_2523 = self.cornerX.clone();
                self.block(child.clone(), _tmp_2523, self.cornerW.clone());
            } else {
                if cached {
                    let mut _tmp_2525 = self.docSource.clone();
                    let mut _tmp_2526 = self.contentX.clone();
                    self.blockCached(child.clone(), &_tmp_2525, _tmp_2526, self.contentW.clone());
                } else {
                    let mut _tmp_2528 = self.contentX.clone();
                    self.block(child.clone(), _tmp_2528, self.contentW.clone());
                }
                self.startFloat(doc, i, from_box);
            }
        }
        self.followH = 0.0;
    }
    /// Whether block `i` goes beside the picture pinned in a bottom corner of
    /// its slide: text that, set across, would reach down into its room.
    pub fn besideCorner(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64) -> bool {
        if false == (self.slides && (self.paginate && self.cornerOn)) {
            return false;
        }
        if self.page != self.cornerPage {
            return false;
        }
        if false == MdLayout::besideable(&doc.borrow().childAt(i), self.splitLevel.clone()) {
            return false;
        }
        let h: f64 = self.measureUpTo(doc, i, 0, i);
        self.cursorY + h > self.cornerTop
    }
    pub fn startFloat(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64, from_box: i64) {
        // a corner picture opened its own (pinPicture)
        if self.floatOn {
            return;
        }
        if i + 1 >= doc.borrow().childCount() {
            return;
        }
        if false == MdLayout::bodyFollows(doc, i, self.splitLevel.clone()) {
            return;
        }
        let mut child: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
        let mut next: Rc<RefCell<MdNode>> = doc.borrow().childAt(i + 1);
        if false == MdLayout::besideable(&next, self.splitLevel.clone()) {
            return;
        }
        // the block's own picture or drawing, the last one it emitted
        let mut k: i64 = (self.boxes.len() as i64) - 1;
        let mut at: i64 = -1;
        while k >= from_box && at < 0 {
            let mut c: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            // a corner picture is out of the flow: nothing goes beside it
            // here, under the title is where the text goes on (pinPicture)
            if (c.borrow().kind == 3 || c.borrow().kind == 4) && !c.borrow().pinned {
                at = k;
            }
            k -= 1;
        }
        if at < 0 && (child.borrow().kind == "table" || child.borrow().kind == "code_block") {
            let mut _tmp_2534 = self.extentOn(from_box, self.page.clone());
            self.startFloatAt(&child, &_tmp_2534);
            return;
        }
        if at < 0 {
            return;
        }
        let mut b: Rc<RefCell<MdBox>> = self.boxes[at as usize].clone();
        self.startFloatAt(&child, &b);
    }
    /// The boxes from `from` on that are on page `p`, as one box (kind -1
    /// when there are none): a table's cells, the thing beside which text
    /// goes when it was given a width.
    pub fn extentOn(&self, from: i64, p: i64) -> Rc<RefCell<MdBox>> {
        let mut e: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        e.borrow_mut().kind = -1;
        let mut right: f64 = 0.0;
        let mut bottom: f64 = 0.0;
        let mut k: i64 = from;
        while k < (self.boxes.len() as i64) {
            let mut c: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            if c.borrow().page == p && !c.borrow().retold {
                if e.borrow().kind < 0 {
                    e.borrow_mut().kind = 1;
                    e.borrow_mut().page = p;
                    e.borrow_mut().x = c.borrow().x;
                    e.borrow_mut().y = c.borrow().y;
                    right = c.borrow().x + c.borrow().w;
                    bottom = c.borrow().y + c.borrow().h;
                }
                if c.borrow().x < e.borrow().x {
                    e.borrow_mut().x = c.borrow().x;
                }
                if c.borrow().y < e.borrow().y {
                    e.borrow_mut().y = c.borrow().y;
                }
                if c.borrow().x + c.borrow().w > right {
                    right = c.borrow().x + c.borrow().w;
                }
                if c.borrow().y + c.borrow().h > bottom {
                    bottom = c.borrow().y + c.borrow().h;
                }
            }
            k += 1;
        }
        let w_v = right - e.borrow().x;
        e.borrow_mut().w = w_v;
        let h_v = bottom - e.borrow().y;
        e.borrow_mut().h = h_v;
        e
    }
    pub fn startFloatAt(&mut self, child: &Rc<RefCell<MdNode>>, b: &Rc<RefCell<MdBox>>) {
        if b.borrow().kind < 0 {
            return;
        }
        if b.borrow().page != self.page {
            return;
        }
        let side: String = self.floatSide(child, b);
        if side.is_empty() {
            return;
        }
        let gap: f64 = self.style.borrow().bodySize * 1.2;
        let mut fx: f64 = (b.borrow().x + b.borrow().w) + gap;
        let mut fw: f64 = (self.contentX + self.contentW) - fx;
        if side == "end" {
            fx = self.contentX;
            fw = (b.borrow().x - self.contentX) - gap;
        }
        if fw < self.contentW * 0.25 {
            return;
        }
        self.floatOn = true;
        self.floatPage = self.page;
        self.floatBottom = self.cursorY;
        self.floatX = fx;
        self.floatW = fw;
        self.cursorY = b.borrow().y;
    }
    /// Which side of the column box `b` (the drawing block `child` emitted)
    /// sits at, for the text to go to the other: "start", "end", or "" when
    /// it fills the column, is centred, or was not given a width.
    pub fn floatSide(&self, child: &Rc<RefCell<MdNode>>, b: &Rc<RefCell<MdBox>>) -> String {
        let mut want: String = child.borrow().attrValue("width");
        if child.borrow().kind == "paragraph" {
            want = MdLayout::loneImage(child).attrValue("width");
            if want.is_empty() {
                want = child.borrow().attrValue("width");
            }
        }
        if want.is_empty() {
            return String::new();
        }
        if b.borrow().w > self.contentW * 0.75 {
            return String::new();
        }
        if b.borrow().kind == 3 {
            let kind: String = MdLayout::firstWord(&child.borrow().info.clone());
            let ei: i64 = self.embeds.borrow().find(&kind, &child.borrow().literal, b.borrow().w);
            // A drawing's box starts the column whatever its alignment:
            // `center` sets the drawing in the middle of its own box
            // (MdEmbedEntry.shiftIn). So a chart given a width has the text
            // after it beside it even when centred (a host centres every
            // chart by default); it went under it, and a chart with its
            // takeaway in a box beside it ended up on two slides.
            if ei >= 0 {
                let mut e: Rc<RefCell<MdEmbedEntry>> = self.embeds.borrow().entryAt(ei);
                if e.borrow().align == "end" {
                    return "end".to_string();
                }
            }
            return "start".to_string();
        }
        // a picture: at the side it was put, a centred one to the start
        if b.borrow()
            .x + b.borrow()
            .w >= (self.contentX + self.contentW) - 0.5 {
            if b.borrow().x > self.contentX + 0.5 {
                return "end".to_string();
            }
        }
        b.borrow_mut().x = self.contentX;
        "start".to_string()
    }
    pub fn endFloat(&mut self) {
        if (self.floatOn && self.page == self.floatPage) && self.cursorY < self.floatBottom {
            self.cursorY = self.floatBottom;
        }
        self.floatOn = false;
    }
    /// How tall the blocks after `i` that stay on its slide are, laid out
    /// and taken back: the room a picture leaves under itself so that the
    /// text written under it is on the same slide.
    pub fn heightAfter(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64) -> f64 {
        if false == MdLayout::bodyFollows(doc, i, self.splitLevel.clone()) {
            return 0.0;
        }
        self.measureBlocks(doc, i + 1, 0)
    }
    /// How tall the run of blocks from `from` on is, laid out from the top and
    /// taken back: every block that stays on the slide `from` is on, and, when
    /// `level` is above 0, only up to the next heading at `level` or above —
    /// the SECTION a sub-heading at `level` opens (see `sectionMovesOn`).
    /// Nothing it laid out is left behind: boxes, headings, the slide count and
    /// what a full-page gallery noted for the slides it would have opened.
    pub fn measureBlocks(&mut self, doc: &Rc<RefCell<MdNode>>, from: i64, level: i64) -> f64 {
        self.measureUpTo(doc, from, level, -1)
    }
    /// …and stopping after block `last` when it is not -1
    pub fn measureUpTo(&mut self, doc: &Rc<RefCell<MdNode>>, from: i64, level: i64, last: i64) -> f64 {
        let n: i64 = doc.borrow().childCount();
        if from >= n {
            return 0.0;
        }
        let saved_boxes: i64 = self.boxes.len() as i64;
        let saved_y: f64 = self.cursorY;
        let saved_page: i64 = self.page;
        let saved_page_count: i64 = self.pageCount;
        let saved_overflows: i64 = self.slideOverflows;
        let saved_heads: i64 = self.headText.len() as i64;
        let saved_header_off: i64 = self.headerOff.len() as i64;
        let saved_footer_off: i64 = self.footerOff.len() as i64;
        let saved_meta: i64 = self.metaPage.len() as i64;
        let saved_suspect: i64 = self.metaSuspect.len() as i64;
        let saved_named: i64 = self.namedPage.len() as i64;
        let saved_hidden_on: i64 = self.hiddenOnPage;
        let saved_full_done: bool = self.fullPageDone;
        let was_paginate: bool = self.paginate;
        self.paginate = false;
        self.cursorY = 0.0;
        let mut j: i64 = from;
        let mut going: bool = true;
        while going && j < n {
            let mut _tmp_2549 = self.contentX.clone();
            let mut _tmp_2550 = self.contentW.clone();
            self.block(doc.borrow().childAt(j).clone(), _tmp_2549, _tmp_2550);
            going = MdLayout::bodyFollows(doc, j, self.splitLevel);
            if j == last {
                going = false;
            }
            if (going && level > 0) && j + 1 < n {
                let mut next: Rc<RefCell<MdNode>> = doc.borrow().childAt(j + 1);
                if next.borrow().kind == "heading" && next.borrow().level <= level {
                    going = false;
                }
            }
            j += 1;
        }
        let h: f64 = self.cursorY;
        self.paginate = was_paginate;
        self.truncate(saved_boxes);
        while (self.headText.len() as i64) > saved_heads {
            self.headText.pop();
            self.headLevel.pop();
            self.headPage.pop();
            self.headY.pop();
        }
        while (self.headerOff.len() as i64) > saved_header_off {
            self.headerOff.pop();
        }
        while (self.metaPage.len() as i64) > saved_meta {
            self.metaPage.pop();
            self.metaKey.pop();
            self.metaVal.pop();
        }
        while (self.metaSuspect.len() as i64) > saved_suspect {
            self.metaSuspect.pop();
        }
        while (self.footerOff.len() as i64) > saved_footer_off {
            self.footerOff.pop();
        }
        while (self.namedPage.len() as i64) > saved_named {
            self.namedPage.pop();
            self.namedText.pop();
        }
        self.hiddenOnPage = saved_hidden_on;
        self.fullPageDone = saved_full_done;
        self.cursorY = saved_y;
        self.page = saved_page;
        self.pageCount = saved_page_count;
        self.slideOverflows = saved_overflows;
        h
    }
    pub fn fitSlideTitle(&mut self, child: Rc<RefCell<MdNode>>, from_box: i64, from_head: i64, start_page: i64) {
        let mut cap: f64 = (self.contentBottom - self.contentTop) * self.slideTitleMax;
        // the cap is the text's: a title on a plate (`.cover { padding-top:
        // 200pt }`) keeps its padding at every size, so that room is not
        // counted, and a padding taller than the slide is no reason to set
        // the text smaller — it would only shrink to nothing
        let mut fixed: f64 = 0.0;
        if self.plateRoomSrc == child.borrow().srcStart {
            fixed = self.plateRoom;
        }
        if fixed >= self.contentBottom - self.contentTop {
            return;
        }
        cap += fixed;
        if self.page == start_page && self.cursorY - self.contentTop <= cap {
            return;
        }
        let mut saved: Vec<f64> = self.styleNumbers();
        let saved_overflows: i64 = self.slideOverflows;
        let mut sc: f64 = 0.9;
        while sc > 0.15 {
            self.truncate(from_box);
            while (self.headText.len() as i64) > from_head {
                self.headText.pop();
                self.headLevel.pop();
                self.headPage.pop();
                self.headY.pop();
            }
            self.scaleStyle(&saved, sc);
            self.page = start_page;
            self.pageCount = self.page + 1;
            self.forgetPagesAfter(self.page.clone());
            self.slideOverflows = saved_overflows;
            self.cursorY = self.contentTop;
            let mut _tmp_2554 = self.contentX.clone();
            self.block(child.clone(), _tmp_2554, self.contentW.clone());
            self.restoreStyle(&saved);
            if self.page == start_page && self.cursorY - self.contentTop <= cap {
                return;
            }
            sc -= 0.1;
        }
    }
    /// ----------------------------------------------------------------------
    /// …and the four rules, in order
    /// ----------------------------------------------------------------------
    pub fn slideBreakBefore(&mut self, doc: &Rc<RefCell<MdNode>>, child: &Rc<RefCell<MdNode>>, i: i64) -> bool {
        // Never before the first block, and never on a slide with nothing on
        // it yet — a rule that fires twice in a row leaves a blank slide.
        if i == 0 {
            return false;
        }
        if self.cursorY - self.contentTop < 0.5 && self.hiddenOnPage != self.page {
            return false;
        }
        // 2. FORBIDDEN beats 1. MANDATORY. An author who wrote `{.no-break}`
        // on a heading meant it, and the alternative is a flag that does
        // nothing whenever it would matter.
        if child.borrow().hasClass("no-break") {
            return false;
        }
        // 1. MANDATORY
        if child.borrow().hasClass("slide") {
            return true;
        }
        if child.borrow().kind == "heading" {
            if child.borrow().level <= self.splitLevel {
                return true;
            }
            // 3. PREFERRED — any other heading, when the slide has to break
            // anyway: see `sectionMovesOn`.
            return self.sectionMovesOn(doc, i);
        }
        false
    }
    /// 3. PREFERRED, in full. A sub-heading opens a SECTION: it and what
    /// follows, up to the next heading at its level or above (or the next
    /// slide's title). When the section fits in the room left, nothing
    /// breaks — a slide is cut where it has to be, not wherever a heading
    /// happens to fall below the middle (that rule sent the last line of
    /// `### h3` … `###### h6` to a slide of its own with half the slide
    /// empty). When it does not fit, the slide is going to break somewhere,
    /// and the place to break is before the heading: the section starts at
    /// the top of the continuation, under the title, whole. That holds only
    /// when the section fits on a continuation of its own; one taller than
    /// that is cut wherever it runs over, as before, and the continuation
    /// says its heading again (`emitSlideTitle`). A slide that holds only
    /// its title keeps the section: moving it would leave the title alone.
    pub fn sectionMovesOn(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64) -> bool {
        if self.slideTitleOnly {
            return false;
        }
        let mut child: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
        let h: f64 = self.measureBlocks(doc, i, child.borrow().level.clone());
        if self.cursorY + h <= self.contentBottom + self.fitSlack {
            return false;
        }
        let alone: f64 = (self.contentBottom - self.contentTop) - self.continuationTitleHeight(false);
        h <= alone
    }
    /// Whether a slide `child` was moved onto by a break is the same slide
    /// going on — a sub-heading, which says the title again over it — rather
    /// than a slide of its own: a title, or a block the author marked
    /// `{.slide}`, which asked for a new slide and gets one as written.
    pub fn continuesSlide(&self, child: &Rc<RefCell<MdNode>>) -> bool {
        if child.borrow().kind != "heading" {
            return false;
        }
        if child.borrow().level <= self.splitLevel {
            return false;
        }
        if child.borrow().hasClass("slide") {
            return false;
        }
        true
    }
    /// KEEP-WITH-NEXT. A heading with nothing under it is the one thing that
    /// makes an automatic deck look automatic, so the heading moves rather than
    /// the paragraph. It is asked for the PAIR: the heading's own height plus
    /// one line of whatever follows.
    pub fn keepWithNext(&mut self, child: &Rc<RefCell<MdNode>>, i: i64, n: i64, doc: &Rc<RefCell<MdNode>>) {
        if child.borrow().kind != "heading" {
            return;
        }
        if MdLayout::hiddenHeading(child) {
            return;
        }
        if i + 1 >= n {
            return;
        }
        let mut next: Rc<RefCell<MdNode>> = doc.borrow().childAt(i + 1);
        let size: f64 = self.style.borrow().headingSize(child.borrow().level.clone());
        let head: f64 = (self.style.borrow().headingSpaceBefore + size * self.style.borrow().lineHeight) + self.style.borrow()
            .headingSpaceAfter;
        let need: f64 = head + self.keepRoom(&next);
        if self.cursorY + need > self.contentBottom {
            if self.cursorY - self.contentTop > 0.5 {
                self.nextSlide();
                if self.continuesSlide(child) {
                    self.emitSlideTitle();
                }
            }
        }
    }
    /// How much of the next block has to fit beside the heading for the pair to
    /// count as together.
    ///
    /// One body line is right for PROSE: a paragraph that starts under its
    /// heading and runs onto the next slide still reads as one thing. It is
    /// wrong for a block that cannot be cut. A diagram is laid out whole or not
    /// at all, so a heading with one line of room under it takes the heading
    /// and leaves the diagram on the next slide — which is the reader's
    /// complaint, in as many words: the heading on one slide and the diagram on
    /// the next.
    ///
    /// The diagram's height is already known: `MdDiagram.prepare` measured it
    /// before layout began, which is the whole reason that pass exists. Capped
    /// at the room a slide has, because a diagram taller than any slide would
    /// otherwise push every heading forward for ever and still not fit.
    ///
    /// A TABLE is the other unsplittable-ish block and still gets one line. Its
    /// height is not known without laying its rows out, and doing that here
    /// would be a second table algorithm beside `tableWidths`. Named rather
    /// than left as an oversight.
    pub fn keepRoom(&self, next: &Rc<RefCell<MdNode>>) -> f64 {
        let line: f64 = self.style.borrow()
            .bodySize * self.style.borrow()
            .lineHeight;
        if next.borrow().kind != "embed" {
            return line;
        }
        let kind: String = MdLayout::firstWord(&next.borrow().info.clone());
        let want: String = next.borrow().attrValue("width");
        let use_w: f64 = MdEmbedKinds::widthOf(&want, self.contentW.clone());
        let known: f64 = self.embeds.borrow().heightFor(&kind, &next.borrow().literal.clone(), use_w);
        if known <= 0.0 {
            return line;
        }
        let room: f64 = self.contentBottom - self.contentTop;
        if known > room {
            return room;
        }
        known
    }
    pub fn nextSlide(&mut self) {
        self.page += 1;
        self.pageCount = self.page + 1;
        self.cursorY = self.contentTop;
    }
    /// The title again, at the top of a slide a block spilled onto. Emitted
    /// here rather than through `blockInner`, because there is no node for it:
    /// it is the same heading, said twice.
    ///
    /// THE RULE, one for every continuation: the slide's title is said again
    /// at the top, whether the continuation begins with a block cut in two,
    /// a whole block or a sub-heading the break was moved before. When it
    /// begins INSIDE a section, that section's sub-heading is said again
    /// under the title, set and marked as the title is — its own level's
    /// size and ink, no box of the source behind it (`segSrc` -1), and no
    /// "(continued)" suffix, which the deck would have to translate.
    pub fn emitSlideTitle(&mut self) {
        self.noteContinuation(self.page.clone());
        let mut said: bool = false;
        if !self.slideTitle.is_empty() && !self.slideTitleHidden {
            let mut _tmp_2566 = self.slideTitle.clone();
            self.emitRepeatedHeading(&_tmp_2566, self.slideTitleLevel.clone());
            said = true;
        }
        if !self.sectionTitle.is_empty() && !self.sectionHidden {
            if said {
                self.cursorY += self.style.borrow().headingSpaceBefore;
            }
            let mut _tmp_2568 = self.sectionTitle.clone();
            self.emitRepeatedHeading(&_tmp_2568, self.sectionLevel.clone());
        }
        self.cursorY += self.openPad;
    }
    /// Wrapped to the content width and fitted as `heading` fits it: a title
    /// said again is no narrower than the first time.
    pub fn emitRepeatedHeading(&mut self, text: &str, level: i64) {
        let mut lines: Vec<Rc<RefCell<MdLine>>> = self.retoldLines(text.to_string(),
            level
        );
        for ln in lines.iter().cloned() {
            let mut t: String = String::new();
            let mut size: f64 = self.style.borrow().headingSize(level);
            let __it_seg = ln.borrow().segs.clone();
            for seg in __it_seg.iter().cloned() {
                t = format!("{}{}", t, seg.borrow().text);
                size = seg.borrow().size;
            }
            let mut _tmp_2571 = self.contentX.clone();
            let mut _tmp_2572 = self.cursorY.clone();
            let mut _tmp_2573 = self.contentW.clone();
            let mut _tmp_2574 = ln.borrow().height.clone();
            let mut b: Rc<RefCell<MdBox>> = MdBox::textBox(
                t.clone(),
                _tmp_2571,
                _tmp_2572,
                _tmp_2573,
                _tmp_2574
            );
            b.borrow_mut().family = self.style.borrow().headingFamily.clone();
            b.borrow_mut().size = size;
            b.borrow_mut().bold = true;
            // the heading's own ink: it is the same title, and a dimmed one
            // is text a reader at the back cannot make out
            b.borrow_mut().color = self.style.borrow().headingInk(level);
            b.borrow_mut().retold = true;
            self.emit(b.clone());
            self.cursorY += ln.borrow().height;
        }
        self.cursorY += self.style.borrow().headingSpaceAfter;
    }
    pub fn retoldLines(&mut self, text: String, level: i64) -> Vec<Rc<RefCell<MdLine>>> {
        let size: f64 = self.style.borrow().headingSize(level);
        let mut r: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        r.borrow_mut().text = text.clone();
        r.borrow_mut().family = self.style.borrow().headingFamily.clone();
        r.borrow_mut().size = size;
        r.borrow_mut().bold = true;
        let mut runs: Vec<Rc<RefCell<MdRun>>> = Vec::new();
        runs.push(r.clone());
        let _tmp_2581 = self.wordFitSize(&runs, size, self.contentW.clone());
        r.borrow_mut().size = _tmp_2581;
        self.breakRuns(&runs, self.contentW.clone())
    }
    pub fn retoldHeight(&mut self, text: &str, level: i64) -> f64 {
        let mut h: f64 = 0.0;
        let __n_li = self.retoldLines(text.to_string(), level).len() as i64;
        for li in 0..__n_li {
            let mut ln = self.retoldLines(text.to_string(), level)[li as usize].clone();
            h += ln.borrow().height;
        }
        h
    }
    /// How tall what `emitSlideTitle` says is, with the section's heading or
    /// without it: the room a continuation has is the slide less this.
    pub fn continuationTitleHeight(&mut self, with_section: bool) -> f64 {
        let mut h: f64 = 0.0;
        let mut said: bool = false;
        if !self.slideTitle.is_empty() && !self.slideTitleHidden {
            let st: String = self.slideTitle.clone();
            h = self.retoldHeight(&st, self.slideTitleLevel) + self.style.borrow()
                .headingSpaceAfter;
            said = true;
        }
        if (with_section && !self.sectionTitle.is_empty()) && !self.sectionHidden {
            if said {
                h += self.style.borrow().headingSpaceBefore;
            }
            let sct: String = self.sectionTitle.clone();
            h = (h + self.retoldHeight(&sct, self.sectionLevel)) + self.style.borrow()
                .headingSpaceAfter;
        }
        h
    }
    pub fn noteContinuation(&mut self, p: i64) {
        let mut t: String = String::new();
        if !self.slideTitleHidden {
            t = self.slideTitle.clone();
        }
        let mut st: String = String::new();
        if !self.sectionHidden {
            st = self.sectionTitle.clone();
        }
        let i: i64 = self.contPage.iter()
            .position( |__r| __r.clone() == p.clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        if i < 0 {
            self.contPage.push(p);
            self.contTitle.push(t.clone());
            self.contTitleLevel.push(self.slideTitleLevel);
            self.contSection.push(st.clone());
            self.contSectionLevel.push(self.sectionLevel);
            return;
        }
        self.contTitle[i as usize] = t.clone();
        self.contTitleLevel[i as usize] = self.slideTitleLevel;
        self.contSection[i as usize] = st.clone();
        self.contSectionLevel[i as usize] = self.sectionLevel;
    }
    /// Slide `p` laid out again from its top: what it said again as a
    /// continuation, said again, and the title and section it was in made the
    /// current ones for what follows on it.
    pub fn resumeContinuation(&mut self, p: i64) {
        let i: i64 = self.contPage.iter()
            .position( |__r| __r.clone() == p.clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        if i < 0 {
            return;
        }
        self.slideTitle = self.contTitle[i as usize].clone();
        self.slideTitleLevel = self.contTitleLevel[i as usize];
        self.slideTitleHidden = false;
        self.sectionTitle = self.contSection[i as usize].clone();
        self.sectionLevel = self.contSectionLevel[i as usize];
        self.sectionHidden = false;
        self.emitSlideTitle();
    }
    /// The continuation records as they are, to put back after a trial
    /// layout (`shrinkSlide`, `growSlide`) — copies, so one kept record can be
    /// put back after each of several trials.
    pub fn takeContinuations(&self) -> Rc<RefCell<MdContinuations>> {
        let mut c: Rc<RefCell<MdContinuations>> = Rc::new(RefCell::new(MdContinuations::new()));
        let _tmp_2588 = MdContinuations::ints(&self.contPage.clone());
        c.borrow_mut().page = _tmp_2588;
        let _tmp_2590 = MdContinuations::strs(&self.contTitle.clone());
        c.borrow_mut().title = _tmp_2590;
        let _tmp_2592 = MdContinuations::ints(&self.contTitleLevel.clone());
        c.borrow_mut().titleLevel = _tmp_2592;
        let _tmp_2594 = MdContinuations::strs(&self.contSection.clone());
        c.borrow_mut().section = _tmp_2594;
        let _tmp_2596 = MdContinuations::ints(&self.contSectionLevel.clone());
        c.borrow_mut().sectionLevel = _tmp_2596;
        c
    }
    pub fn putContinuations(&mut self, c: &Rc<RefCell<MdContinuations>>) {
        let _tmp_2598 = MdContinuations::ints(&c.borrow().page.clone());
        self.contPage = _tmp_2598;
        let _tmp_2600 = MdContinuations::strs(&c.borrow().title.clone());
        self.contTitle = _tmp_2600;
        let _tmp_2602 = MdContinuations::ints(&c.borrow().titleLevel.clone());
        self.contTitleLevel = _tmp_2602;
        let _tmp_2604 = MdContinuations::strs(&c.borrow().section.clone());
        self.contSection = _tmp_2604;
        let _tmp_2606 = MdContinuations::ints(&c.borrow().sectionLevel.clone());
        self.contSectionLevel = _tmp_2606;
    }
    /// Slide `p` went (shrunk into the one before it): its record goes and
    /// every later slide's moves up one.
    /// A layout taken back to page `p` (a block tried on the next slide and
    /// laid out again where it was): the slides after it it had opened are
    /// gone, and so are their notes. A note left behind made the next slide
    /// that came to have that number a continuation, its title said again
    /// over that slide's own (resumeContinuation).
    pub fn forgetPagesAfter(&mut self, p: i64) {
        let mut keep_p: Vec<i64> = Vec::new();
        let mut keep_t: Vec<String> = Vec::new();
        let mut keep_tl: Vec<i64> = Vec::new();
        let mut keep_s: Vec<String> = Vec::new();
        let mut keep_sl: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < (self.contPage.len() as i64) {
            if self.contPage[i as usize] <= p {
                keep_p.push(self.contPage[i as usize]);
                keep_t.push(self.contTitle[i as usize].clone());
                keep_tl.push(self.contTitleLevel[i as usize]);
                keep_s.push(self.contSection[i as usize].clone());
                keep_sl.push(self.contSectionLevel[i as usize]);
            }
            i += 1;
        }
        self.contPage = keep_p.clone();
        self.contTitle = keep_t.clone();
        self.contTitleLevel = keep_tl.clone();
        self.contSection = keep_s.clone();
        self.contSectionLevel = keep_sl.clone();
    }
    pub fn dropContinuation(&mut self, p: i64) {
        let mut keep_p: Vec<i64> = Vec::new();
        let mut keep_t: Vec<String> = Vec::new();
        let mut keep_tl: Vec<i64> = Vec::new();
        let mut keep_s: Vec<String> = Vec::new();
        let mut keep_sl: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < (self.contPage.len() as i64) {
            let mut cp: i64 = self.contPage[i as usize];
            if cp != p {
                if cp > p {
                    cp -= 1;
                }
                keep_p.push(cp);
                keep_t.push(self.contTitle[i as usize].clone());
                keep_tl.push(self.contTitleLevel[i as usize]);
                keep_s.push(self.contSection[i as usize].clone());
                keep_sl.push(self.contSectionLevel[i as usize]);
            }
            i += 1;
        }
        self.contPage = keep_p.clone();
        self.contTitle = keep_t.clone();
        self.contTitleLevel = keep_tl.clone();
        self.contSection = keep_s.clone();
        self.contSectionLevel = keep_sl.clone();
    }
    pub fn clearSection(&mut self) {
        self.sectionTitle = String::new();
        self.sectionLevel = 0;
        self.sectionHidden = false;
    }
    pub fn resetHeadings(&mut self) {
        self.hiddenSrc.clear();
        self.hiddenPage.clear();
        self.namedPage.clear();
        self.namedText.clear();
        self.overlaySrc.clear();
        self.overlayPage.clear();
        self.overlayPath.clear();
        self.hiddenOnPage = -1;
        self.headText.clear();
        self.headLevel.clear();
        self.headPage.clear();
        self.headY.clear();
    }
    pub fn noteHidden(&mut self, src: i64, p: i64) {
        let mut i: i64 = 0;
        while i < (self.hiddenSrc.len() as i64) {
            if self.hiddenSrc[i as usize] == src {
                self.hiddenPage[i as usize] = p;
                return;
            }
            i += 1;
        }
        self.hiddenSrc.push(src);
        self.hiddenPage.push(p);
    }
    /// An overlay picture written at `src` is on page `p`. A slide laid out
    /// again (shrunk to fit) notes it again: the last word holds.
    pub fn noteOverlay(&mut self, src: i64, path: &str, p: i64) {
        let mut i: i64 = 0;
        while i < (self.overlaySrc.len() as i64) {
            if self.overlaySrc[i as usize] == src {
                self.overlayPage[i as usize] = p;
                self.overlayPath[i as usize] = path.to_string();
                return;
            }
            i += 1;
        }
        self.overlaySrc.push(src);
        self.overlayPage.push(p);
        self.overlayPath.push(path.to_string());
    }
    pub fn isOverlayType(&self, content_type: &str) -> bool {
        (self.overlayTypes.iter().position( |__r| __r.clone() == content_type.clone() ).map(|__i| __i as i64).unwrap_or(-1)) >= 0
    }
    /// The page a hidden heading starting at `src` names, -1: none does.
    /// A full-page gallery's picture is a slide of its own with no heading:
    /// it is named by its caption, else the last title and its number
    /// ("Hietaniemi (2)"), so overviews and reports can call it something.
    pub fn fullPageName(&self, it: &Rc<RefCell<MdGalleryItem>>, k: i64) -> String {
        let mut cap: String = it.borrow().caption.trim().to_string();
        if it.borrow().isText {
            cap = it.borrow().text.trim().to_string();
            let nl: i64 = MdText::index_of(&cap, "\n");
            if nl >= 0 {
                cap = MdText::substr(&cap, 0, nl).trim().to_string();
            }
        }
        if !cap.is_empty() {
            return cap;
        }
        let mut last: String = String::new();
        let n: i64 = self.headText.len() as i64;
        if n > 0 {
            last = self.headText[(n - 1) as usize].clone();
        }
        if last.is_empty() {
            return String::new();
        }
        format!("{} ({})", last, k + 1)
    }
    pub fn namePage(&mut self, p: i64, name: &str) {
        if name.is_empty() {
            return;
        }
        let mut i: i64 = 0;
        while i < (self.namedPage.len() as i64) {
            if self.namedPage[i as usize] == p {
                return;
            }
            i += 1;
        }
        self.namedPage.push(p);
        self.namedText.push(name.to_string());
    }
    /// The name a page was given without a heading (a full-page gallery's
    /// picture), or "".
    pub fn pageName(&self, p: i64) -> String {
        let mut i: i64 = 0;
        while i < (self.namedPage.len() as i64) {
            if self.namedPage[i as usize] == p {
                return self.namedText[i as usize].clone();
            }
            i += 1;
        }
        String::new()
    }
    pub fn pageOfHidden(&self, src: i64) -> i64 {
        let mut i: i64 = 0;
        while i < (self.hiddenSrc.len() as i64) {
            if self.hiddenSrc[i as usize] == src {
                return self.hiddenPage[i as usize];
            }
            i += 1;
        }
        -1
    }
    pub fn reserve(&mut self, h: f64) -> f64 {
        if self.paginate {
            // the room the page's footnotes have claimed is not the text's
            let bottom: f64 = self.contentBottom - self.noteRoomOn(self.page.clone());
            // past the budget the rest stays on the last page: a block taller
            // than any page is drawn cut, not spread over thousands
            if self.cursorY + h > bottom + self.fitSlack && self.page < MdCssLimits::pageBudget() {
                self.page += 1;
                self.pageCount = self.page + 1;
                self.cursorY = self.contentTop;
                // A block that did not fit is a block cut in half, and on a
                // deck that is worth saying out loud — and worth repeating the
                // title for, so a reader of the second half knows what it is
                // the second half of.
                if self.slides {
                    self.slideOverflows += 1;
                    // a later column running onto the slide an earlier one
                    // already ran onto: under the title said there, once
                    if self.columnsOn > 0 && self.page == self.colSpillPage {
                        self.cursorY = self.colSpillTop + self.openPad;
                    } else {
                        self.emitSlideTitle();
                        if self.columnsOn > 0 {
                            self.colSpillPage = self.page;
                            self.colSpillTop = self.cursorY - self.openPad;
                        }
                    }
                }
            }
        }
        self.cursorY
    }
    pub fn emit(&mut self, b: Rc<RefCell<MdBox>>) {
        b.borrow_mut().page = self.page;
        b.borrow_mut().srcStart = self.currentSrc;
        self.boxes.push(b.clone());
        if b.borrow().noteRef > 0 && false == self.inNotes {
            let mut _tmp_2613 = b.borrow().noteRef.clone();
            self.noteSeen(_tmp_2613, self.page.clone());
        }
    }
    /// The first box a block produced, or -1. The two panes of the viewer are
    /// kept in step through this and nothing else.
    ///
    /// It was a scan of every box in the document, once per call, and the
    /// viewer calls it once per BLOCK — so putting the source caret back after
    /// a click was quadratic in the document. Nobody noticed at a click a
    /// second; a caret that moves on every keystroke would have. The index is
    /// built once per layout, at the end, keyed by the block's own srcStart.
    pub fn buildBoxIndex(&mut self) {
        self.boxIndexAt = HashMap::new();
        let mut i: i64 = 0;
        let n: i64 = self.boxes.len() as i64;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b.borrow().srcStart >= 0 {
                let k: String = b.borrow().srcStart.to_string();
                if false == self.boxIndexAt.contains_key(&k) {
                    self.boxIndexAt.insert(k.clone(), i);
                }
            }
            i += 1;
        }
        self.boxIndexReady = true;
    }
    pub fn forgetPages(&mut self) {
        self.pageIndexReady = false;
    }
    pub fn buildPageIndex(&mut self) {
        self.pageOrder.clear();
        self.pageFrom.clear();
        let n: i64 = self.boxes.len() as i64;
        let mut last: i64 = -1;
        let mut i: i64 = 0;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b.borrow().page > last {
                last = b.borrow().page;
            }
            i += 1;
        }
        // pageFrom[p] .. pageFrom[p + 1]: page p's run in pageOrder
        let mut p: i64 = 0;
        while p <= last + 1 {
            self.pageFrom.push(0);
            p += 1;
        }
        i = 0;
        while i < n {
            let mut b2: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b2.borrow().page >= 0 {
                let mut _tmp_2615 = b2.borrow().page + 1;
                self.pageFrom[_tmp_2615 as usize] = self.pageFrom[(b2.borrow().page + 1) as usize] + 1;
                self.pageOrder.push(0);
            }
            i += 1;
        }
        p = 1;
        while p <= last + 1 {
            self.pageFrom[p as usize] = self.pageFrom[p as usize] + self.pageFrom[(p - 1) as usize];
            p += 1;
        }
        let mut at: Vec<i64> = Vec::new();
        p = 0;
        while p <= last {
            at.push(self.pageFrom[p as usize]);
            p += 1;
        }
        i = 0;
        while i < n {
            let mut b3: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b3.borrow().page >= 0 {
                let k: i64 = at[(b3.borrow().page) as usize];
                self.pageOrder[k as usize] = i;
                at[(b3.borrow().page) as usize] = k + 1;
            }
            i += 1;
        }
        self.pageIndexBoxes = n;
        self.pageIndexReady = true;
    }
    /// the indexes in `boxes` of the boxes on page `pg`, in box order
    pub fn boxesOnPage(&mut self, pg: i64) -> Vec<i64> {
        if false == self.pageIndexReady || self.pageIndexBoxes != (self.boxes.len() as i64) {
            self.buildPageIndex();
        }
        let mut out: Vec<i64> = Vec::new();
        if pg < 0 || pg + 1 >= (self.pageFrom.len() as i64) {
            return out;
        }
        let mut k: i64 = self.pageFrom[pg as usize];
        let end: i64 = self.pageFrom[(pg + 1) as usize];
        while k < end {
            out.push(self.pageOrder[k as usize]);
            k += 1;
        }
        out
    }
    pub fn firstBoxOf(&mut self, src_start: i64) -> i64 {
        if false == self.boxIndexReady {
            self.buildBoxIndex();
        }
        let k: String = src_start.to_string();
        if self.boxIndexAt.contains_key(&k) {
            return self.boxIndexAt.get(&k).cloned().unwrap();
        }
        -1
    }
    /// ----------------------------------------------------------------------
    /// Footnotes
    /// ----------------------------------------------------------------------
    ///
    /// A reference is a raised number in the line (`collectRuns`). The notes
    /// themselves go where a printed page puts them: at the foot of the page
    /// — the slide — that first refers to them, small, numbered, under a short
    /// rule. A definition draws nothing where it was written.
    ///
    /// Two halves, because a note's place is only known once the page is:
    ///
    ///   while laying out, a line that first refers to a note reserves the
    ///   note's height at the foot of its page with it (`noteRoomFor`), and
    ///   once placed claims it (`emit` → `noteSeen`), so the text after it
    ///   breaks earlier and never runs into the notes;
    ///
    ///   after the pages are final — shrunk, grown — the notes are drawn
    ///   (`placeNotesPaged`) from the boxes alone: whatever page the first
    ///   reference's box ended up on is the page its note is on. The claims
    ///   are bookkeeping for the breaks; the boxes are the truth.
    ///
    /// Without pages (`layoutContinuous`, one long column) there is no foot to
    /// put them at, so they close the document, in number order, as the HTML
    /// export ends with them.
    ///
    /// The notes' boxes are `furniture` 3: placed by the page, not by the flow
    /// of blocks, which is what every reader of a block's boxes already
    /// leaves aside. A labelled note's boxes carry its definition's source, so
    /// a click on one finds the text that is drawn; an inline note's carry
    /// none, because its text is inside the paragraph that refers to it and a
    /// box there would claim to be part of that paragraph.
    pub fn startNotes(&mut self, doc: Rc<RefCell<MdNode>>) {
        self.notesDoc = doc.clone();
        self.noteFirstPage.clear();
        self.noteRoom.clear();
        let mut i: i64 = 0;
        while i < doc.borrow().noteCount() {
            self.noteFirstPage.push(-1);
            i += 1;
        }
        self.inNotes = false;
    }
    pub fn noteRoomOn(&self, p: i64) -> f64 {
        if p < 0 || p >= (self.noteRoom.len() as i64) {
            return 0.0;
        }
        self.noteRoom[p as usize]
    }
    pub fn addNoteRoom(&mut self, p: i64, h: f64) {
        while (self.noteRoom.len() as i64) <= p {
            self.noteRoom.push(0.0);
        }
        self.noteRoom[p as usize] = self.noteRoom[p as usize] + h;
    }
    /// the gap from the text to the notes' rule, and from the rule to them
    pub fn noteSepH(&self) -> f64 {
        self.style.borrow().bodySize * 0.9
    }
    /// Footnote `n` is referred to on page `p`: the first time, it claims its
    /// room there.
    pub fn noteSeen(&mut self, n: i64, p: i64) {
        if n < 1 || n > (self.noteFirstPage.len() as i64) {
            return;
        }
        if self.noteFirstPage[(n - 1) as usize] >= 0 {
            return;
        }
        self.noteFirstPage[(n - 1) as usize] = p;
        let mut add: f64 = self.noteHeight(n);
        if self.noteRoomOn(p) <= 0.0 {
            add += self.noteSepH();
        }
        self.addNoteRoom(p, add);
    }
    /// The room the notes a line refers to for the first time will claim.
    pub fn noteRoomFor(&mut self, ln: &Rc<RefCell<MdLine>>) -> f64 {
        if false == self.paginate || self.inNotes {
            return 0.0;
        }
        if self.noteFirstPage.is_empty() {
            return 0.0;
        }
        let mut sum: f64 = 0.0;
        let mut counted: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < (ln.borrow().segs.len() as i64) {
            let mut seg: Rc<RefCell<MdSeg>> = ln.borrow().segs[i as usize].clone();
            let n: i64 = seg.borrow().noteRef;
            if (n >= 1) && (n <= self.noteFirstPage.len() as i64) {
                if self.noteFirstPage[(n - 1) as usize] < 0 {
                    let mut seen: bool = false;
                    for cn in counted.iter().copied() {
                        if cn == n {
                            seen = true;
                        }
                    }
                    if false == seen {
                        counted.push(n);
                        sum += self.noteHeight(n);
                    }
                }
            }
            i += 1;
        }
        if sum > 0.0 && self.noteRoomOn(self.page.clone()) <= 0.0 {
            sum += self.noteSepH();
        }
        sum
    }
    /// The claims again, from the boxes that are left — after a slide was cut
    /// back to be laid out again.
    pub fn retrackNotes(&mut self) {
        let mut i: i64 = 0;
        while i < (self.noteFirstPage.len() as i64) {
            self.noteFirstPage[i as usize] = -1;
            i += 1;
        }
        self.noteRoom.clear();
        let mut k: i64 = 0;
        while k < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            if b.borrow().noteRef > 0 && b.borrow().furniture == 0 {
                let mut _tmp_2620 = b.borrow().noteRef.clone();
                self.noteSeen(_tmp_2620, b.borrow().page.clone());
            }
            k += 1;
        }
    }
    /// How tall footnote `n` is, set as the notes are, in the text column.
    pub fn noteHeight(&mut self, n: i64) -> f64 {
        let key: String = format!("{}/{}/{}", n.to_string(), self.style.borrow().bodySize, self.contentW);
        if self.noteHeights.contains_key(&key) {
            return self.noteHeights.get(&key).cloned().unwrap();
        }
        let mut one: Vec<i64> = Vec::new();
        one.push(n);
        let mut _tmp_2622 = self.contentX.clone();
        let h: f64 = self.measureNotes(&one, _tmp_2622, self.contentW.clone());
        self.noteHeights.insert(key.clone(), h);
        h
    }
    /// The notes laid out for real and taken away again: their height.
    pub fn measureNotes(&mut self, list: &[i64], x: f64, w: f64) -> f64 {
        let saved_paginate: bool = self.paginate;
        let saved_page: i64 = self.page;
        let saved_count: i64 = self.pageCount;
        let saved_cursor: f64 = self.cursorY;
        let saved_in: bool = self.inNotes;
        let saved_src: i64 = self.currentSrc;
        let from: i64 = self.boxes.len() as i64;
        self.paginate = false;
        self.inNotes = true;
        self.cursorY = 0.0;
        self.drawNoteList(list, x, w);
        let mut bottom: f64 = 0.0;
        let mut i: i64 = from;
        while i < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b.borrow().y + b.borrow().h > bottom {
                bottom = b.borrow().y + b.borrow().h;
            }
            i += 1;
        }
        while (self.boxes.len() as i64) > from {
            self.boxes.pop();
        }
        self.paginate = saved_paginate;
        self.page = saved_page;
        self.pageCount = saved_count;
        self.cursorY = saved_cursor;
        self.inNotes = saved_in;
        self.currentSrc = saved_src;
        bottom
    }
    /// The notes in `list`, one under the other from the cursor, each its
    /// number and then its blocks, set `noteScale` smaller than the text.
    pub fn drawNoteList(&mut self, list: &[i64], x: f64, w: f64) {
        let mut saved: Vec<f64> = self.styleNumbers();
        self.scaleStyle(&saved, self.noteScale.clone());
        let was_tight: bool = self.inTightItem;
        self.inTightItem = true;
        let mut probe: Rc<RefCell<MdRun>> = self.baseRun();
        let mut widest: f64 = 0.0;
        let mut k: i64 = 0;
        while k < (list.len() as i64) {
            let mw: f64 = self.measureRun(&format!("{}.", list[k as usize].to_string()),
                &probe
            );
            if mw > widest {
                widest = mw;
            }
            k += 1;
        }
        let indent: f64 = widest + self.style.borrow().markerGap;
        let mut i: i64 = 0;
        while i < (list.len() as i64) {
            let n: i64 = list[i as usize];
            let mut fd: Rc<RefCell<MdNode>> = self.notesDoc.borrow().noteAt(n);
            if fd.borrow().kind == "footnote_def" {
                let first: i64 = self.boxes.len() as i64;
                let top: f64 = self.cursorY;
                self.blocks(&fd, x + indent, w - indent);
                let mut marker_y: f64 = top;
                if first < (self.boxes.len() as i64) {
                    let mut fb: Rc<RefCell<MdBox>> = self.firstTextBox(first);
                    marker_y = fb.borrow().y;
                }
                let marker: String = format!("{}.", n.to_string());
                let mut _tmp_2626 = self.measureRun(&marker, &probe);
                let mut mb: Rc<RefCell<MdBox>> = MdBox::textBox(
                    marker.clone(),
                    x,
                    marker_y,
                    _tmp_2626,
                    self.style.borrow().bodySize * self.style.borrow().lineHeight
                );
                mb.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                mb.borrow_mut().size = self.style.borrow().bodySize;
                mb.borrow_mut().color = self.style.borrow().mutedColor.clone();
                self.emit(mb.clone());
                // whose source the boxes name: a labelled note its
                // definition's, an inline one none (see above)
                let mut j: i64 = first;
                while j < (self.boxes.len() as i64) {
                    let mut nb: Rc<RefCell<MdBox>> = self.boxes[j as usize].clone();
                    if !fd.borrow().info.is_empty() {
                        if nb.borrow().srcStart < 0 || j == (self.boxes.len() as i64) - 1 {
                            nb.borrow_mut().srcStart = fd.borrow().srcStart;
                        }
                    } else {
                        nb.borrow_mut().srcStart = -1;
                    }
                    j += 1;
                }
            }
            i += 1;
        }
        self.inTightItem = was_tight;
        self.restoreStyle(&saved);
    }
    /// The notes, with their rule, on page `p` from `top`. Returns where they
    /// end.
    pub fn drawNotesAt(&mut self, list: &[i64], p: i64, top: f64) -> f64 {
        let saved_paginate: bool = self.paginate;
        let saved_page: i64 = self.page;
        let saved_in: bool = self.inNotes;
        let saved_src: i64 = self.currentSrc;
        self.paginate = false;
        self.inNotes = true;
        self.page = p;
        let from: i64 = self.boxes.len() as i64;
        let rule_w: f64 = self.contentW * 0.25;
        let mut _tmp_2631 = self.contentX.clone();
        let mut rule: Rc<RefCell<MdBox>> = MdBox::rect(_tmp_2631, top, rule_w, 0.75, self.style.borrow().ruleColor.clone());
        self.emit(rule.clone());
        rule.borrow_mut().srcStart = -1;
        self.cursorY = top + self.noteSepH() * 0.5;
        let mut _tmp_2634 = self.contentX.clone();
        self.drawNoteList(list, _tmp_2634, self.contentW.clone());
        let mut end: f64 = self.cursorY;
        let mut i: i64 = from;
        while i < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            b.borrow_mut().furniture = 3;
            if b.borrow().y + b.borrow().h > end {
                end = b.borrow().y + b.borrow().h;
            }
            i += 1;
        }
        self.paginate = saved_paginate;
        self.page = saved_page;
        self.inNotes = saved_in;
        self.currentSrc = saved_src;
        end
    }
    /// Each page's notes, at its foot: the notes whose first reference was
    /// drawn on it, in number order, then the notes those refer to.
    pub fn placeNotesPaged(&mut self) {
        let count: i64 = self.notesDoc.borrow().noteCount();
        if count == 0 {
            return;
        }
        let mut first: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < count {
            first.push(-1);
            i += 1;
        }
        let mut k: i64 = 0;
        while k < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            let n: i64 = b.borrow().noteRef;
            if (n >= 1 && n <= count) && b.borrow().furniture == 0 {
                if first[(n - 1) as usize] < 0 {
                    first[(n - 1) as usize] = b.borrow().page;
                }
            }
            k += 1;
        }
        let saved_cursor: f64 = self.cursorY;
        let mut p: i64 = 0;
        while p < self.pageCount {
            let mut list: Vec<i64> = Vec::new();
            let mut m: i64 = 1;
            while m <= count {
                if first[(m - 1) as usize] == p {
                    list.push(m);
                }
                m += 1;
            }
            if !list.is_empty() {
                let mut j: i64 = 0;
                while j < (list.len() as i64) {
                    MdLayout::notesWithin(
                        &self.notesDoc.borrow().noteAt(list[j as usize]),
                        &mut first,
                        p,
                        &mut list
                    );
                    j += 1;
                }
                let mut _tmp_2638 = self.contentX.clone();
                let h: f64 = self.measureNotes(&list, _tmp_2638, self.contentW.clone());
                let sep: f64 = self.noteSepH();
                // where this page's own text ends
                let mut text_end: f64 = self.contentTop;
                let mut q: i64 = 0;
                while q < (self.boxes.len() as i64) {
                    let mut tb: Rc<RefCell<MdBox>> = self.boxes[q as usize].clone();
                    if tb.borrow().page == p && tb.borrow().furniture == 0 {
                        if tb.borrow().y + tb.borrow().h > text_end {
                            text_end = tb.borrow().y + tb.borrow().h;
                        }
                    }
                    q += 1;
                }
                let mut top: f64 = (self.contentBottom - h) - sep * 0.5;
                // Text that reached into their room — a picture fitted to
                // the page, a reference in a table — has them under it
                // rather than over it.
                if top < text_end + sep * 0.5 {
                    top = text_end + sep * 0.5;
                }
                self.drawNotesAt(&list, p, top);
            }
            p += 1;
        }
        self.cursorY = saved_cursor;
    }
    /// One long column: the notes close the document.
    pub fn placeNotesAtEnd(&mut self) {
        let count: i64 = self.notesDoc.borrow().noteCount();
        if count == 0 {
            return;
        }
        let mut list: Vec<i64> = Vec::new();
        let mut m: i64 = 1;
        while m <= count {
            list.push(m);
            m += 1;
        }
        let top: f64 = self.cursorY + self.noteSepH() * 0.5;
        let _tmp_2642 = self.drawNotesAt(&list, self.page.clone(), top);
        self.cursorY = _tmp_2642;
    }
    /// ----------------------------------------------------------------------
    /// Definition lists and `::: containers`
    /// ----------------------------------------------------------------------
    /// Each term bold, at the list's edge; each of its definitions under it,
    /// indented as a list item's text is. A tight list keeps its definitions
    /// as close as a tight list's items.
    pub fn defList(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let was_tight: bool = self.inTightItem;
        let indent: f64 = self.style.borrow().listIndent;
        let saved_src: i64 = self.currentSrc;
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if c.borrow().kind == "dt" {
                let mut base: Rc<RefCell<MdRun>> = self.baseRun();
                base.borrow_mut().bold = true;
                let mut runs: Vec<Rc<RefCell<MdRun>>> = self.inlineRuns(&c, &base);
                if c.borrow().srcStart >= 0 {
                    self.currentSrc = c.borrow().srcStart;
                }
                self.paragraph(&runs, x, w);
                self.currentSrc = saved_src;
                self.cursorY += self.style.borrow().listTightSpacing;
            }
            if c.borrow().kind == "dd" {
                self.inTightItem = node.borrow().listTight;
                self.blocks(&c, x + indent, w - indent);
                self.inTightItem = was_tight;
            }
            i += 1;
        }
        if node.borrow().listTight {
            let extra: f64 = self.style.borrow()
                .paraSpacing - self.style.borrow()
                .listTightSpacing;
            if extra > 0.0 {
                self.cursorY += extra;
            }
        }
    }
    /// Its title, when the opening line had one, bold over its blocks. The
    /// plate around them is `containerBlock`'s, the one `{container=box}`
    /// draws.
    pub fn customContainer(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        if MdColumns::isRow(node) {
            self.columnsBlock(node, x, w);
            return;
        }
        if !node.borrow().title.is_empty() {
            let mut tr: Rc<RefCell<MdRun>> = self.baseRun();
            tr.borrow_mut().bold = true;
            tr.borrow_mut().text = node.borrow().title.clone();
            let mut runs: Vec<Rc<RefCell<MdRun>>> = Vec::new();
            runs.push(tr.clone());
            self.paragraph(&runs, x, w);
            self.cursorY += self.style.borrow().listTightSpacing;
        }
        self.blocks(node, x, w);
    }
    /// ----------------------------------------------------------------------
    /// Blocks
    /// ----------------------------------------------------------------------
    pub fn blocks(&mut self, parent: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let mut i: i64 = 0;
        let n: i64 = parent.borrow().childCount();
        while i < n {
            let mut child: Rc<RefCell<MdNode>> = parent.borrow().childAt(i);
            self.block(child.clone(), x, w);
            i += 1;
        }
    }
    /// The empty lines of `blankStops` in [from, to) (to -1: to the end),
    /// each a line of the body's height with a caret stop at its start.
    pub fn blankLinesIn(&mut self, from: i64, to: i64, x: f64) {
        if !self.caretStops || self.blankStops.is_empty() {
            return;
        }
        let lh: f64 = self.style.borrow()
            .bodySize * self.style.borrow()
            .lineHeight;
        let mut k: i64 = 0;
        while k < (self.blankStops.len() as i64) {
            let off: i64 = self.blankStops[k as usize];
            if off >= from && (to < 0 || off < to) {
                let y: f64 = self.reserve(lh);
                let mut stop: Rc<RefCell<MdBox>> = MdBox::textBox(String::new(), x, y, 0.0, lh);
                stop.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                stop.borrow_mut().size = self.style.borrow().bodySize;
                stop.borrow_mut().segSrc = off;
                stop.borrow_mut().segLen = 0;
                self.emit(stop.clone());
                self.cursorY = (y + lh) + self.style.borrow().paraSpacing;
            }
            k += 1;
        }
    }
    pub fn block(&mut self, node: Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let outer_src: i64 = self.currentSrc;
        if node.borrow().srcStart >= 0 {
            self.currentSrc = node.borrow().srcStart;
        }
        // `{.lead}` and `{#id}`: what the sheet says about this block's size
        // and colour holds while it is laid out
        let mut scaled: bool = false;
        let mut saved: Vec<f64> = Vec::new();
        let saved_color: String = self.style.borrow().textColor.clone();
        let saved_align: String = self.align.clone();
        let mut saved_typo: Rc<RefCell<MdTypo>> = self.typo.clone().clone();
        let saved_family: String = self.blockFamily.clone();
        let mut own: String = self.blockAlign(&node);
        // a section or statement slide centres its lines
        if (own.is_empty() && self.slideCenter) && (node.borrow().kind == "paragraph" || node.borrow().kind == "heading") {
            own = "center".to_string();
        }
        if !own.is_empty() {
            self.align = own.clone();
        }
        // a statement's text at its title's size; a statement that is only
        // its title, larger than a title
        let place_r: f64 = self.statementRatio(&node);
        if place_r > 1.001 {
            saved = self.styleNumbers();
            self.scaleStyle(&saved, place_r);
            scaled = true;
        }
        // the typography it inherits (the document's at the top), then its
        // element's (`heading`, then `h2`), then its class's, below
        let mut nt: Rc<RefCell<MdTypo>> = self.typo.borrow().copy();
        if self.typoDepth == 0 {
            nt = self.style.borrow().typoOf("document").copy();
        }
        let tag: String = MdCss::tagOf(&node);
        if node.borrow().kind == "heading" {
            nt.borrow_mut().over(&self.style.borrow().typoOf("heading"));
        }
        nt.borrow_mut().over(&self.style.borrow().typoOf(&tag));
        // a class's background, padding or border: the block on a plate of
        // its own, as `{container=box}` would put it (`.note { background-
        // color: #fff3cd; padding: 12pt }` needs no attribute)
        let mut boxed: bool = false;
        if !self.sheetText.is_empty() && (node.borrow().classCount() > 0 || !node.borrow().nodeId.is_empty()) {
            let mut got: Rc<RefCell<CssComputed>> = self.styleOf(&node);
            nt.borrow_mut().over(&MdCss::typoOf(&got));
            if CssComputed::has(&got, "font-family") {
                let fam: String = CssValue::first_family(&CssComputed::get(&got, "font-family"));
                if !fam.is_empty() {
                    self.blockFamily = fam.clone();
                }
            }
            // …its OWN rules only: `h2 { border-color }` is every h2's look
            boxed = MdLayout::boxLook(&self.ownStyleOf(&node));
            if CssComputed::has(&got, "font-size") && !scaled {
                let mut _tmp_2656 = self.sizeRatio(&node,
                    &CssComputed::get(&got, "font-size")
                );
                let r: f64 = self.safeRatio(&node, _tmp_2656);
                if r > 0.0 && ((r - 1.0).abs()) > 0.001 {
                    saved = self.styleNumbers();
                    self.scaleStyle(&saved, r);
                    scaled = true;
                }
            }
            if CssComputed::has(&got, "color") {
                let mut c: Rc<RefCell<EVGColor>> = CssValue::color(&CssComputed::get(&got, "color"));
                if c.borrow().is_set {
                    self.style.borrow_mut().textColor = MdCss::hexOf(&c);
                }
            }
        }
        self.typo = nt.clone();
        self.typoDepth += 1;
        // `p { color }` (a class's own colour, set above, wins)
        if (node.borrow().kind == "paragraph" && !self.style.borrow().paraColor.is_empty()) && self.style.borrow()
            .textColor == saved_color {
            if self.quoteDepth == 0 {
                let textColor_v = self.style.borrow().paraColor.clone();
                self.style.borrow_mut().textColor = textColor_v;
            }
        }
        let mut plate: String = MdContainer::kindOf(&node);
        // `::: columns` / `::: col` arrange their blocks: a sheet's
        // `container { background }` plates boxes, not them
        if MdColumns::isRow(&node) || MdColumns::isCell(&node) {
            boxed = false;
        }
        if plate.is_empty() && boxed {
            if !node.borrow().hasAttr("container") {
                plate = "box".to_string();
            }
        }
        // a `::: container` is a plate wherever it is — they nest, with more
        // colons outside; a block INSIDE a `{container=…}` plate gets none
        let fx_from: i64 = self.boxes.len() as i64;
        if !plate.is_empty() && (self.containerDepth == 0 || node.borrow().kind == "container") {
            self.containerBlock(node.clone(), x, w, &plate);
        } else {
            self.blockInner(node.clone(), x, w);
        }
        self.blockEffect(&node, fx_from);
        self.typoDepth -= 1;
        self.typo = saved_typo.clone();
        self.blockFamily = saved_family.clone();
        if scaled {
            self.restoreStyle(&saved);
        }
        self.style.borrow_mut().textColor = saved_color.clone();
        // only the block's own: an HTML wrapper's alignment runs on past
        // the block that opened it, to its closing tag's
        if !own.is_empty() {
            self.align = saved_align.clone();
        }
        self.currentSrc = outer_src;
    }
    /// `{fx=neon fx-power=2}` under a block: the effect over the block's
    /// boxes, as a box of kind 6 put before them, so a painter draws a source
    /// effect under the block and a filter or backdrop over what is there
    /// (MdToEvg makes it an effect instance). The box spans what the block
    /// drew on the page it began on; `fx=none` and an empty name give none.
    /// On a heading `fx=` is the slide's (`## Title {fx=starfield}`), not a
    /// block's.
    pub fn blockEffect(&mut self, node: &Rc<RefCell<MdNode>>, from: i64) {
        if !node.borrow().hasAttr("fx") || node.borrow().kind == "heading" {
            return;
        }
        let name: String = node.borrow().attrValue("fx").trim().to_string();
        if name.is_empty() || name == "none" {
            return;
        }
        let n: i64 = self.boxes.len() as i64;
        if from >= n {
            return;
        }
        let mut first: Rc<RefCell<MdBox>> = self.boxes[from as usize].clone();
        let mut x0: f64 = 0.0;
        let mut y0: f64 = 0.0;
        let mut x1: f64 = 0.0;
        let mut y1: f64 = 0.0;
        let mut any: bool = false;
        let mut i: i64 = from;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if (b.borrow().page == first.borrow().page && !b.borrow().retold) && (b.borrow().furniture == 0 && (b.borrow().w > 0.0 && b.borrow().h > 0.0)) {
                if !any {
                    x0 = b.borrow().x;
                    y0 = b.borrow().y;
                    x1 = b.borrow().x + b.borrow().w;
                    y1 = b.borrow().y + b.borrow().h;
                    any = true;
                } else {
                    x0 = MdLayout::minD(x0, b.borrow().x);
                    y0 = MdLayout::minD(y0, b.borrow().y);
                    x1 = MdLayout::maxD(x1, b.borrow().x + b.borrow().w);
                    y1 = MdLayout::maxD(y1, b.borrow().y + b.borrow().h);
                }
            }
            i += 1;
        }
        if !any {
            return;
        }
        let mut fb: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        fb.borrow_mut().kind = 6;
        fb.borrow_mut().x = x0;
        fb.borrow_mut().y = y0;
        fb.borrow_mut().w = x1 - x0;
        fb.borrow_mut().h = y1 - y0;
        fb.borrow_mut().page = first.borrow().page;
        fb.borrow_mut().srcStart = node.borrow().srcStart;
        fb.borrow_mut().embedKind = name.clone();
        let mut k: i64 = 0;
        while k < (node.borrow().attrKeys.len() as i64) {
            let key: String = node.borrow().attrKeys[k as usize].clone();
            if (MdText::str_len(&key) as i64) > 3 && MdText::substr(&key, 0, 3) == "fx-" {
                let v: Option<f64> = node.borrow()
                    .attrVals[k as usize].clone()
                    .trim()
                    .to_string()
                    .parse::<f64>()
                    .ok();
                if v.is_some() {
                    fb.borrow_mut().fxNames.push(MdText::substr(&key, 3, (MdText::str_len(&key) as i64)));
                    fb.borrow_mut().fxValues.push(v.unwrap());
                }
            }
            k += 1;
        }
        let mut out: Vec<Rc<RefCell<MdBox>>> = Vec::new();
        i = 0;
        while i < n {
            if i == from {
                out.push(fb.clone());
            }
            out.push(self.boxes[i as usize].clone());
            i += 1;
        }
        self.boxes = out.clone();
    }
    /// `{container=box}` / `{container=bubble}` under a block: the block laid
    /// out inside a padded column, then a rounded plate under it on each page
    /// it reached — put BEFORE its boxes, so it is painted first. What colour
    /// the plate and the text are is `MdContainer.plan`'s question.
    pub fn containerBlock(&mut self, node: Rc<RefCell<MdNode>>, x: f64, w: f64, kind: &str) {
        let mut paper: String = self.style.borrow().pageBackground.clone();
        if paper == "transparent" {
            paper = String::new();
        }
        // the block's own colour: its attribute, else what the sheet says
        // about its classes (`.warning { background-color }`)
        let mut own: String = MdContainer::fillOf(&node);
        let mut radius: f64 = self.style.borrow().containerRadius;
        let mut shadow: String = self.style.borrow().containerShadow.clone();
        // [top, right, bottom, left], as `padding` lists them
        let mut pads: Vec<f64> = vec![self.style.borrow().containerPadY, self.style.borrow().containerPadX, self.style.borrow().containerPadY, self.style.borrow().containerPadX];
        let mut border: String = self.style.borrow().containerBorder.clone();
        let mut border_w: f64 = self.style.borrow().containerBorderWidth;
        if !self.sheetText.is_empty() && (node.borrow().classCount() > 0 || !node.borrow().nodeId.is_empty()) {
            let mut got: Rc<RefCell<CssComputed>> = self.ownStyleOf(&node);
            if own.is_empty() {
                own = MdLayout::sheetColour(&got, "background-color");
            }
            if own.is_empty() {
                own = MdLayout::sheetColour(&got, "background");
            }
            if CssComputed::has(&got, "border-radius") {
                radius = CssValue::font_points(&CssComputed::get(&got, "border-radius"), radius);
            }
            if CssComputed::has(&got, "box-shadow") {
                shadow = MdContainer::shadowColour(&CssComputed::get(&got, "box-shadow"));
            }
            let mut gp: Vec<f64> = MdCss::padding(&got);
            let mut pi: i64 = 0;
            while pi < 4 {
                if gp[pi as usize] >= 0.0 {
                    pads[pi as usize] = gp[pi as usize];
                }
                pi += 1;
            }
            let bc: String = MdCss::borderColour(&got);
            if !bc.is_empty() {
                border = bc.clone();
                border_w = MdCss::borderWidth(&got, 1.0);
            }
        }
        // `::: info|tip|warning|danger` the theme says nothing about: the
        // tone's tint and a line in its hue (MdContainer.toneFill)
        if own.is_empty() && node.borrow().kind == "container" {
            let tone: String = MdContainer::toneOf(&node.borrow().info.clone());
            if !tone.is_empty() {
                own = MdContainer::toneFill(&tone, &paper, &self.style.borrow().textColor);
                if border.is_empty() {
                    border = MdContainer::toneBorder(
                        &tone,
                        &own,
                        &self.style.borrow().textColor
                    );
                    border_w = 1.0;
                }
            }
        }
        // A `::: warning` is a callout on the page, not text lifted off a
        // picture: on a page of its own colour the plate `{container=box}`
        // defaults to (that colour, nearly opaque) would not show. So with
        // nothing said, it takes the theme's own neutral surface — the one
        // code and table heads stand on.
        let mut theme_fill: String = self.style.borrow().containerBackground.clone();
        if theme_fill.is_empty() && node.borrow().kind == "container" {
            theme_fill = self.style.borrow().codeBackground.clone();
        }
        let mut _tmp_2662 = self.style.borrow().textColor.clone();
        let mut _tmp_2663 = self.style.borrow().bodySize.clone();
        let mut p: Rc<RefCell<MdContainer>> = MdContainer::plan(
            kind.to_string(),
            &own,
            &theme_fill,
            radius,
            &paper,
            &_tmp_2662,
            _tmp_2663
        );
        p.borrow_mut().shadow = shadow.clone();
        // each side its own: `padding-top` alone moves the text down, it
        // does not put as much room under it too
        if pads[0] >= 0.0 {
            p.borrow_mut().padY = pads[0];
        }
        if pads[2] >= 0.0 {
            p.borrow_mut().padBottom = pads[2];
        }
        if pads[3] >= 0.0 {
            p.borrow_mut().padX = pads[3];
        }
        if !border.is_empty() && border_w > 0.0 {
            p.borrow_mut().border = border.clone();
            p.borrow_mut().borderWidth = border_w;
        }
        // `radius=12px` and `padding=20px` (or `padding="10px 24px"`, top and
        // bottom, then the sides) on the block, as the theme's lengths read
        if node.borrow().hasAttr("radius") {
            let _tmp_2665 = CssValue::font_points(&node.borrow().attrValue("radius"), p.borrow().radius.clone());
            p.borrow_mut().radius = _tmp_2665;
        }
        if node.borrow().hasAttr("padding") {
            let mut apads: Vec<String> = node.borrow().attrValue("padding")
                .trim()
                .to_string()
                .split(" ")
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            let mut pv: Vec<String> = Vec::new();
            for pp in apads.iter().cloned() {
                if !pp.is_empty() {
                    pv.push(pp.clone());
                }
            }
            if !pv.is_empty() {
                let _tmp_2667 = CssValue::font_points(&pv[0].clone(), p.borrow().padY.clone());
                p.borrow_mut().padY = _tmp_2667;
                let padBottom_v = p.borrow().padY;
                p.borrow_mut().padBottom = padBottom_v;
                let padX_v = p.borrow().padY;
                p.borrow_mut().padX = padX_v;
                if (pv.len() as i64) > 1 {
                    let _tmp_2671 = CssValue::font_points(&pv[1].clone(), p.borrow().padX.clone());
                    p.borrow_mut().padX = _tmp_2671;
                }
            }
        }
        // A chart or diagram (`{container=box}` under its fence) keeps the
        // column it was drawn for — the drawing is cached by that width, so a
        // narrower column would find none — and the plate hugs the drawing,
        // which has room of its own round its axes and labels.
        if node.borrow().kind == "embed" {
            p.borrow_mut().padX = 0.0;
            p.borrow_mut().hug = true;
            p.borrow_mut().tail = 0.0;
        }
        let saved_ink: String = self.style.borrow().textColor.clone();
        if !p.borrow().ink.is_empty() {
            self.style.borrow_mut().textColor = p.borrow().ink.clone();
        }
        let saved_plate: String = self.plateFill.clone();
        let saved_plate_paper: String = self.platePaper.clone();
        self.plateFill = p.borrow().fill.clone();
        self.platePaper = paper.clone();
        let before: i64 = self.boxes.len() as i64;
        let start_page: i64 = self.page;
        let top: f64 = self.cursorY;
        self.plateRoom = (p.borrow().padY + p.borrow().padBottom) + p.borrow().tail;
        self.plateRoomSrc = node.borrow().srcStart;
        self.cursorY += p.borrow().padY;
        self.containerDepth += 1;
        self.openPad += p.borrow().padY;
        let arg_0 = x + p.borrow().padX;
        self.blockInner(node.clone(), arg_0, w - p.borrow().padX * 2.0);
        self.openPad -= p.borrow().padY;
        self.containerDepth -= 1;
        self.style.borrow_mut().textColor = saved_ink.clone();
        self.plateFill = saved_plate.clone();
        self.platePaper = saved_plate_paper.clone();
        let mut plates: Vec<Rc<RefCell<MdBox>>> = self.plates(&p,
            before,
            start_page,
            top,
            x,
            w
        );
        if plates.is_empty() {
            return;
        }
        let mut last: Rc<RefCell<MdBox>> = plates[((plates.len() as i64) - 1) as usize].clone();
        // the block's own space after it, kept below the plate (and its tail)
        let ink_bottom: f64 = (last.borrow().y + last.borrow().h) - p.borrow().padBottom;
        let mut gap: f64 = self.cursorY - ink_bottom;
        if gap < 0.0 {
            gap = 0.0;
        }
        self.insertBoxes(before, &plates);
        self.cursorY = ((last.borrow().y + last.borrow().h) + p.borrow().tail) + gap;
    }
    /// One plate per page the block's boxes are on, from `before`.
    pub fn plates(&self, p: &Rc<RefCell<MdContainer>>, before: i64, start_page: i64, top: f64, x: f64, w: f64) -> Vec<Rc<RefCell<MdBox>>> {
        let mut out: Vec<Rc<RefCell<MdBox>>> = Vec::new();
        let n: i64 = self.boxes.len() as i64;
        let mut pg: i64 = start_page;
        while pg <= self.page {
            let mut min_y: f64 = 0.0;
            let mut max_y: f64 = 0.0;
            let mut min_x: f64 = 0.0;
            let mut max_x: f64 = 0.0;
            let mut any: bool = false;
            let mut i: i64 = before;
            while i < n {
                let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
                if b.borrow().page == pg && !b.borrow().retold {
                    let mut bx0: f64 = b.borrow().x;
                    let mut bx1: f64 = b.borrow().x + b.borrow().w;
                    // a drawing narrower than its column: where it is drawn
                    if b.borrow().kind == 3 {
                        let ei: i64 = self.embeds.borrow().find(&b.borrow().embedKind, &b.borrow().embedSource, b.borrow().w);
                        if ei >= 0 {
                            let mut e: Rc<RefCell<MdEmbedEntry>> = self.embeds.borrow().entryAt(ei);
                            if e.borrow().ok && e.borrow().width > 0.0 {
                                bx0 = b.borrow().x + e.borrow().shiftIn(b.borrow().w);
                                if e.borrow().width < b.borrow().w {
                                    bx1 = bx0 + e.borrow().width;
                                }
                            }
                        }
                    }
                    if false == any || b.borrow().y < min_y {
                        min_y = b.borrow().y;
                    }
                    if false == any || b.borrow().y + b.borrow().h > max_y {
                        max_y = b.borrow().y + b.borrow().h;
                    }
                    if false == any || bx0 < min_x {
                        min_x = bx0;
                    }
                    if false == any || bx1 > max_x {
                        max_x = bx1;
                    }
                    any = true;
                }
                i += 1;
            }
            if any {
                let mut pt: f64 = min_y - p.borrow().padY;
                if pg == start_page && top < pt {
                    pt = top;
                }
                let mut px: f64 = x;
                let mut pw: f64 = w;
                if p.borrow().hug {
                    px = min_x - p.borrow().padX;
                    if px < x {
                        px = x;
                    }
                    pw = (max_x + p.borrow().padX) - px;
                    if px + pw > x + w {
                        pw = (x + w) - px;
                    }
                }
                let ph: f64 = (max_y + p.borrow().padBottom) - pt;
                // a soft shadow (`box-shadow`): three layers under the plate,
                // each a little larger, lower and fainter
                if !p.borrow().shadow.is_empty() {
                    let mut sc: Rc<RefCell<EVGColor>> = EVGColor::parse(&p.borrow().shadow.clone());
                    let mut li: i64 = 3;
                    while li >= 1 {
                        let g: f64 = li as f64;
                        let a: f64 = 0.05 + 0.03 * (3.0 - g);
                        let mut layer: Rc<RefCell<MdBox>> = MdBox::rect(
                            px - g,
                            pt + g * 1.5,
                            pw + g * 2.0,
                            ph + g,
                            MdContainer::hexOf(&EVGColor::rgba(EVGColor::red(&sc), EVGColor::green(&sc), EVGColor::blue(&sc), a))
                        );
                        layer.borrow_mut().radius = p.borrow().radius + g;
                        layer.borrow_mut().page = pg;
                        layer.borrow_mut().srcStart = self.currentSrc;
                        out.push(layer.clone());
                        li -= 1;
                    }
                }
                let mut plate: Rc<RefCell<MdBox>> = MdBox::rect(px, pt, pw, ph, p.borrow().fill.clone());
                plate.borrow_mut().radius = p.borrow().radius;
                plate.borrow_mut().page = pg;
                plate.borrow_mut().srcStart = self.currentSrc;
                if pg == self.page {
                    plate.borrow_mut().tail = p.borrow().tail;
                }
                out.push(plate.clone());
                // `border: 2pt solid …`: a line round the plate, inside it
                if !p.borrow().border.is_empty() {
                    let half: f64 = p.borrow().borderWidth * 0.5;
                    let mut _tmp_2683 = p.borrow().border.clone();
                    let mut _tmp_2684 = p.borrow().borderWidth.clone();
                    let mut edge: Rc<RefCell<MdBox>> = MdBox::outline(
                        px + half,
                        pt + half,
                        pw - p.borrow().borderWidth,
                        ph - p.borrow().borderWidth,
                        _tmp_2683,
                        _tmp_2684
                    );
                    edge.borrow_mut().radius = p.borrow().radius;
                    edge.borrow_mut().page = pg;
                    edge.borrow_mut().srcStart = self.currentSrc;
                    out.push(edge.clone());
                }
            }
            pg += 1;
        }
        out
    }
    /// `add` put into the box list at `at`, ahead of what is there
    pub fn insertBoxes(&mut self, at: i64, add: &Vec<Rc<RefCell<MdBox>>>) {
        let mut out: Vec<Rc<RefCell<MdBox>>> = Vec::new();
        let n: i64 = self.boxes.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            if i == at {
                for a in add.iter().cloned() {
                    out.push(a.clone());
                }
            }
            out.push(self.boxes[i as usize].clone());
            i += 1;
        }
        if at >= n {
            for a2 in add.iter().cloned() {
                out.push(a2.clone());
            }
        }
        self.boxes = out.clone();
        self.pageIndexReady = false;
    }
    /// How a heading or paragraph sets its lines: `{.center}`, `{.right}`,
    /// `{.left}` or `{align=center}` under it, else the sheet's `text-align`
    /// for it (`h1 { text-align: center }`, `.lead { text-align: center }`);
    /// "" leaves the alignment it is in (an HTML wrapper's, or the left).
    /// Lists, tables and code keep theirs: a centred list item would leave
    /// its bullet behind.
    pub fn blockAlign(&self, node: &Rc<RefCell<MdNode>>) -> String {
        if node.borrow().kind != "paragraph" && node.borrow().kind != "heading" {
            return String::new();
        }
        if node.borrow().hasClass("center") {
            return "center".to_string();
        }
        if node.borrow().hasClass("right") {
            return "right".to_string();
        }
        if node.borrow().hasClass("left") {
            return "left".to_string();
        }
        let said: String = MdLayout::alignOfWord(&MdCss::alignWord(&node.borrow().attrValue("align")));
        if !said.is_empty() {
            return said;
        }
        if MdText::index_of(&self.sheetText, "text-align") < 0 {
            return String::new();
        }
        let mut got: Rc<RefCell<CssComputed>> = self.styleOf(node);
        if CssComputed::has(&got, "text-align") {
            return MdLayout::alignOfWord(&MdCss::alignWord(&CssComputed::get(&got, "text-align")));
        }
        String::new()
    }
    /// A block's `font-size` as a multiple of what it would have had: `1.3em`
    /// and `130%` say so directly, `26pt` against the sheet's size for the
    /// block's kind (the document's, or the heading level's) — a ratio, so a
    /// slide that was shrunk to fit shrinks the block with it.
    /// The multiple, cut where it would take a size out of the font range.
    /// `scaleStyle` scales every size, not only the block's own — a title said
    /// again on the slide the block spills onto is set in the scaled heading
    /// size — so the largest of them is the one that is held to the range.
    pub fn safeRatio(&self, node: &Rc<RefCell<MdNode>>, r: f64) -> f64 {
        let mut base: f64 = self.style.borrow().bodySize;
        let mut sizes: Vec<f64> = vec![self.style.borrow().h1Size, self.style.borrow().h2Size, self.style.borrow().h3Size, self.style.borrow().h4Size, self.style.borrow().h5Size, self.style.borrow().h6Size, self.style.borrow().codeSize];
        let mut i: i64 = 0;
        while i < (sizes.len() as i64) {
            if sizes[i as usize] > base {
                base = sizes[i as usize];
            }
            i += 1;
        }
        if r <= 1.0 {
            let mut _tmp_2686 = self.style.borrow().bodySize.clone();
            let mut _tmp_2687 = self.style.borrow().pageWidth.clone();
            return MdCssLimits::ratio(r, _tmp_2686, _tmp_2687, self.style.borrow().pageHeight.clone());
        }
        let mut _tmp_2689 = self.style.borrow().pageWidth.clone();
        MdCssLimits::ratio(r, base, _tmp_2689, self.style.borrow().pageHeight.clone())
    }
    pub fn sizeRatio(&self, node: &Rc<RefCell<MdNode>>, value: &str) -> f64 {
        let t: String = CssSheet::lower(&CssSheet::tidy(value));
        let n: i64 = MdText::str_len(&t) as i64;
        if n > 2 && MdText::substr(&t, n - 2, n) == "em" {
            return if ((MdText::substr(&t, 0, n - 2).parse::<f64>().ok()).is_some()) { ((MdText::substr(&t, 0, n - 2).parse::<f64>().ok()).unwrap()) } else { 1.0 };
        }
        if n > 1 && MdText::substr(&t, n - 1, n) == "%" {
            return (if ((MdText::substr(&t, 0, n - 1).parse::<f64>().ok()).is_some()) { ((MdText::substr(&t, 0, n - 1).parse::<f64>().ok()).unwrap()) } else { 100.0 }) / 100.0;
        }
        let mut tag: String = "document".to_string();
        let mut fallback: f64 = self.style.borrow().bodySize;
        if node.borrow().kind == "heading" {
            tag = format!("h{}", node.borrow().level);
            fallback = self.style.borrow().headingSize(node.borrow().level);
        }
        let mut base: f64 = fallback;
        let mut b: Rc<RefCell<CssComputed>> = self.sheet.borrow_mut().forTag(tag.clone());
        if CssComputed::has(&b, "font-size") {
            base = CssValue::font_points(&CssComputed::get(&b, "font-size"), fallback);
        }
        if base <= 0.0 {
            return 1.0;
        }
        CssValue::font_points(value, base) / base
    }
    /// ----------------------------------------------------------------------
    /// Columns
    /// ----------------------------------------------------------------------
    ///
    /// ```md
    /// - Verkkokauppa kasvoi 18 %
    /// - Jälleenmyynti pysyi ennallaan
    /// - Lisenssit laskivat 4 %
    /// {.c3}
    /// ```
    ///
    /// A block the sheet gives `column-count` to is laid out ONCE at the width
    /// of one column, and the boxes that came out are then cut into columns and
    /// moved. Laying each column out separately would be the obvious thing and
    /// it is wrong twice over: a list's marker width is measured across the
    /// whole list, and an item that wrapped differently in a narrower pass
    /// would change the balance it was being balanced by.
    ///
    /// **The unit is the ITEM, not the line.** `PLAN_SLIDES.md` Stage C: in
    /// document order DOWN each column, and the columns differ by at most one
    /// item. A line-level flow reads better in a newspaper and is unreadable in
    /// a deck, where a bullet split across two columns is a bullet nobody can
    /// follow.
    ///
    /// Which boxes belong to which item is read off the boxes themselves: each
    /// carries the source offset of the block it came from. A box whose offset
    /// is inside no item — a list's own bullet marker is emitted by the list,
    /// not by the item — joins the group before it, which is where it was
    /// drawn.
    pub fn columnCount(&self, node: &Rc<RefCell<MdNode>>) -> i64 {
        if self.sheetText.is_empty() {
            return 1;
        }
        if false == MdLayout::canColumn(&node.borrow().kind.clone()) {
            return 1;
        }
        let mut got: Rc<RefCell<CssComputed>> = self.styleOf(node);
        if false == CssComputed::has(&got, "column-count") {
            return 1;
        }
        let n: Option<i64> = CssSheet::tidy(&CssComputed::get(&got, "column-count"))
            .parse::<i64>()
            .ok();
        let cols: i64 = if n.is_some() { n.unwrap() } else { 1 };
        if cols < 2 {
            return 1;
        }
        if cols > 8 {
            return 8;
        }
        cols
    }
    pub fn columnGap(&self, node: &Rc<RefCell<MdNode>>) -> f64 {
        let mut got: Rc<RefCell<CssComputed>> = self.styleOf(node);
        if false == CssComputed::has(&got, "column-gap") {
            return 18.0;
        }
        let g: f64 = CssValue::font_points(&CssComputed::get(&got, "column-gap"), 18.0);
        let mut _tmp_2694 = self.style.borrow().pageWidth.clone();
        MdCssLimits::clampKind("space", g, _tmp_2694, self.style.borrow().pageHeight.clone())
    }
    pub fn columnFlow(&mut self, node: Rc<RefCell<MdNode>>, x: f64, w: f64, cols: i64, gap: f64) {
        let col_w: f64 = (w - gap * ((cols - 1) as f64)) / ((cols) as f64);
        if col_w < 24.0 {
            self.blockPlain(node.clone(), x, w);
            return;
        }
        let from: i64 = self.boxes.len() as i64;
        let mut top: f64 = self.cursorY;
        let mut h: f64 = self.columnPass(node.clone(), x, col_w, cols, gap, top, from);
        // In a paginated layout a block cut into columns is cut as a whole, so
        // it either fits where it is or starts a sheet of its own. It cannot
        // be asked to fit before it has been laid out, so it is laid out, and
        // laid out again from the top of the next sheet when it does not.
        if self.paginate {
            if top + h > self.contentBottom {
                if self.contentTop + h <= self.contentBottom {
                    self.truncate(from);
                    self.page += 1;
                    self.pageCount = self.page + 1;
                    top = self.contentTop;
                    h = self.columnPass(node.clone(), x, col_w, cols, gap, top, from);
                }
            }
        }
        self.cursorY = top + h;
    }
    /// a block's `width=`, or "" when its column took it
    pub fn widthAttr(&self, node: &Rc<RefCell<MdNode>>) -> String {
        if node.borrow().srcStart == self.takenSrc {
            return String::new();
        }
        return node.borrow().attrValue("width");
    }
    pub fn columnsBlock(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let mut plan: Rc<RefCell<MdColumnsPlan>> = MdColumns::plan(node);
        let mut li: i64 = 0;
        while li < (plan.borrow().lead.len() as i64) {
            self.block(node.borrow().childAt(plan.borrow().lead[li as usize]).clone(), x, w);
            li += 1;
        }
        if plan.borrow().count() == 0 {
            return;
        }
        let from: i64 = self.boxes.len() as i64;
        let start_page: i64 = self.page;
        let top: f64 = self.cursorY;
        let overflows: i64 = self.slideOverflows;
        self.columnsPass(node, &plan, x, w, top);
        if (self.paginate && self.page > start_page) && top - self.contentTop > 0.5 {
            // does it fit at the top of a slide of its own?
            // unused:  let end_page: i64 = self.page;
            // unused:  let end_y: f64 = self.cursorY;
            self.truncate(from);
            self.page = start_page;
            self.pageCount = self.page + 1;
            self.forgetPagesAfter(self.page.clone());
            self.slideOverflows = overflows;
            self.cursorY = self.contentTop;
            self.nextSlide();
            if self.slides {
                self.emitSlideTitle();
            }
            let fresh_page: i64 = self.page;
            self.columnsPass(node, &plan, x, w, self.cursorY.clone());
            if self.page != fresh_page {
                // no: as it was
                self.truncate(from);
                self.page = start_page;
                self.pageCount = self.page + 1;
                self.forgetPagesAfter(self.page.clone());
                self.slideOverflows = overflows;
                self.cursorY = top;
                self.columnsPass(node, &plan, x, w, top);
            }
        }
    }
    pub fn columnsPass(&mut self, node: &Rc<RefCell<MdNode>>, plan: &Rc<RefCell<MdColumnsPlan>>, x: f64, w: f64, top: f64) {
        let mut xs: Vec<f64> = plan.borrow().lefts(x, w);
        let mut ws: Vec<f64> = plan.borrow().widths(w);
        let start_page: i64 = self.page;
        let mut end_page: i64 = self.page;
        let mut end_y: f64 = top;
        let saved_taken: i64 = self.takenSrc;
        let saved_spill: i64 = self.colSpillPage;
        let saved_spill_top: f64 = self.colSpillTop;
        self.columnsOn += 1;
        self.colSpillPage = -1;
        let mut g: i64 = 0;
        while g < plan.borrow().count() {
            let mut grp: Vec<i64> = plan.borrow().groups[g as usize].clone();
            let cx: f64 = xs[g as usize];
            let cw: f64 = ws[g as usize];
            self.page = start_page;
            self.cursorY = top;
            let mut k: i64 = 0;
            while k < (grp.len() as i64) {
                let ci: i64 = grp[k as usize];
                let mut child: Rc<RefCell<MdNode>> = node.borrow().childAt(ci);
                self.takenSrc = -2;
                if plan.borrow().isTaken(ci) {
                    self.takenSrc = child.borrow().srcStart;
                }
                // a picture or chart leaves room for what is under it in
                // its column
                self.followH = 0.0;
                self.followBeside = false;
                if self.slides && k + 1 < (grp.len() as i64) {
                    if MdColumns::isMedia(&child) {
                        self.followH = self.measureNodes(node, &grp, k + 1, cw);
                    }
                }
                self.block(child.clone(), cx, cw);
                self.followH = 0.0;
                k += 1;
            }
            if self.page > end_page || self.page == end_page && self.cursorY > end_y {
                end_page = self.page;
                end_y = self.cursorY;
            }
            g += 1;
        }
        self.takenSrc = saved_taken;
        self.columnsOn -= 1;
        self.colSpillPage = saved_spill;
        self.colSpillTop = saved_spill_top;
        self.page = end_page;
        if self.page + 1 > self.pageCount {
            self.pageCount = self.page + 1;
        }
        self.cursorY = end_y;
    }
    /// How tall children `idx[from…]` of `parent` are at width `w`, laid out
    /// from the top and taken back (as `measureBlocks`).
    pub fn measureNodes(&mut self, parent: &Rc<RefCell<MdNode>>, idx: &[i64], from: i64, w: f64) -> f64 {
        let saved_boxes: i64 = self.boxes.len() as i64;
        let saved_y: f64 = self.cursorY;
        let saved_page: i64 = self.page;
        let saved_page_count: i64 = self.pageCount;
        let saved_overflows: i64 = self.slideOverflows;
        let saved_heads: i64 = self.headText.len() as i64;
        let saved_hidden_on: i64 = self.hiddenOnPage;
        let saved_taken: i64 = self.takenSrc;
        let was_paginate: bool = self.paginate;
        self.paginate = false;
        self.cursorY = 0.0;
        let mut j: i64 = from;
        while j < (idx.len() as i64) {
            self.takenSrc = -2;
            let mut _tmp_2706 = self.contentX.clone();
            self.block(parent.borrow().childAt(idx[j as usize]).clone(), _tmp_2706, w);
            j += 1;
        }
        let h: f64 = self.cursorY;
        self.paginate = was_paginate;
        self.truncate(saved_boxes);
        while (self.headText.len() as i64) > saved_heads {
            self.headText.pop();
            self.headLevel.pop();
            self.headPage.pop();
            self.headY.pop();
        }
        self.hiddenOnPage = saved_hidden_on;
        self.takenSrc = saved_taken;
        self.cursorY = saved_y;
        self.page = saved_page;
        self.pageCount = saved_page_count;
        self.slideOverflows = saved_overflows;
        h
    }
    pub fn notePlace(&mut self, title: &Rc<RefCell<MdNode>>, has_body: bool) {
        let word: String = MdSlideLayout::wordOf(title, &self.deckPlace.clone());
        self.slideCenter = MdSlideLayout::whole(&word);
        self.slideStatement = word == "statement";
        self.slideStatementAlone = self.slideStatement && !has_body;
        if word.is_empty() {
            return;
        }
        let i: i64 = self.placeSrc.iter()
            .position( |__r| __r.clone() == (title.borrow().srcStart).clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        if i >= 0 {
            self.placeWord[i as usize] = word.clone();
            return;
        }
        self.placeSrc.push(title.borrow().srcStart);
        self.placeWord.push(word);
    }
    /// Slide title `i` opens a slide: its body narrower by `asideFrac` when the
    /// slide's source, up to the next slide's title, holds an `asideAt`.
    pub fn noteAside(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64) {
        self.slideAside = false;
        if self.asideFrac <= 0.0 || self.fullContentW <= 0.0 {
            return;
        }
        let mut title: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
        let from: i64 = title.borrow().srcStart;
        let upto: i64 = MdLayout::slideEndSrc(doc, i, self.splitLevel.clone());
        let __it_at = self.asideAt.clone();
        for at in __it_at.iter().copied() {
            if at >= from && (upto < 0 || at < upto) {
                self.slideAside = true;
            }
        }
        self.contentW = self.fullContentW;
        if self.slideAside {
            // never narrower than half the column
            let _tmp_2712 = self.fullContentW - self.asideFrac * self.style.borrow()
                .pageWidth;
            self.contentW = _tmp_2712;
            if self.contentW < self.fullContentW * 0.5 {
                self.contentW = self.fullContentW * 0.5;
            }
        }
    }
    /// how much larger block `node` is set on a statement slide
    pub fn statementRatio(&self, node: &Rc<RefCell<MdNode>>) -> f64 {
        if false == self.slideStatement {
            return 1.0;
        }
        if self.containerDepth > 0 {
            return 1.0;
        }
        if node.borrow().kind == "heading" {
            if self.slideStatementAlone && node.borrow()
                .level <= self.splitLevel {
                return 1.5;
            }
            return 1.0;
        }
        if node.borrow().kind != "paragraph" {
            return 1.0;
        }
        if self.style.borrow().bodySize <= 0.0 {
            return 1.0;
        }
        // a short statement (a line or two, as the slide is for) at one and
        // a half times the title's size, as a statement that is only its
        // title is; a longer one at the title's size
        let mut r: f64 = self.style.borrow().headingSize(self.slideTitleLevel.clone()) / self.style.borrow()
            .bodySize;
        if MdLayout::wordCount(&node.borrow().plainText()) <= 12 {
            r *= 1.5;
        }
        if r < 1.0 {
            return 1.0;
        }
        self.safeRatio(node, r)
    }
    pub fn alignSlides(&mut self) {
        if self.placeSrc.is_empty() {
            return;
        }
        // the word for each page, from the title on it
        let mut word: Vec<String> = Vec::new();
        let mut tsrc: Vec<i64> = Vec::new();
        let mut p0: i64 = 0;
        while p0 < self.pageCount {
            word.push(String::new().to_string());
            tsrc.push(-1);
            p0 += 1;
        }
        let mut i: i64 = 0;
        while i < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if (!b.borrow().retold && b.borrow().furniture == 0) && (b.borrow().page >= 0 && b.borrow().page < self.pageCount) {
                let k: i64 = self.placeSrc.iter()
                    .position( |__r| __r.clone() == (b.borrow().srcStart).clone() )
                    .map(|__i| __i as i64)
                    .unwrap_or(-1);
                if k >= 0 && (MdText::str_len(&word[(b.borrow().page) as usize].clone()) as i64) == 0 {
                    word[(b.borrow().page) as usize] = self.placeWord[k as usize].clone();
                    tsrc[(b.borrow().page) as usize] = b.borrow().srcStart;
                }
            }
            i += 1;
        }
        // a hidden title draws nothing: its page is where it was noted
        let mut h: i64 = 0;
        while h < (self.placeSrc.len() as i64) {
            let hp: i64 = self.pageOfHidden(self.placeSrc[h as usize]);
            if (hp >= 0 && hp < self.pageCount) && (MdText::str_len(&word[hp as usize].clone()) as i64) == 0 {
                word[hp as usize] = self.placeWord[h as usize].clone();
                tsrc[hp as usize] = self.placeSrc[h as usize];
            }
            h += 1;
        }
        let mut p: i64 = 0;
        while p < self.pageCount {
            let w: String = word[p as usize].clone();
            if !w.is_empty() {
                self.alignPage(p, &w, tsrc[p as usize]);
            }
            p += 1;
        }
        self.pageIndexReady = false;
    }
    pub fn movable(&self, b: &Rc<RefCell<MdBox>>, p: i64) -> bool {
        if b.borrow().page != p {
            return false;
        }
        if b.borrow().pinned || b.borrow().furniture != 0 {
            return false;
        }
        true
    }
    pub fn alignPage(&self, p: i64, w: &str, title: i64) {
        let whole: bool = MdSlideLayout::whole(w);
        let mut top: f64 = 0.0;
        let mut bottom: f64 = 0.0;
        let mut any: bool = false;
        let mut i: i64 = 0;
        while i < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if self.movable(&b, p) {
                if whole || b.borrow().srcStart != title {
                    if false == any || b.borrow().y < top {
                        top = b.borrow().y;
                    }
                    if false == any || b.borrow().y + b.borrow().h > bottom {
                        bottom = b.borrow().y + b.borrow().h;
                    }
                    any = true;
                }
            }
            i += 1;
        }
        if false == any {
            return;
        }
        let foot: f64 = self.contentBottom - self.noteRoomOn(p);
        let mut d: f64 = MdSlideLayout::shift(w, top, bottom, self.contentTop.clone(), foot);
        d = self.clearOfCorners(p, w, title, d);
        if d < 0.5 {
            return;
        }
        let mut k: i64 = 0;
        while k < (self.boxes.len() as i64) {
            let mut c: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            if self.movable(&c, p) {
                if whole || c.borrow().srcStart != title {
                    let y_v = c.borrow().y + d;
                    c.borrow_mut().y = y_v;
                }
            }
            k += 1;
        }
    }
    /// How far the blocks of page `p` may move down (`d` asked) and stay
    /// above a picture pinned in a bottom corner, beside which they were not
    /// set: a centred slide does not push its text into the picture.
    pub fn clearOfCorners(&self, p: i64, w: &str, title: i64, d0: f64) -> f64 {
        let mut d: f64 = d0;
        let whole: bool = MdSlideLayout::whole(w);
        let mut i: i64 = 0;
        while i < (self.boxes.len() as i64) {
            let mut pic: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if (pic.borrow().page == p && pic.borrow().pinned) && (pic.borrow().y > self.contentTop + 0.5 && pic.borrow().kind == 4) {
                let gap: f64 = self.style.borrow().bodySize * 0.6;
                let mut k: i64 = 0;
                while k < (self.boxes.len() as i64) {
                    let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
                    if self.movable(&b, p) && (whole || b.borrow().srcStart != title) {
                        if b.borrow()
                            .x + b.borrow()
                            .w > pic.borrow()
                            .x && b.borrow()
                            .x < pic.borrow()
                            .x + pic.borrow()
                            .w {
                            let room: f64 = (pic.borrow().y - gap) - (b.borrow().y + b.borrow().h);
                            if room < d {
                                d = room;
                            }
                        }
                    }
                    k += 1;
                }
            }
            i += 1;
        }
        if d < 0.0 {
            return 0.0;
        }
        d
    }
    /// ----------------------------------------------------------------------
    /// A picture in a corner of the slide (`{float=top-right width=8%}`)
    /// ----------------------------------------------------------------------
    ///
    /// Out of the flow: drawn in the corner of the content area, `pinned`
    /// (a centred slide does not move it), and nothing goes under it — the
    /// slide's title is set narrower (`cornerInset`) and the text after it
    /// beside it, down to its foot, as beside any narrow picture (FLOAT).
    fn pinPicture(&mut self, node: &Rc<RefCell<MdNode>>, corner: &str, path: String, img: &Rc<RefCell<MdNode>>, st: &Rc<RefCell<VfsStat>>) {
        let w_attr: String = node.borrow().attrValue("width").trim().to_string();
        let h_attr: String = node.borrow().attrValue("height").trim().to_string();
        let full: f64 = self.contentBottom - self.contentTop;
        let mut fw: f64 = MdSlideLayout::topCornerShare(node) * self.contentW;
        if MdText::index_of(&corner, "bottom") == 0 {
            fw = MdEmbedKinds::widthOf(&w_attr, self.contentW);
            if w_attr.is_empty() {
                fw = self.contentW * 0.12;
            }
        }
        let mut fh: f64 = fw * 0.5;
        if st.borrow().pixelW > 0 && st.borrow().pixelH > 0 {
            fh = fw * ((st.borrow().pixelH as f64) / (st.borrow().pixelW as f64));
        }
        // A picture that says no size takes a corner's worth: at most 30%
        // of the height. One that says `width=` is that wide, and `height=`
        // (of the content height, as under any picture) is as high as it
        // may be; neither is held to the corner's worth.
        let mut most: f64 = full * 0.3;
        if !w_attr.is_empty() || !h_attr.is_empty() {
            most = full;
        }
        if !h_attr.is_empty() {
            most = MdEmbedKinds::widthOf(&h_attr, full);
        }
        if fh > most {
            fw *= most / fh;
            fh = most;
        }
        // a bottom corner's `height=` without a `width=`: that high, as wide
        // as that makes it (a top corner's width is the title's inset)
        if (MdText::index_of(&corner, "bottom") == 0 && !h_attr.is_empty()) && (w_attr.is_empty() && fh > 0.0) {
            let mut k: f64 = most / fh;
            if fw * k > self.contentW {
                k = self.contentW / fw;
            }
            fw *= k;
            fh *= k;
        }
        let mut px: f64 = self.contentX;
        if MdText::index_of(&corner, "right") > 0 {
            px = (self.contentX + self.contentW) - fw;
        }
        let mut py: f64 = self.contentTop;
        if MdText::index_of(&corner, "bottom") == 0 {
            py = self.contentBottom - fh;
        }
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 4;
        b.borrow_mut().x = px;
        b.borrow_mut().y = py;
        b.borrow_mut().w = fw;
        b.borrow_mut().h = fh;
        b.borrow_mut().imagePath = path.clone();
        b.borrow_mut().imageAlt = img.borrow().plainText();
        b.borrow_mut().imageNaturalW = st.borrow().pixelW;
        b.borrow_mut().imageNaturalH = st.borrow().pixelH;
        b.borrow_mut().pinned = true;
        self.emit(b.clone());
        let gap: f64 = self.style.borrow().bodySize * 1.2;
        // in a bottom corner: the text after it that would reach down to it
        // goes beside it (besideCorner)
        if MdText::index_of(&corner, "bottom") == 0 {
            self.cornerOn = true;
            self.cornerPage = self.page;
            self.cornerTop = py - gap * 0.5;
            self.cornerLeft = px;
            self.cornerRight = px + fw;
            self.cornerW = (self.contentW - fw) - gap;
            self.cornerX = self.contentX;
            if MdText::index_of(&corner, "left") > 0 {
                self.cornerX = (self.contentX + fw) + gap;
            }
        }
        // the text after it, beside it while it is above its foot
        if MdText::index_of(&corner, "top") == 0 && self.cursorY < py + fh {
            self.floatOn = true;
            self.floatPage = self.page;
            self.floatBottom = py + fh;
            self.floatW = (self.contentW - fw) - gap;
            self.floatX = self.contentX;
            if MdText::index_of(&corner, "left") > 0 {
                self.floatX = (self.contentX + fw) + gap;
            }
        }
    }
    /// The block of slide-title `i`'s slide that is pinned in a top corner,
    /// or an empty node.
    pub fn cornerNode(&self, doc: &Rc<RefCell<MdNode>>, i: i64) -> Rc<RefCell<MdNode>> {
        let mut j: i64 = i;
        while MdLayout::bodyFollows(doc, j, self.splitLevel) {
            let mut c: Rc<RefCell<MdNode>> = doc.borrow().childAt(j + 1);
            if c.borrow().kind == "paragraph" && MdSlideLayout::topCornerShare(&c) > 0.0 {
                let mut __re_gx1: Rc<RefCell<MdNode>> = MdLayout::loneImage(&c);
                if __re_gx1.borrow().kind == "image" {
                    return c;
                }
            }
            j += 1;
        }
        Rc::new(RefCell::new(MdNode::new()))
    }
    /// how much of the width (0..1) that corner block takes, 0 for none
    pub fn cornerInset(&self, doc: &Rc<RefCell<MdNode>>, i: i64) -> f64 {
        let mut c: Rc<RefCell<MdNode>> = self.cornerNode(doc, i);
        if c.borrow().kind != "paragraph" {
            return 0.0;
        }
        MdSlideLayout::topCornerShare(&c)
    }
    pub fn shrinkSlides(&mut self, doc: &Rc<RefCell<MdNode>>) {
        let n: i64 = doc.borrow().childCount();
        if (self.blockFirstPage.len() as i64) != n {
            return;
        }
        let mut p: i64 = 0;
        while p + 1 < self.pageCount {
            self.shrinkSlide(doc, p);
            p += 1;
        }
    }
    pub fn shrinkSlide(&mut self, doc: &Rc<RefCell<MdNode>>, p: i64) -> bool {
        let n: i64 = doc.borrow().childCount();
        // the blocks that start on p (the last of them may run on to p + 1),
        // then those that start on p + 1 and end there
        let mut members: Vec<i64> = Vec::new();
        let mut spanning: i64 = -1;
        let mut extras: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let fp: i64 = self.blockFirstPage[i as usize];
            let lp: i64 = self.blockLastPage[i as usize];
            if fp < p && lp >= p {
                return false;
            }
            if fp == p + 1 {
                let mut ex: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
                if lp != p + 1 {
                    return false;
                }
                if ex.borrow().hasClass("slide") {
                    return false;
                }
                // a heading there may have been a break the rules chose
                if ex.borrow().kind == "heading" {
                    return false;
                }
                members.push(i);
                extras += 1;
            }
            if fp == p {
                if extras > 0 {
                    return false;
                }
                members.push(i);
                if lp == p + 1 {
                    spanning = i;
                } else {
                    if lp != p {
                        return false;
                    }
                }
            }
            i += 1;
        }
        if (members.len() as i64) == extras {
            return false;
        }
        if spanning < 0 && extras == 0 {
            return false;
        }
        if spanning >= 0 {
            if members[(((members.len() as i64) - extras) - 1) as usize] != spanning {
                return false;
            }
        }
        // how much went over, and where the two slides' boxes are
        // what spilled, not counting the title said again above it
        let mut b0: i64 = -1;
        let mut b1: i64 = -1;
        let mut spill_top: f64 = self.contentBottom;
        let mut spill_bottom: f64 = self.contentTop;
        let mut k: i64 = 0;
        while k < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            if b.borrow().page == p || b.borrow().page == p + 1 {
                if b0 < 0 {
                    b0 = k;
                }
                b1 = k + 1;
                if b.borrow().kind == 3 {
                    return false;
                }
                let is_title: bool = b.borrow().kind == 0 && b.borrow().segSrc < 0;
                if b.borrow().page == p + 1 && !is_title {
                    if b.borrow().y < spill_top {
                        spill_top = b.borrow().y;
                    }
                    if b.borrow().y + b.borrow().h > spill_bottom {
                        spill_bottom = b.borrow().y + b.borrow().h;
                    }
                }
            }
            k += 1;
        }
        // a table spread evenly over the two slides put more on the second
        // than filling the first would have: what would have spilled
        let spill: f64 = (spill_bottom - spill_top) - self.spillExcessAt(p + 1);
        let room: f64 = self.contentBottom - self.contentTop;
        let mut table_only: bool = false;
        if spanning >= 0 && extras == 0 {
            let mut sb: Rc<RefCell<MdNode>> = doc.borrow().childAt(spanning);
            table_only = sb.borrow().kind == "table";
        }
        let mut most: f64 = self.slideShrinkSpill;
        let mut least: f64 = self.slideShrinkMin;
        if table_only {
            most = self.tableShrinkSpill;
            least = self.tableShrinkMin;
        }
        if b0 < 0 || spill > room * most {
            return false;
        }
        let mut kept: Vec<Rc<RefCell<MdBox>>> = Vec::new();
        let mut m: i64 = b0;
        while m < b1 {
            kept.push(self.boxes[m as usize].clone());
            m += 1;
        }
        let mut tail: Vec<Rc<RefCell<MdBox>>> = Vec::new();
        m = b1;
        while m < (self.boxes.len() as i64) {
            tail.push(self.boxes[m as usize].clone());
            m += 1;
        }
        let mut saved: Vec<f64> = self.styleNumbers();
        let saved_page: i64 = self.page;
        let saved_page_count: i64 = self.pageCount;
        let saved_cursor: f64 = self.cursorY;
        let saved_overflows: i64 = self.slideOverflows;
        let mut saved_cont: Rc<RefCell<MdContinuations>> = self.takeContinuations();
        let mut sc: f64 = 0.95;
        while sc >= least - 0.001 {
            self.truncate(b0);
            if table_only {
                self.scaleTable(&saved, sc);
            } else {
                self.scaleStyle(&saved, sc);
            }
            self.page = p;
            self.pageCount = p + 1;
            self.cursorY = self.contentTop;
            self.resumeContinuation(p);
            self.floatOn = false;
            let mut j: i64 = 0;
            while j < (members.len() as i64) {
                self.relayoutBlock(doc, members[j as usize]);
                j += 1;
            }
            self.endFloat();
            let fits: bool = self.page == p;
            self.restoreStyle(&saved);
            self.putContinuations(&saved_cont);
            if fits {
                // the continuation is gone: every later slide moves up one
                let mut t: i64 = 0;
                while t < (tail.len() as i64) {
                    let mut tb: Rc<RefCell<MdBox>> = tail[t as usize].clone();
                    let page_v = tb.borrow().page - 1;
                    tb.borrow_mut().page = page_v;
                    self.boxes.push(tb.clone());
                    t += 1;
                }
                let mut q: i64 = 0;
                while q < n {
                    if self.blockFirstPage[q as usize] > p {
                        self.blockFirstPage[q as usize] = self.blockFirstPage[q as usize] - 1;
                    }
                    if self.blockLastPage[q as usize] > p {
                        self.blockLastPage[q as usize] = self.blockLastPage[q as usize] - 1;
                    }
                    q += 1;
                }
                self.dropSpillExcess(p + 1);
                self.dropContinuation(p + 1);
                self.pagesMovedUp(p);
                self.page = saved_page - 1;
                self.pageCount = saved_page_count - 1;
                self.cursorY = saved_cursor;
                self.slideOverflows = saved_overflows - 1;
                if self.slideOverflows < 0 {
                    self.slideOverflows = 0;
                }
                self.slidesShrunk += 1;
                return true;
            }
            sc -= 0.05;
        }
        // nothing smaller fitted: the two slides as they were
        self.truncate(b0);
        let mut r: i64 = 0;
        while r < (kept.len() as i64) {
            self.boxes.push(kept[r as usize].clone());
            r += 1;
        }
        r = 0;
        while r < (tail.len() as i64) {
            self.boxes.push(tail[r as usize].clone());
            r += 1;
        }
        self.page = saved_page;
        self.pageCount = saved_page_count;
        self.cursorY = saved_cursor;
        self.slideOverflows = saved_overflows;
        false
    }
    /// One block of a slide laid out again (shrunk or grown), keeping the
    /// title and the section current as `bodyPass` does, so a continuation
    /// it runs onto says the right ones.
    pub fn relayoutBlock(&mut self, doc: &Rc<RefCell<MdNode>>, i: i64) {
        let mut child: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
        if self.scoped {
            self.scopeAtBlock(doc, i);
        }
        if child.borrow().kind == "heading" {
            self.clearSection();
            if child.borrow().level <= self.splitLevel {
                self.slideTitle = child.borrow().plainText();
                self.slideTitleLevel = child.borrow().level;
                self.slideTitleHidden = MdLayout::hiddenHeading(&child);
                self.notePlace(&child, MdLayout::bodyFollows(doc, i, self.splitLevel));
                self.noteAside(doc, i);
            }
        }
        self.placeBlock(doc, i, false);
        if child.borrow().kind == "heading" && child.borrow().level > self.splitLevel {
            self.sectionTitle = child.borrow().plainText();
            self.sectionLevel = child.borrow().level;
            self.sectionHidden = MdLayout::hiddenHeading(&child);
        }
    }
    pub fn growSlides(&mut self, doc: &Rc<RefCell<MdNode>>) {
        let n: i64 = doc.borrow().childCount();
        if (self.blockFirstPage.len() as i64) != n {
            return;
        }
        let mut p: i64 = 0;
        while p < self.pageCount {
            self.growSlide(doc, p);
            p += 1;
        }
    }
    pub fn growSlide(&mut self, doc: &Rc<RefCell<MdNode>>, p: i64) {
        let n: i64 = doc.borrow().childCount();
        let mut media_w: Vec<f64> = Vec::new();
        let mut media_h: Vec<f64> = Vec::new();
        // the blocks of this slide, none spilling into or out of it
        let mut members: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < n {
            let fp: i64 = self.blockFirstPage[i as usize];
            let lp: i64 = self.blockLastPage[i as usize];
            if fp == p {
                if lp != p {
                    return;
                }
                members.push(i);
            } else {
                if fp < p && lp >= p {
                    return;
                }
            }
            i += 1;
        }
        if members.is_empty() {
            return;
        }
        // its boxes, which are one run: pages only ever go forward
        let mut b0: i64 = -1;
        let mut b1: i64 = -1;
        let mut bottom: f64 = self.contentTop;
        let mut k: i64 = 0;
        while k < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            if b.borrow().page == p {
                if b0 < 0 {
                    b0 = k;
                }
                b1 = k + 1;
                // a picture or chart may stay as it is while the text grows
                // (beside it, or in a column of its own): the slide grows
                // only when every one of them keeps its size
                if (b.borrow().kind == 3 || !b.borrow().imagePath.is_empty()) && !b.borrow().pinned {
                    media_w.push(b.borrow().w);
                    media_h.push(b.borrow().h);
                }
                if b.borrow().y + b.borrow().h > bottom {
                    bottom = b.borrow().y + b.borrow().h;
                }
            }
            k += 1;
        }
        if b0 < 0 {
            return;
        }
        let room: f64 = self.contentBottom - self.contentTop;
        let used: f64 = bottom - self.contentTop;
        let has_media: bool = !media_w.is_empty();
        if used <= 0.0 {
            return;
        }
        // a picture or chart that keeps its size may be what makes the slide
        // full: the text beside it can still grow, down to that same foot
        if used >= room * self.slideGrowFill && !has_media {
            return;
        }
        // …and with one, what is checked is the boxes' foot, not the cursor
        // (the space after a chart grows with the text and is no box)
        let mut limit: f64 = self.contentTop + room * (self.slideGrowFill + 0.1);
        if has_media && bottom > limit {
            limit = self.contentBottom;
        }
        // the first guess from the height; text wraps more as it grows, so
        // each try is laid out and checked, a step smaller when it does not fit
        let mut s: f64 = (room * self.slideGrowFill) / used;
        // with a picture or chart the room the text has is not the slide's
        // height: each size from the largest down is tried
        if has_media {
            s = self.slideGrow;
        }
        if s > self.slideGrow {
            s = self.slideGrow;
        }
        let mut kept: Vec<Rc<RefCell<MdBox>>> = Vec::new();
        let mut m: i64 = b0;
        while m < b1 {
            kept.push(self.boxes[m as usize].clone());
            m += 1;
        }
        let mut tail: Vec<Rc<RefCell<MdBox>>> = Vec::new();
        m = b1;
        while m < (self.boxes.len() as i64) {
            tail.push(self.boxes[m as usize].clone());
            m += 1;
        }
        let mut saved: Vec<f64> = self.styleNumbers();
        let heads_before: i64 = MdLayout::headingLines(
            &kept,
            0,
            kept.len() as i64,
            saved[0] * 1.15
        );
        let saved_page: i64 = self.page;
        let saved_page_count: i64 = self.pageCount;
        let saved_cursor: f64 = self.cursorY;
        let saved_overflows: i64 = self.slideOverflows;
        let mut saved_cont: Rc<RefCell<MdContinuations>> = self.takeContinuations();
        while s > 1.04 {
            self.truncate(b0);
            self.growStyle(&saved, s);
            self.page = p;
            self.cursorY = self.contentTop;
            self.resumeContinuation(p);
            self.floatOn = false;
            self.growTrial = true;
            let mut j: i64 = 0;
            while j < (members.len() as i64) {
                self.relayoutBlock(doc, members[j as usize]);
                j += 1;
            }
            self.endFloat();
            self.growTrial = false;
            // …and a heading that was one line stays one line: a title
            // broken in two to be larger is not larger, it is worse
            let mut _tmp_2740 = self.boxes.clone();
            let heads_now: i64 = MdLayout::headingLines(
                &_tmp_2740,
                b0,
                self.boxes.len() as i64,
                (saved[0] * s) * 1.15
            );
            let mut reach: f64 = self.cursorY;
            if has_media {
                reach = self.footOn(b0, p);
            }
            let mut fits: bool = (self.page == p && reach <= limit + 0.5) && heads_now <= heads_before;
            if fits && !media_w.is_empty() {
                fits = self.mediaKept(b0, p, &media_w, &media_h);
            }
            self.restoreStyle(&saved);
            self.putContinuations(&saved_cont);
            self.page = saved_page;
            self.pageCount = saved_page_count;
            self.cursorY = saved_cursor;
            self.slideOverflows = saved_overflows;
            if fits {
                let mut t: i64 = 0;
                while t < (tail.len() as i64) {
                    self.boxes.push(tail[t as usize].clone());
                    t += 1;
                }
                self.slidesGrown += 1;
                return;
            }
            s -= 0.08;
        }
        // nothing larger fitted: the slide as it was
        self.truncate(b0);
        let mut r: i64 = 0;
        while r < (kept.len() as i64) {
            self.boxes.push(kept[r as usize].clone());
            r += 1;
        }
        r = 0;
        while r < (tail.len() as i64) {
            self.boxes.push(tail[r as usize].clone());
            r += 1;
        }
    }
    /// The lowest edge of the boxes from `from` on page `p`.
    pub fn footOn(&self, from: i64, p: i64) -> f64 {
        let mut foot: f64 = self.contentTop;
        let mut k: i64 = from;
        while k < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            if b.borrow().page == p && b.borrow().y + b.borrow().h > foot {
                foot = b.borrow().y + b.borrow().h;
            }
            k += 1;
        }
        foot
    }
    /// Whether the pictures and charts from box `from` on page `p` are the
    /// sizes `ws`×`hs` they were, in the same order.
    pub fn mediaKept(&self, from: i64, p: i64, ws: &[f64], hs: &[f64]) -> bool {
        let mut m: i64 = 0;
        let mut k: i64 = from;
        while k < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
            if b.borrow().page == p && ((b.borrow().kind == 3 || !b.borrow().imagePath.is_empty()) && !b.borrow().pinned) {
                if m >= (ws.len() as i64) {
                    return false;
                }
                if ((b.borrow().w - ws[m as usize]).abs()) > 1.0 {
                    return false;
                }
                if ((b.borrow().h - hs[m as usize]).abs()) > 1.0 {
                    return false;
                }
                m += 1;
            }
            k += 1;
        }
        m == (ws.len() as i64)
    }
    /// Every size and space a slide's content is measured with, in one list,
    /// so a grown slide can put them all back exactly.
    pub fn styleNumbers(&self) -> Vec<f64> {
        let mut v: Vec<f64> = vec![self.style.borrow().bodySize, self.style.borrow().paraSpacing, self.style.borrow().h1Size, self.style.borrow().h2Size, self.style.borrow().h3Size, self.style.borrow().h4Size, self.style.borrow().h5Size, self.style.borrow().h6Size];
        v.push(self.style.borrow().headingSpaceBefore);
        v.push(self.style.borrow().headingSpaceAfter);
        v.push(self.style.borrow().codeSize);
        v.push(self.style.borrow().codePadding);
        v.push(self.style.borrow().quoteBarWidth);
        v.push(self.style.borrow().quoteIndent);
        v.push(self.style.borrow().quotePadY);
        v.push(self.style.borrow().listIndent);
        v.push(self.style.borrow().markerGap);
        v.push(self.style.borrow().listTightSpacing);
        v.push(self.style.borrow().cellPadX);
        v.push(self.style.borrow().cellPadY);
        v.push(self.style.borrow().embedSpacing);
        v.push(self.style.borrow().tableSize);
        v.to_vec()
    }
    pub fn scaleStyle(&self, v: &[f64], s: f64) {
        self.style.borrow_mut().bodySize = v[0] * s;
        self.style.borrow_mut().paraSpacing = v[1] * s;
        self.style.borrow_mut().h1Size = v[2] * s;
        self.style.borrow_mut().h2Size = v[3] * s;
        self.style.borrow_mut().h3Size = v[4] * s;
        self.style.borrow_mut().h4Size = v[5] * s;
        self.style.borrow_mut().h5Size = v[6] * s;
        self.style.borrow_mut().h6Size = v[7] * s;
        self.style.borrow_mut().headingSpaceBefore = v[8] * s;
        self.style.borrow_mut().headingSpaceAfter = v[9] * s;
        self.style.borrow_mut().codeSize = v[10] * s;
        self.style.borrow_mut().codePadding = v[11] * s;
        self.style.borrow_mut().quoteBarWidth = v[12] * s;
        self.style.borrow_mut().quoteIndent = v[13] * s;
        self.style.borrow_mut().quotePadY = v[14] * s;
        self.style.borrow_mut().listIndent = v[15] * s;
        self.style.borrow_mut().markerGap = v[16] * s;
        self.style.borrow_mut().listTightSpacing = v[17] * s;
        self.style.borrow_mut().cellPadX = v[18] * s;
        self.style.borrow_mut().cellPadY = v[19] * s;
        self.style.borrow_mut().embedSpacing = v[20] * s;
        self.style.borrow_mut().tableSize = v[21] * s;
    }
    /// GROW's scale: everything on the slide larger except the slide titles
    /// (the headings a slide breaks at, `splitLevel` and up) and the space
    /// around headings. A title is the same size on every slide of the deck,
    /// however much is under it; a heading inside the slide grows with its
    /// text, never past the title's size.
    pub fn growStyle(&self, v: &[f64], s: f64) {
        self.scaleStyle(v, s);
        let mut title_size: f64 = v[2];
        let mut lv: i64 = 1;
        while lv <= 6 {
            let was: f64 = v[(lv + 1) as usize];
            if lv <= self.splitLevel {
                title_size = was;
            }
            lv += 1;
        }
        let _tmp_2742 = MdLayout::grownHeading(v[2], s, 1, self.splitLevel.clone(), title_size);
        self.style.borrow_mut().h1Size = _tmp_2742;
        let _tmp_2744 = MdLayout::grownHeading(v[3], s, 2, self.splitLevel.clone(), title_size);
        self.style.borrow_mut().h2Size = _tmp_2744;
        let _tmp_2746 = MdLayout::grownHeading(v[4], s, 3, self.splitLevel.clone(), title_size);
        self.style.borrow_mut().h3Size = _tmp_2746;
        let _tmp_2748 = MdLayout::grownHeading(v[5], s, 4, self.splitLevel.clone(), title_size);
        self.style.borrow_mut().h4Size = _tmp_2748;
        let _tmp_2750 = MdLayout::grownHeading(v[6], s, 5, self.splitLevel.clone(), title_size);
        self.style.borrow_mut().h5Size = _tmp_2750;
        let _tmp_2752 = MdLayout::grownHeading(v[7], s, 6, self.splitLevel.clone(), title_size);
        self.style.borrow_mut().h6Size = _tmp_2752;
        self.style.borrow_mut().headingSpaceBefore = v[8];
        self.style.borrow_mut().headingSpaceAfter = v[9];
    }
    /// A table's own size (`tableSize`, else the body's) and its cells'
    /// padding scaled by `s` (the padding over and under by `s`²);
    /// everything else as saved in `v`.
    pub fn scaleTable(&self, v: &[f64], s: f64) {
        self.scaleStyle(v, 1.0);
        let mut size: f64 = v[21];
        if size <= 0.0 {
            size = v[0];
        }
        self.style.borrow_mut().tableSize = size * s;
        self.style.borrow_mut().cellPadX = v[18] * s;
        // the air between rows gives way faster than the text
        self.style.borrow_mut().cellPadY = (v[19] * s) * s;
    }
    pub fn restoreStyle(&self, v: &[f64]) {
        self.scaleStyle(v, 1.0);
    }
    pub fn truncate(&mut self, n: i64) {
        while (self.boxes.len() as i64) > n {
            self.boxes.pop();
        }
        // what the boxes left say about the pages' footnotes
        if self.notesDoc.borrow().noteCount() > 0 {
            if false == self.inNotes {
                self.retrackNotes();
            }
        }
    }
    /// Lay the block out at one column's width from `top`, then move the groups
    /// into columns. Returns the height the columns take.
    pub fn columnPass(&mut self, node: Rc<RefCell<MdNode>>, x: f64, col_w: f64, cols: i64, gap: f64, top: f64, from: i64) -> f64 {
        let was_paginate: bool = self.paginate;
        self.paginate = false;
        self.cursorY = top;
        self.blockPlain(node.clone(), x, col_w);
        self.paginate = was_paginate;
        let to: i64 = self.boxes.len() as i64;
        if to <= from {
            return 0.0;
        }
        // Which item each box belongs to.
        let mut owner: Vec<i64> = Vec::new();
        let kids: i64 = node.borrow().childCount();
        let mut i: i64 = from;
        let mut last: i64 = 0;
        while i < to {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            // A list's own bullet carries the LIST's source offset, and a
            // list starts where its first item does — so asking the spans
            // alone puts every marker in the first item's group and leaves
            // the bullets behind in column one while their text moves. The
            // groups are emitted in order, so a box may never belong to an
            // earlier one than the box before it.
            let mut g: i64 = MdLayout::itemOf(&node, kids, b.borrow().srcStart.clone());
            if g < last {
                g = last;
            }
            owner.push(g);
            last = g;
            i += 1;
        }
        // …and where each group sits.
        let mut g_top: Vec<f64> = Vec::new();
        let mut g_bot: Vec<f64> = Vec::new();
        let mut g_seen: Vec<bool> = Vec::new();
        let mut k: i64 = 0;
        while k < kids {
            g_top.push(0.0);
            g_bot.push(0.0);
            g_seen.push(false);
            k += 1;
        }
        i = from;
        while i < to {
            let mut b2: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            let g2: i64 = owner[(i - from) as usize];
            if g2 < kids {
                if g_seen[g2 as usize] {
                    if b2.borrow().y < g_top[g2 as usize] {
                        g_top[g2 as usize] = b2.borrow().y;
                    }
                    if b2.borrow().y + b2.borrow().h > g_bot[g2 as usize] {
                        g_bot[g2 as usize] = b2.borrow().y + b2.borrow().h;
                    }
                } else {
                    g_seen[g2 as usize] = true;
                    g_top[g2 as usize] = b2.borrow().y;
                    g_bot[g2 as usize] = b2.borrow().y + b2.borrow().h;
                }
            }
            i += 1;
        }
        // The split: document order down each column, and at most one item
        // between the longest column and the shortest.
        let base: i64 = kids / cols;
        let extra: i64 = kids - base * cols;
        let mut col_of: Vec<i64> = Vec::new();
        let mut first_of: Vec<i64> = Vec::new();
        let mut c: i64 = 0;
        let mut at: i64 = 0;
        while c < cols {
            let mut take: i64 = base;
            if c < extra {
                take += 1;
            }
            first_of.push(at);
            let mut t: i64 = 0;
            while t < take {
                col_of.push(c);
                at += 1;
                t += 1;
            }
            c += 1;
        }
        while (col_of.len() as i64) < kids {
            col_of.push(cols - 1);
        }
        // Move them.
        let mut dy: Vec<f64> = Vec::new();
        c = 0;
        while c < cols {
            let first_idx: i64 = first_of[c as usize];
            let mut shift: f64 = 0.0;
            if first_idx < kids {
                if g_seen[first_idx as usize] {
                    shift = top - g_top[first_idx as usize];
                }
            }
            dy.push(shift);
            c += 1;
        }
        i = from;
        while i < to {
            let mut b3: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            let mut g3: i64 = owner[(i - from) as usize];
            if g3 >= kids {
                g3 = kids - 1;
            }
            if g3 < 0 {
                g3 = 0;
            }
            let col: i64 = col_of[g3 as usize];
            let x_v = b3.borrow().x + ((col) as f64) * (col_w + gap);
            b3.borrow_mut().x = x_v;
            let y_v = b3.borrow().y + dy[col as usize];
            b3.borrow_mut().y = y_v;
            i += 1;
        }
        // The tallest column is the block's height.
        let mut h: f64 = 0.0;
        let mut g4: i64 = 0;
        while g4 < kids {
            if g_seen[g4 as usize] {
                let col2: i64 = col_of[g4 as usize];
                let bot: f64 = (g_bot[g4 as usize] + dy[col2 as usize]) - top;
                if bot > h {
                    h = bot;
                }
            }
            g4 += 1;
        }
        h
    }
    pub fn blockInner(&mut self, node: Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let cols: i64 = self.columnCount(&node);
        if cols > 1 {
            self.columnFlow(node.clone(), x, w, cols, self.columnGap(&node));
            return;
        }
        self.blockPlain(node.clone(), x, w);
    }
    pub fn blockPlain(&mut self, node: Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let kind: String = node.borrow().kind.clone();
        if kind == "paragraph" {
            if self.pictureBlock(&node, x, w) {
                return;
            }
            if MdSvg::isWhole(&MdSvg::paraText(&node)) && self.svgPicture(&node, x, w) {
                return;
            }
            let mut runs: Vec<Rc<RefCell<MdRun>>> = self.inlineRuns(&node, &self.baseRun());
            self.paragraph(&runs, x, w);
            let mut gap: f64 = self.style.borrow().paraSpacing;
            if self.inTightItem {
                gap = self.style.borrow().listTightSpacing;
            }
            self.cursorY += gap;
            return;
        }
        if kind == "heading" {
            self.heading(&node, x, w);
            return;
        }
        if kind == "thematic_break" {
            let y: f64 = self.reserve(14.0);
            let arg_0 = MdBox::rect(
                x,
                y + 6.0,
                w,
                1.0,
                self.style.borrow().ruleColor.clone()
            ).clone();
            self.emit(arg_0);
            self.cursorY = y + 14.0;
            return;
        }
        if kind == "code_block" {
            if MdGallery::isGallery(&node) {
                self.gallery(&node, x, w);
                return;
            }
            self.codeBlock(node.clone(), x, w);
            return;
        }
        if kind == "html_block" {
            if self.htmlBlock(&node, x, w) {
                return;
            }
            // Markup this layout does not read is shown as what it is —
            // which is also more honest than dropping it.
            let mut shown: Rc<RefCell<MdNode>> = MdNode::create("code_block".to_string());
            shown.borrow_mut().literal = node.borrow().literal.clone();
            shown.borrow_mut().info = "html".to_string();
            self.codeBlock(shown.clone(), x, w);
            return;
        }
        if kind == "block_quote" {
            self.blockQuote(&node, x, w);
            return;
        }
        if kind == "list" {
            self.list(&node, x, w);
            return;
        }
        if kind == "table" {
            self.table(&node, x, w);
            return;
        }
        if kind == "front_matter" {
            return;
        }
        if kind == "embed" {
            self.embed(&node, x, w);
            return;
        }
        if kind == "math_block" {
            self.mathBlock(&node, x, w);
            return;
        }
        // drawn at the foot of the page that first refers to it (`placeNotes`)
        if kind == "footnote_def" {
            return;
        }
        if kind == "dl" {
            self.defList(&node, x, w);
            return;
        }
        if kind == "container" {
            self.customContainer(&node, x, w);
            return;
        }
        // anything unexpected still shows its contents rather than vanishing
        self.blocks(&node, x, w);
    }
    pub fn baseRun(&self) -> Rc<RefCell<MdRun>> {
        let mut r: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        r.borrow_mut().family = self.style.borrow().bodyFamily.clone();
        if !self.blockFamily.is_empty() {
            r.borrow_mut().family = self.blockFamily.clone();
        }
        r.borrow_mut().size = self.style.borrow().bodySize;
        r.borrow_mut().color = self.style.borrow().textColor.clone();
        if self.quoteDepth > 0 {
            r.borrow_mut().color = self.style.borrow().quoteColor.clone();
        }
        r
    }
    /// A heading's colour where it is: the theme's for its level, or, on a
    /// plate that colour does not read on, the plate's own ink
    /// (MdContainer.inkOnPaper, as the text in it gets).
    pub fn headingInkHere(&self, level: i64) -> String {
        let ink: String = self.style.borrow().headingInk(level);
        if self.plateFill.is_empty() {
            return ink;
        }
        let mut _tmp_2766 = self.plateFill.clone();
        let on: String = MdContainer::inkOnPaper(&_tmp_2766, &ink, &self.platePaper.clone());
        if !on.is_empty() {
            return on;
        }
        ink
    }
    pub fn heading(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        if self.style.borrow().breakOnH1 && node.borrow().level == 1 {
            // A chapter opener starts its own sheet — but not the first one,
            // which would be a blank page before the title.
            if self.paginate && !self.boxes.is_empty() {
                self.page += 1;
                self.pageCount = self.page + 1;
                self.cursorY = self.contentTop;
            }
        }
        if MdLayout::hiddenHeading(node) {
            // `{heading=hidden}`: the slide's name, its anchor and its line
            // in the contents, and nothing on the slide — the room is the
            // pictures'. Recorded at the top of what comes next.
            self.headText.push(node.borrow().plainText());
            self.headLevel.push(node.borrow().level);
            self.headPage.push(self.page);
            self.settlePaper(self.page.clone());
            self.headY.push(self.cursorY);
            self.readEdgesOff(node);
            self.readSlideMeta(node);
            let mut _tmp_2772 = node.borrow().srcStart.clone();
            self.noteHidden(_tmp_2772, self.page.clone());
            self.hiddenOnPage = self.page;
            return;
        }
        let mut size: f64 = self.style.borrow().headingSize(node.borrow().level.clone());
        let mut _tmp_2778 = self.headingBase(node.borrow().level.clone(), size);
        let mut runs: Vec<Rc<RefCell<MdRun>>> = self.inlineRuns(node, &_tmp_2778);
        // A word wider than the column cannot wrap: the heading is set
        // smaller until its widest word fits (a long title on a portrait
        // slide, or one long compound word).
        let mut tries: i64 = 0;
        while tries < 4 {
            let fit: f64 = self.wordFitSize(&runs, size, w);
            if fit < size {
                size = fit;
                runs = self.inlineRuns(node,
                    &self.headingBase(node.borrow().level, size)
                );
                tries += 1;
            } else {
                tries = 4;
            }
        }
        self.cursorY += self.style.borrow().headingSpaceBefore;
        // Recorded BEFORE the lines are placed, so the page is the page the
        // heading starts on rather than the one it may have spilled onto.
        let body_lh: f64 = self.style.borrow().lineHeight;
        let at: f64 = self.reserve(size * body_lh);
        self.headText.push(node.borrow().plainText());
        self.headLevel.push(node.borrow().level);
        self.headPage.push(self.page);
        self.settlePaper(self.page.clone());
        self.headY.push(at);
        self.readEdgesOff(node);
        self.readSlideMeta(node);
        self.paragraph(&runs, x, w);
        if node.borrow().level <= 2 {
            let y: f64 = self.cursorY;
            let arg_0 = MdBox::rect(
                x,
                y + 4.0,
                w,
                1.0,
                self.style.borrow().ruleColor.clone()
            ).clone();
            self.emit(arg_0);
            self.cursorY = y + 5.0;
        }
        self.cursorY += self.style.borrow().headingSpaceAfter;
    }
    pub fn headingBase(&self, level: i64, size: f64) -> Rc<RefCell<MdRun>> {
        let mut base: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        base.borrow_mut().family = self.style.borrow().headingFamily.clone();
        if !self.blockFamily.is_empty() {
            base.borrow_mut().family = self.blockFamily.clone();
        }
        base.borrow_mut().size = size;
        base.borrow_mut().bold = true;
        base.borrow_mut().color = self.headingInkHere(level);
        base
    }
    /// The size a heading set at `size` takes so that its widest word fits in
    /// `w`: `size` when it already does. Never below the body size — a word
    /// that does not fit even there is too long for any heading.
    pub fn wordFitSize(&mut self, runs: &Vec<Rc<RefCell<MdRun>>>, size: f64, w: f64) -> f64 {
        let mut widest: f64 = 0.0;
        for r in runs.iter().cloned() {
            if r.borrow().math.is_empty() {
                let t: String = r.borrow().text.clone();
                let n: i64 = MdText::str_len(&t) as i64;
                let mut i: i64 = 0;
                while i < n {
                    while i < n && MdChar::isSpaceOrTab(MdText::char_at(&t, i)) {
                        i += 1;
                    }
                    let start: i64 = i;
                    while i < n && !MdChar::isSpaceOrTab(MdText::char_at(&t, i)) {
                        i += 1;
                    }
                    if i > start {
                        let ww: f64 = self.measureRun(&MdText::substr(&t, start, i), &r);
                        if ww > widest {
                            widest = ww;
                        }
                    }
                }
            }
        }
        if widest <= w || w <= 0.0 {
            return size;
        }
        let floor: f64 = self.style.borrow().bodySize;
        if floor >= size {
            return size;
        }
        let fit: f64 = (size * (w / widest)) * 0.98;
        if fit < floor {
            return floor;
        }
        fit
    }
    /// A code block: a `diff` block's added and removed lines on green and red
    /// bands, line numbers in a gutter (`.numbers`), highlighted lines
    /// (`lines=`) on a band with a bar at the left — what each line is comes
    /// from MdCodeLines.
    pub fn codeBlock(&mut self, node: Rc<RefCell<MdNode>>, x: f64, w0: f64) {
        // `{width=55%}` under it: that share of the column, and the text
        // after it beside it (startFloat)
        let w: f64 = MdEmbedKinds::widthOf(&self.widthAttr(&node), w0);
        let mut numbers: String = String::new();
        if node.borrow().hasClass("numbers") {
            numbers = "1".to_string();
        }
        if node.borrow().hasAttr("numbers") {
            numbers = node.borrow().attrValue("numbers");
        }
        let mut _tmp_2784 = node.borrow().literal.clone();
        let mut _tmp_2785 = node.borrow().info.clone();
        let mut cl: Rc<RefCell<MdCodeLines>> = MdCodeLines::read(
            &_tmp_2784,
            &_tmp_2785,
            &numbers,
            &node.borrow().attrValue("lines")
        );
        let count: i64 = cl.borrow().lines.len() as i64;
        let mut shown: i64 = count;
        if shown < 1 {
            shown = 1;
        }
        let lh: f64 = self.style.borrow()
            .codeSize * self.style.borrow()
            .codeLineHeight;
        let mut lang: Rc<RefCell<MdLanguage>> = MdCodeHighlight::languageFor(&cl.borrow().lang.clone());
        let mut probe: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        probe.borrow_mut().family = self.style.borrow().monoFamily.clone();
        probe.borrow_mut().size = self.style.borrow().codeSize;
        // the gutter: as wide as the widest number, and a gap
        let mut gutter: f64 = 0.0;
        if cl.borrow().numbers {
            let mut widest: String = String::new();
            let __it_gl = cl.borrow().lines.clone();
            for gl in __it_gl.iter() {
                let nt: String = gl.borrow().number.to_string();
                if gl.borrow().number >= 0 && (MdText::str_len(&nt) as i64) > (MdText::str_len(&widest) as i64) {
                    widest = nt.clone();
                }
            }
            if !widest.is_empty() {
                gutter = self.measureRun(&widest, &probe) + self.style.borrow()
                    .codePadding;
            }
        }
        let tx0: f64 = (x + self.style.borrow().codePadding) + gutter;
        // a diff's markers in a column of their own, so the code lines up
        // whatever the face's widths of +, - and space are
        let mut mark_col: f64 = 0.0;
        if cl.borrow().diff {
            mark_col = self.measureRun("+",
                &probe
            ) + self.measureRun(" ", &probe);
            let dw: f64 = self.measureRun("-",
                &probe
            ) + self.measureRun(" ", &probe);
            if dw > mark_col {
                mark_col = dw;
            }
        }
        // a line wider than the block goes on under itself (MdCodeWrap), so
        // the block is as tall as its rows, not its lines
        let mut rows_in: i64 = 0;
        let mut i0: i64 = 0;
        while i0 < count {
            let mut wl: Rc<RefCell<MdCodeLine>> = cl.borrow().lines[i0 as usize].clone();
            self.wrapCodeLine(&wl, x + w, tx0, mark_col, &probe);
            rows_in = (rows_in + 1) + (wl.borrow().wraps.len() as i64);
            i0 += 1;
        }
        let mut shown_rows: i64 = rows_in;
        if shown_rows < 1 {
            shown_rows = 1;
        }
        let box_h: f64 = lh * (shown_rows as f64) + self.style.borrow()
            .codePadding * 2.0;
        let y: f64 = self.reserve(box_h);
        let arg_0 = MdBox::rect(
            x,
            y,
            w,
            box_h,
            self.style.borrow().codeBackground.clone()
        ).clone();
        self.emit(arg_0);
        let arg_0 = MdBox::outline(
            x,
            y,
            w,
            box_h,
            self.style.borrow().codeBorder.clone(),
            1.0
        ).clone();
        self.emit(arg_0);
        let mut ty: f64 = y + self.style.borrow().codePadding;
        let dark: bool = MdLayout::luminance(&self.style.borrow().codeBackground.clone()) < 0.25;
        let mut added: String = MdLayout::tint(&self.style.borrow().codeBackground.clone(), "#2da44e", 0.2);
        let mut removed: String = MdLayout::tint(&self.style.borrow().codeBackground.clone(), "#cf222e", 0.18);
        let mut add_ink: String = "#1a7f37";
        let mut del_ink: String = "#cf222e";
        let mut add_span: String = MdLayout::tint(&self.style.borrow().codeBackground.clone(), "#2da44e", 0.42);
        let mut del_span: String = MdLayout::tint(&self.style.borrow().codeBackground.clone(), "#cf222e", 0.36);
        if dark {
            added = MdLayout::tint(
                &self.style.borrow().codeBackground,
                "#2ea043",
                0.3
            );
            removed = MdLayout::tint(
                &self.style.borrow().codeBackground,
                "#f85149",
                0.26
            );
            add_span = MdLayout::tint(
                &self.style.borrow().codeBackground,
                "#2ea043",
                0.58
            );
            del_span = MdLayout::tint(
                &self.style.borrow().codeBackground,
                "#f85149",
                0.5
            );
            add_ink = "#3fb950";
            del_ink = "#f85149";
        }
        // a highlight is a lighter (darker) band, its bar an accent blue
        let mut mark_band: String = MdLayout::tint(&self.style.borrow().codeBackground.clone(), "#1f2328", 0.08);
        let mut mark_bar: String = "#0969da";
        let mut num_ink: String = "#57606a";
        if dark {
            mark_band = MdLayout::tint(
                &self.style.borrow().codeBackground,
                "#e6edf3",
                0.13
            );
            mark_bar = "#79c0ff";
            num_ink = "#8b949e";
        }
        self.codeNode = Some(node.clone());
        let build: bool = node.borrow().hasClass("build") && cl.borrow().steps > 0;
        // `lines=` without build steps: the marked lines are what the block is
        // shown for, so the rest of it fades and the eye goes to them instead
        // of reading the whole block
        let dim_rest: bool = cl.borrow().steps > 0 && !build;
        let mut state: i64 = 0;
        let mut i: i64 = 0;
        while i < count {
            let mut ln: Rc<RefCell<MdCodeLine>> = cl.borrow().lines[i as usize].clone();
            // a wrapped line's bands cover all its rows
            let line_h: f64 = lh * ((1 + (ln.borrow().wraps.len() as i64)) as f64);
            if ln.borrow().kind == 1 {
                self.emit(MdBox::rect(x + 1.0, ty, w - 2.0, line_h, added.clone()).clone());
            }
            if ln.borrow().kind == 2 {
                self.emit(MdBox::rect(x + 1.0, ty, w - 2.0, line_h, removed.clone()).clone());
            }
            if ln.borrow().mark > 0 {
                // on a changed line the highlight keeps the line's green
                // (red), a shade deeper, so it still reads as added (removed)
                let mut band_ink: String = mark_band.clone();
                if ln.borrow().kind == 1 {
                    band_ink = MdLayout::tint(&added, mark_bar, 0.12);
                }
                if ln.borrow().kind == 2 {
                    band_ink = MdLayout::tint(&removed, mark_bar, 0.12);
                }
                let mut band: Rc<RefCell<MdBox>> = MdBox::rect(
                    x + 1.0,
                    ty,
                    w - 2.0,
                    line_h,
                    band_ink.clone()
                );
                let mut bar: Rc<RefCell<MdBox>> = MdBox::rect(
                    x + 1.0,
                    ty,
                    3.0,
                    line_h,
                    mark_bar.to_string()
                );
                if build {
                    band.borrow_mut().buildStep = ln.borrow().mark;
                    bar.borrow_mut().buildStep = ln.borrow().mark;
                }
                self.emit(band.clone());
                self.emit(bar.clone());
            }
            // a diff's added and removed lines are the change: they keep
            // their ink when lines= points at a few of them
            self.codeDim = (dim_rest && ln.borrow().mark == 0) && (ln.borrow().kind != 1 && ln.borrow().kind != 2);
            if (cl.borrow().numbers && ln.borrow().number >= 0) && gutter > 0.0 {
                let nt2: String = ln.borrow().number.to_string();
                let nw: f64 = self.measureRun(&nt2, &probe);
                let mut nb: Rc<RefCell<MdBox>> = MdBox::textBox(
                    nt2.clone(),
                    (x + self.style.borrow().codePadding) + ((gutter - self.style.borrow().codePadding) - nw),
                    ty,
                    nw,
                    lh
                );
                nb.borrow_mut().family = self.style.borrow().monoFamily.clone();
                nb.borrow_mut().size = self.style.borrow().codeSize;
                nb.borrow_mut().color = self.codeInk(num_ink);
                // a gutter number is drawn dim on purpose and read with its
                // line: not the text a contrast check is for
                nb.borrow_mut().quiet = true;
                self.emit(nb.clone());
            }
            let mut lx: f64 = tx0;
            if !ln.borrow().marker.is_empty() {
                let mw: f64 = self.measureRun(&ln.borrow().marker.clone(), &probe);
                if ln.borrow().marker != " " {
                    let mut mb: Rc<RefCell<MdBox>> = MdBox::textBox(ln.borrow().marker.clone(), lx, ty, mw, lh);
                    mb.borrow_mut().family = self.style.borrow().monoFamily.clone();
                    mb.borrow_mut().size = self.style.borrow().codeSize;
                    mb.borrow_mut().color = add_ink.to_string();
                    if ln.borrow().kind == 2 {
                        mb.borrow_mut().color = del_ink.to_string();
                    }
                    let _tmp_2806 = self.codeInk(&mb.borrow().color.clone());
                    mb.borrow_mut().color = _tmp_2806;
                    self.emit(mb.clone());
                }
                lx += mark_col;
            }
            // what changed inside a changed line, a stronger band behind
            // just those characters (MdCodeLines.changeOf)
            if ln.borrow().changeFrom >= 0 && (ln.borrow().kind == 1 || ln.borrow().kind == 2) {
                let mut sc: String = add_span.clone();
                if ln.borrow().kind == 2 {
                    sc = del_span.clone();
                }
                self.changeSpans(&ln, lx, ty, lh, &self.codeInk(&sc), &probe);
            }
            let row_w: f64 = (w - (lx - x)) - self.style.borrow().codePadding;
            if ln.borrow().kind == 3 || ln.borrow().kind == 4 {
                self.codeAt = ln.borrow().litAt;
                let mut _tmp_2809 = ln.borrow().text.clone();
                let mut _tmp_2810 = self.style.borrow().codeComment.clone();
                self.codeRows(&ln, &_tmp_2809, lx, ty, lh, row_w, &_tmp_2810, ln.borrow().kind == 4);
            } else {
                self.codeAt = -1;
                if ln.borrow().litAt >= 0 {
                    self.codeAt = ln.borrow().litAt + (MdText::str_len(&ln.borrow().marker) as i64);
                }
                if self.style.borrow().highlightCode {
                    let mut _tmp_2812 = ln.borrow().code.clone();
                    let mut hl: Rc<RefCell<MdHighlightedLine>> = MdCodeHighlight::line(
                        &_tmp_2812,
                        &lang,
                        state
                    );
                    state = hl.borrow().state;
                    self.codeLine(&hl, &ln, lx, ty, lh);
                } else {
                    let mut _tmp_2813 = ln.borrow().code.clone();
                    self.codeRows(&ln, &_tmp_2813, lx, ty, lh, row_w, &self.style.borrow().textColor.clone(), false);
                }
                // a line of no text (or only spaces): a stop at its end
                if self.caretStops && ln.borrow().code.trim().to_string().is_empty() {
                    let mut _tmp_2816 = self.measureRun(&ln.borrow().code.clone(), &probe);
                    let mut stop: Rc<RefCell<MdBox>> = MdBox::textBox(
                        String::new(),
                        lx + _tmp_2816,
                        ty,
                        0.0,
                        lh
                    );
                    stop.borrow_mut().family = self.style.borrow().monoFamily.clone();
                    stop.borrow_mut().size = self.style.borrow().codeSize;
                    stop.borrow_mut().segSrc = self.codeSrc((MdText::str_len(&ln.borrow().code) as i64));
                    stop.borrow_mut().segLen = 0;
                    self.emit(stop.clone());
                }
            }
            ty += line_h;
            i += 1;
        }
        self.codeDim = false;
        self.codeAt = -1;
        self.cursorY = (y + box_h) + self.style.borrow().paraSpacing;
    }
    /// The source offset of the k-th character of the code being laid (-1
    /// when it has none).
    pub fn codeSrc(&self, k: i64) -> i64 {
        if self.codeAt < 0 || self.codeNode.is_none() {
            return -1;
        }
        let mut cn: Rc<RefCell<MdNode>> = self.codeNode.clone().unwrap();
        return cn.borrow().srcOfLiteral(self.codeAt + k);
    }
    /// A code box's source: the run [k, k + len) of the code being laid.
    pub fn codeSeg(&self, b: &Rc<RefCell<MdBox>>, k: i64, __len: i64) {
        let at: i64 = self.codeSrc(k);
        if at >= 0 {
            b.borrow_mut().segSrc = at;
            b.borrow_mut().segLen = __len;
        }
    }
    /// a code line's ink: as given, or faded toward the block's background on
    /// a line outside the marks (codeDim)
    pub fn codeInk(&self, color: &str) -> String {
        if self.codeDim {
            return MdLayout::tint(color, &self.style.borrow().codeBackground.clone(), 0.62);
        }
        color.to_string()
    }
    /// Where a code line's rows begin when it is wider than the block
    /// (MdCodeWrap decides, from each character's width): into ln.wraps and
    /// ln.wrapIndent. right is the block's right edge.
    pub fn wrapCodeLine(&mut self, ln: &Rc<RefCell<MdCodeLine>>, right: f64, tx0: f64, mark_col: f64, probe: &Rc<RefCell<MdRun>>) {
        ln.borrow_mut().wraps.clear();
        ln.borrow_mut().wrapIndent = 0.0;
        let mut text: String = ln.borrow().code.clone();
        let mut lx: f64 = tx0;
        if ln.borrow().kind == 3 || ln.borrow().kind == 4 {
            text = ln.borrow().text.clone();
        } else {
            if !ln.borrow().marker.is_empty() {
                lx += mark_col;
            }
        }
        let avail: f64 = (right - lx) - self.style.borrow().codePadding;
        if text.is_empty() {
            return;
        }
        // most lines fit, and one measure says so
        if self.measureRun(&text, probe) <= avail + 0.01 {
            return;
        }
        let mut units: Vec<String> = MdCodeWrap::units(&text);
        let mut widths: Vec<f64> = Vec::new();
        for u in units.iter().cloned() {
            widths.push(self.measureRun(&u, probe));
        }
        let indent: f64 = MdCodeWrap::indentOf(&units, &widths, avail);
        let mut rows: Vec<i64> = MdCodeWrap::rows(&units, &widths, avail, avail - indent);
        let mut offs: Vec<i64> = MdCodeWrap::offsets(&units, &rows);
        for o in offs.iter().copied() {
            ln.borrow_mut().wraps.push(o);
        }
        ln.borrow_mut().wrapIndent = indent;
    }
    /// the band behind a line's changed characters, cut where the line wraps
    pub fn changeSpans(&mut self, ln: &Rc<RefCell<MdCodeLine>>, lx: f64, ty: f64, lh: f64, color: &str, probe: &Rc<RefCell<MdRun>>) {
        let mut rws: Vec<i64> = MdLayout::rowStarts(ln);
        let nr: i64 = (rws.len() as i64) - 1;
        let mut r: i64 = 0;
        while r < nr {
            let a: i64 = rws[r as usize];
            let e: i64 = rws[(r + 1) as usize];
            let mut s0: i64 = a;
            if ln.borrow().changeFrom > s0 {
                s0 = ln.borrow().changeFrom;
            }
            let mut e0: i64 = e;
            if ln.borrow().changeTo < e0 {
                e0 = ln.borrow().changeTo;
            }
            if s0 < e0 {
                let mut rx: f64 = lx;
                if r > 0 {
                    rx = lx + ln.borrow().wrapIndent;
                }
                let sx: f64 = rx + self.measureRun(&MdText::substr(&ln.borrow().code, a, s0),
                    probe
                );
                let sw: f64 = self.measureRun(&MdText::substr(&ln.borrow().code, s0, e0),
                    probe
                );
                let arg_0 = MdBox::rect(
                    sx,
                    (ty + lh * (r as f64)) + 1.0,
                    sw,
                    lh - 2.0,
                    color.to_string()
                ).clone();
                self.emit(arg_0);
            }
            r += 1;
        }
    }
    /// a line in one colour, row by row as it wraps
    pub fn codeRows(&mut self, ln: &Rc<RefCell<MdCodeLine>>, text: &str, lx: f64, ty: f64, lh: f64, row_w: f64, color: &str, bold: bool) {
        let mut rws: Vec<i64> = MdLayout::rowStarts(ln);
        let nr: i64 = (rws.len() as i64) - 1;
        let mut r: i64 = 0;
        while r < nr {
            let mut rx: f64 = lx;
            if r > 0 {
                rx = lx + ln.borrow().wrapIndent;
            }
            let mut b: Rc<RefCell<MdBox>> = MdBox::textBox(
                MdText::substr(&text, rws[r as usize], rws[(r + 1) as usize]),
                rx,
                ty + lh * (r as f64),
                row_w - (rx - lx),
                lh
            );
            b.borrow_mut().family = self.style.borrow().monoFamily.clone();
            b.borrow_mut().size = self.style.borrow().codeSize;
            b.borrow_mut().color = self.codeInk(color);
            b.borrow_mut().quiet = self.codeDim;
            b.borrow_mut().bold = bold;
            self.codeSeg(&b, rws[r as usize], (MdText::str_len(&b.borrow().text) as i64));
            self.emit(b.clone());
            r += 1;
        }
    }
    /// One line of code, token by token. Each is measured and placed after the
    /// one before it, exactly as a paragraph's segments are — the face is the
    /// same for all of them, so what varies is only the colour, and the run
    /// still has to be measured because the face is not monospaced. A token
    /// that crosses one of the line's wraps is cut there and goes on at the
    /// start of the next row.
    pub fn codeLine(&mut self, hl: &Rc<RefCell<MdHighlightedLine>>, ln: &Rc<RefCell<MdCodeLine>>, x0: f64, y0: f64, lh: f64) {
        let mut probe: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        probe.borrow_mut().family = self.style.borrow().monoFamily.clone();
        probe.borrow_mut().size = self.style.borrow().codeSize;
        let mut cx: f64 = x0;
        let mut y: f64 = y0;
        // where the next row begins in the line, and where this token is
        let mut wi: i64 = 0;
        let mut next_wrap: i64 = -1;
        if !ln.borrow().wraps.is_empty() {
            next_wrap = ln.borrow().wraps[0];
        }
        let mut pos: i64 = 0;
        let mut i: i64 = 0;
        let n: i64 = hl.borrow().tokens.len() as i64;
        while i < n {
            let mut t: Rc<RefCell<MdToken>> = hl.borrow().tokens[i as usize].clone();
            let mut rest: String = t.borrow().text.clone();
            let mut going: bool = true;
            while going {
                let rlen: i64 = MdText::str_len(&rest) as i64;
                if next_wrap >= 0 && pos + rlen > next_wrap {
                    let cut: i64 = next_wrap - pos;
                    if cut > 0 {
                        self.codeK = pos;
                        cx = self.codeToken(&MdText::substr(&rest, 0, cut),
                            t.borrow().kind,
                            cx,
                            y,
                            &probe,
                            lh
                        );
                    }
                    rest = MdText::substr(&rest, cut, rlen);
                    pos = next_wrap;
                    wi += 1;
                    next_wrap = -1;
                    if wi < (ln.borrow().wraps.len() as i64) {
                        next_wrap = ln.borrow().wraps[wi as usize];
                    }
                    cx = x0 + ln.borrow().wrapIndent;
                    y += lh;
                } else {
                    self.codeK = pos;
                    cx = self.codeToken(&rest, t.borrow().kind, cx, y, &probe, lh);
                    pos += rlen;
                    going = false;
                }
            }
            i += 1;
        }
    }
    /// One token (or the part of it on this row) at cx; returns where the next
    /// one starts.
    pub fn codeToken(&mut self, text: &str, kind: i64, cx0: f64, y: f64, probe: &Rc<RefCell<MdRun>>, lh: f64) -> f64 {
        let mut cx: f64 = cx0;
        let tlen: i64 = MdText::str_len(&text) as i64;
        if tlen == 0 {
            return cx;
        }
        // The whitespace at either end of a token is laid as advance
        // rather than as text — the same rule a paragraph's segments
        // follow, and for the same reason: a space has no glyph, and a
        // consumer that trims the edges of a text run then draws the
        // geometry the PDF draws. Without it `const frame` prints
        // correctly and previews as `constframe`.
        let lead: i64 = MdLayout::leadingSpaces(text);
        let trail: i64 = MdLayout::trailingSpaces(text, tlen, lead);
        if lead > 0 {
            let gap: String = MdText::substr(&text, 0, lead);
            cx += self.measureRun(&gap, probe);
        }
        let core: String = MdText::substr(&text, lead, tlen - trail);
        let clen: i64 = MdText::str_len(&core) as i64;
        if clen > 0 {
            let wid: f64 = self.measureRun(&core, probe);
            let mut b: Rc<RefCell<MdBox>> = MdBox::textBox(core.clone(), cx, y, wid, lh);
            b.borrow_mut().family = self.style.borrow().monoFamily.clone();
            b.borrow_mut().size = self.style.borrow().codeSize;
            b.borrow_mut().color = self.codeInk(&self.style.borrow().codeColor(kind));
            b.borrow_mut().quiet = self.codeDim;
            self.codeSeg(&b, self.codeK + lead, clen);
            self.emit(b.clone());
            cx += wid;
        }
        if trail > 0 {
            let tail: String = MdText::substr(&text, tlen - trail, tlen);
            cx += self.measureRun(&tail, probe);
        }
        cx
    }
    pub fn blockQuote(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let pad: f64 = self.style.borrow().quotePadY;
        let top: f64 = self.cursorY;
        let inner_x: f64 = (x + self.style.borrow().quoteBarWidth) + self.style.borrow()
            .quoteIndent;
        let inner_w: f64 = w - (self.style.borrow().quoteBarWidth + self.style.borrow().quoteIndent);
        let start_page: i64 = self.page;
        let before: i64 = self.boxes.len() as i64;
        self.cursorY += pad;
        self.quoteDepth += 1;
        self.openPad += pad;
        // A quote is a block with paragraphs in it wherever it stands, even
        // inside a tight list item.
        let was_tight: bool = self.inTightItem;
        self.inTightItem = false;
        self.blocks(node, inner_x, inner_w);
        self.inTightItem = was_tight;
        self.openPad -= pad;
        self.quoteDepth -= 1;
        // Where the CONTENT ended, which is not where the cursor is: `blocks`
        // leaves the cursor past the last child's own trailing space, and that
        // space belongs OUTSIDE the bar. Reading the bottom off the boxes it
        // just emitted works whatever the last child turned out to be — a
        // paragraph, a list, a table, a nested quote, a drawing — where
        // subtracting a paragraph's spacing would only work for a paragraph.
        // (on a sheet after the one it began on, from that sheet's top: `top`
        // is a place on the first one)
        // On a sheet after the one it began on, its part there begins at its
        // first box (under the title said again, `retold`), not at the top.
        let mut ink_bottom: f64 = top + pad;
        let mut part_top: f64 = top;
        if self.page != start_page {
            ink_bottom = self.contentTop;
            part_top = self.contentBottom;
        }
        let mut i: i64 = before;
        let n: i64 = self.boxes.len() as i64;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            // On this sheet only. A quote that crossed a break has boxes whose
            // y is measured from the top of another page.
            if b.borrow().page == self.page && !b.borrow().retold {
                let edge: f64 = b.borrow().y + b.borrow().h;
                if edge > ink_bottom {
                    ink_bottom = edge;
                }
                if self.page != start_page && b.borrow().y - pad < part_top {
                    part_top = b.borrow().y - pad;
                }
            }
            i += 1;
        }
        if part_top < self.contentTop {
            part_top = self.contentTop;
        }
        if part_top > ink_bottom {
            part_top = ink_bottom;
        }
        let bottom: f64 = ink_bottom + pad;
        // `blockquote { background-color }`: a band under the quote on its
        // last page, put before its boxes so it is painted first
        if !self.style.borrow().quoteBackground.is_empty() {
            let band_top: f64 = part_top;
            let mut band: Rc<RefCell<MdBox>> = MdBox::rect(x, band_top, w, bottom - band_top, self.style.borrow().quoteBackground.clone());
            band.borrow_mut().page = self.page;
            band.borrow_mut().srcStart = self.currentSrc;
            let mut first_here: i64 = self.boxes.len() as i64;
            let mut fi: i64 = (self.boxes.len() as i64) - 1;
            while fi >= before {
                let mut fb: Rc<RefCell<MdBox>> = self.boxes[fi as usize].clone();
                if fb.borrow().page == self.page {
                    first_here = fi;
                }
                fi -= 1;
            }
            let mut one: Vec<Rc<RefCell<MdBox>>> = Vec::new();
            one.push(band.clone());
            self.insertBoxes(first_here, &one);
        }
        // The bar is drawn per page, so a quote that spans a break gets one on
        // each sheet rather than a single stripe that goes off the bottom.
        if self.page == start_page {
            let arg_0 = MdBox::rect(
                x,
                top,
                self.style.borrow().quoteBarWidth,
                bottom - top,
                self.style.borrow().quoteBarColor.clone()
            ).clone();
            self.emit(arg_0);
        } else {
            let arg_0 = MdBox::rect(
                x,
                part_top,
                self.style.borrow().quoteBarWidth,
                bottom - part_top,
                self.style.borrow().quoteBarColor.clone()
            ).clone();
            self.emit(arg_0);
        }
        self.cursorY = bottom + self.style.borrow().paraSpacing;
    }
    pub fn list(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let items: i64 = node.borrow().childCount();
        let mut i: i64 = 0;
        let mut number: i64 = node.borrow().listStart;
        self.listDepth += 1;
        let was_tight: bool = self.inTightItem;
        // `li { text-transform … }`: what the items' text inherits
        let mut list_typo: Rc<RefCell<MdTypo>> = self.typo.clone().clone();
        let mut lt: Rc<RefCell<MdTypo>> = self.typo.borrow().copy();
        lt.borrow_mut().over(&self.style.borrow().typoOf("li"));
        self.typo = lt.clone();
        // How far the text is indented, which is as far as the WIDEST marker
        // needs and never less than the style's own step.
        //
        // It was the step alone. `1.` fits inside it and `100.` does not, so a
        // list long enough to reach three digits wrote its numbers across the
        // first letters of its own items — and the box the marker claimed
        // stayed the same width as the number grew, which is a lie anything
        // that clips or hit-tests a box would have believed.
        let mut probe: Rc<RefCell<MdRun>> = self.baseRun();
        let mut widest: f64 = 0.0;
        let mut probe_num: i64 = node.borrow().listStart;
        let mut k: i64 = 0;
        while k < items {
            let mut probe_item: Rc<RefCell<MdNode>> = node.borrow().childAt(k);
            let pm: String = MdLayout::markerFor(node, &probe_item, probe_num);
            if node.borrow().listOrdered {
                probe_num += 1;
            }
            let pw: f64 = self.measureRun(&pm, &probe);
            if pw > widest {
                widest = pw;
            }
            k += 1;
        }
        let mut indent: f64 = self.style.borrow().listIndent;
        let need: f64 = widest + self.style.borrow().markerGap;
        if need > indent {
            indent = need;
        }
        while i < items {
            let mut item: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            let marker: String = MdLayout::markerFor(node, &item, number);
            if node.borrow().listOrdered {
                number += 1;
            }
            let inner_x: f64 = x + indent;
            let inner_w: f64 = w - indent;
            let mut marker_y: f64 = self.cursorY;
            let mut marker_page: i64 = self.page;
            let mut first_box: i64 = self.boxes.len() as i64;
            let start_page: i64 = self.page;
            let start_y: f64 = self.cursorY;
            let start_overflows: i64 = self.slideOverflows;
            self.inTightItem = node.borrow().listTight;
            self.blocks(&item, inner_x, inner_w);
            // An empty item ("- " and nothing after it) is still a line: its
            // marker on a line of its own, not on top of the next item's.
            let mut empty_at: f64 = -1.0;
            if item.borrow().childCount() == 0 {
                let elh: f64 = self.style.borrow()
                    .bodySize * self.style.borrow()
                    .lineHeight;
                empty_at = self.reserve(elh);
                self.cursorY = (empty_at + elh) + (if node.borrow().listTight { self.style.borrow().listTightSpacing } else { self.style.borrow().paraSpacing });
                if self.caretStops {
                    // where its words would start: after the marker and
                    // its space, before the newline that ends the item
                    // ("-" alone has no space to stand after)
                    let mut stop: Rc<RefCell<MdBox>> = MdBox::textBox(
                        String::new(),
                        inner_x,
                        empty_at,
                        0.0,
                        elh
                    );
                    stop.borrow_mut().family = self.style.borrow().bodyFamily.clone();
                    stop.borrow_mut().size = self.style.borrow().bodySize;
                    let mut words_at: i64 = (item.borrow().srcStart + item.borrow().markerOffset) + item.borrow().padding;
                    if words_at > item.borrow().srcEnd - 1 {
                        words_at = item.borrow().srcEnd - 1;
                    }
                    stop.borrow_mut().segSrc = words_at;
                    stop.borrow_mut().segLen = 0;
                    self.emit(stop.clone());
                }
            }
            // On a slide an item is not cut in two when it fits on one: its
            // bullet on one slide and its words on the next reads as a slide
            // that lost a line. Undone and laid out again on the next slide.
            if (self.slides && self.paginate) && self.page > start_page {
                let item_h: f64 = self.itemHeight(first_box, start_y, start_page);
                let full: f64 = self.contentBottom - self.contentTop;
                if item_h <= full && start_y > self.contentTop + 0.5 {
                    self.truncate(first_box);
                    self.page = start_page;
                    self.pageCount = self.page + 1;
                    self.forgetPagesAfter(self.page.clone());
                    self.slideOverflows = start_overflows;
                    // the next reserve starts the next slide, with its title
                    self.cursorY = self.contentBottom;
                    first_box = self.boxes.len() as i64;
                    self.blocks(&item, inner_x, inner_w);
                }
            }
            self.inTightItem = was_tight;
            // The marker goes where the item's first line went: the page and
            // the top of its first box.
            if first_box < (self.boxes.len() as i64) {
                let mut fb: Rc<RefCell<MdBox>> = self.firstTextBox(first_box);
                marker_y = fb.borrow().y;
                marker_page = fb.borrow().page;
            } else {
                if empty_at >= 0.0 {
                    marker_y = empty_at;
                    marker_page = self.page;
                }
            }
            let saved_page: i64 = self.page;
            self.page = marker_page;
            let marker_w: f64 = self.measureRun(&marker, &probe);
            let mut mb: Rc<RefCell<MdBox>> = MdBox::textBox(
                marker.clone(),
                x,
                marker_y,
                marker_w,
                self.style.borrow().bodySize * self.style.borrow().lineHeight
            );
            mb.borrow_mut().family = self.style.borrow().bodyFamily.clone();
            mb.borrow_mut().size = self.style.borrow().bodySize;
            mb.borrow_mut().color = if (!self.style.borrow().markerColor.is_empty()) { self.style.borrow().markerColor.clone() } else { self.style.borrow().mutedColor.clone() };
            self.emit(mb.clone());
            self.page = saved_page;
            if false == node.borrow().listTight {
                self.cursorY += 2.0;
            }
            i += 1;
        }
        self.typo = list_typo.clone();
        self.listDepth -= 1;
        // The list's own separation from whatever follows it, which the items
        // stopped providing when the list went tight. Only the OUTERMOST list
        // gets it: a nested list is followed by its parent's next item, and an
        // item's worth of air is what that wants.
        if self.listDepth == 0 {
            if node.borrow().listTight {
                let extra: f64 = self.style.borrow()
                    .paraSpacing - self.style.borrow()
                    .listTightSpacing;
                if extra > 0.0 {
                    self.cursorY += extra;
                }
            }
        }
    }
    /// How tall an item laid out from box `from` on is, counting from where it
    /// started — across a slide break, the part on each slide.
    pub fn itemHeight(&self, from: i64, start_y: f64, start_page: i64) -> f64 {
        let mut total: f64 = 0.0;
        let mut top: f64 = start_y;
        let mut bottom: f64 = start_y;
        let mut pg: i64 = start_page;
        let mut i: i64 = from;
        while i < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b.borrow().page != pg {
                total += bottom - top;
                pg = b.borrow().page;
                top = b.borrow().y;
                bottom = b.borrow().y;
            }
            if b.borrow().y + b.borrow().h > bottom {
                bottom = b.borrow().y + b.borrow().h;
            }
            i += 1;
        }
        total + (bottom - top)
    }
    /// The first text box from `from` on — not a continuation title — or the
    /// first box at all.
    pub fn firstTextBox(&self, from: i64) -> Rc<RefCell<MdBox>> {
        let mut i: i64 = from;
        while i < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b.borrow().kind == 0 && b.borrow().segSrc >= 0 {
                return b;
            }
            if b.borrow().kind == 5 {
                return b;
            }
            i += 1;
        }
        self.boxes[from as usize].clone()
    }
    /// A fenced diagram. The layout cannot draw one and must still leave a
    /// hole of the right height — so it asks the cache, which a handler filled
    /// before layout began. A diagram nobody prepared is a labelled slot: an
    /// empty rectangle tells a reader nothing, and a silent omission tells
    /// them less.
    /// The pictures a chart draws (its `image` marks, which name them as the
    /// document does: `media/logo.svg`), resolved against this layout's
    /// pictures the way `![](media/logo.svg)` is, and each fitted inside the
    /// box the chart gave it with its own proportions (Vega's `aspect`, on by
    /// default). A display list stretches a picture to its box, so the fit is
    /// made here. A name the store does not have draws nothing and is counted
    /// in `missingImages`. Works from the name the scene kept in `alt`, so a
    /// second pass over the same drawing gives the same boxes.
    fn chartPictures(&mut self, root: &Rc<RefCell<EVGElement>>) {
        let mut i: i64 = 0;
        while i < (root.borrow().children.len() as i64) {
            let mut c: Rc<RefCell<EVGElement>> = root.borrow().children[i as usize].clone();
            i += 1;
            if c.borrow().tag_name == "img" && !c.borrow().alt.is_empty() {
                c.borrow_mut().src = String::new();
                if self.haveImages {
                    let mut _tmp_2846 = c.borrow().alt.clone();
                    let path: String = Vfs::resolve(&self.images, &_tmp_2846, &self.docDir.clone());
                    let mut st: Rc<RefCell<VfsStat>> = Rc::new(RefCell::new(VfsStat::new()));
                    if !path.is_empty() {
                        st = Vfs::stat(&self.images, &path);
                    }
                    if st.borrow().found {
                        c.borrow_mut().src = path.clone();
                        if st.borrow().pixelW > 0 && st.borrow().pixelH > 0 {
                            MdLayout::containIn(&c, st.borrow().pixelW as f64, st.borrow().pixelH as f64);
                        }
                    } else {
                        self.missed(&format!("chart:{}", c.borrow().alt).clone());
                    }
                } else {
                    self.missed(&format!("chart:{}", c.borrow().alt).clone());
                }
            }
        }
    }
    pub fn embed(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let kind: String = MdLayout::firstWord(&node.borrow().info.clone());
        // `{width=360}` under the fence, clamped to the column. `MdDiagram`
        // works the same number out of the same attribute before layout
        // begins, and it has to be the same number: the cache is keyed by the
        // width the diagram was prepared at, so one point of disagreement is a
        // slot with an apology in it instead of a drawing.
        let want: String = self.widthAttr(node);
        let use_w: f64 = MdEmbedKinds::widthOf(&want, w);
        let known: f64 = self.embeds.borrow().heightFor(&kind, &node.borrow().literal.clone(), use_w);
        let drawn: bool = known > 0.0;
        if drawn {
            let pe: i64 = self.embeds.borrow().find(&kind, &node.borrow().literal, use_w);
            if pe >= 0 {
                let mut pentry: Rc<RefCell<MdEmbedEntry>> = self.embeds.borrow().entryAt(pe);
                if pentry.borrow().ok && pentry.borrow().root.is_some() {
                    self.chartPictures(&pentry.borrow().root.clone().unwrap().clone());
                }
            }
        }
        let mut h: f64 = 64.0;
        if drawn {
            h = known;
        }
        // On a slide a diagram taller than the room left under its heading is
        // given that room rather than a slide of its own: the deck scales a
        // scene into its box, and a heading alone on one slide with its
        // diagram on the next reads as two broken slides. Only when most of
        // the slide is still free; a diagram under a full slide moves on.
        //
        // The room is less what is written under it on the same slide
        // (`followH`, as a picture's): a chart filling the slide down to the
        // margin pushed a one-line source or takeaway under it onto a slide
        // of its own. Text that goes beside a narrow one (startFloat) takes
        // no room under it.
        if self.slides && self.paginate {
            let full: f64 = self.contentBottom - self.contentTop;
            let mut avail: f64 = self.contentBottom - self.cursorY;
            let room: f64 = avail;
            // …and the space between them
            let mut below: f64 = 0.0;
            if self.followH > 0.0 {
                below = self.followH + self.style.borrow().embedSpacing;
            }
            if (self.followBeside && !want.is_empty()) && use_w <= self.contentW * 0.75 {
                if (self.contentW - use_w) - self.style.borrow()
                    .bodySize * 1.2 >= self.contentW * 0.25 {
                    below = 0.0;
                }
            }
            if below > 0.0 && avail - below >= full * 0.3 {
                avail -= below;
            }
            let under: bool = self.slideTitleOnly && room >= full * 0.2;
            let held: bool = h > avail && (room >= full * 0.45 || under);
            if held {
                h = avail;
            }
            if h > full {
                h = full;
            }
            // A chart or figure is drawn for a room: one cut to a smaller
            // box would run over what is under it, so it is drawn again for
            // the box (MdDiagram.refit); one held smaller before and with
            // more room now is drawn for that.
            let notation: String = MdEmbedKinds::notationOf(&kind);
            if (drawn && !self.growTrial) && ((notation == "vega" || notation == "figure") || notation == "app") {
                let had: f64 = self.embeds.borrow().fitFor(&kind, &node.borrow().literal.clone(), use_w);
                if held {
                    let mut _tmp_2857 = node.borrow().literal.clone();
                    self.embeds.clone().borrow_mut().askRoom(&kind, &_tmp_2857, use_w, h);
                } else {
                    if had > 0.0 && avail > had + 1.0 {
                        let mut _tmp_2860 = node.borrow().literal.clone();
                        self.embeds.clone().borrow_mut().askRoom(&kind, &_tmp_2860, use_w, avail);
                    } else {
                        // a chart that grows to its room and says no height,
                        // drawn for the room a slide keeps on any slide
                        // (`chartRoomKept`) and shorter than the room this
                        // slide has under its title: drawn for this room
                        if ((notation == "vega" && had <= 0.0) && self.embeds.borrow().chartGrow > 1.0) && (avail > h + 8.0 && !MdEmbedKinds::statesHeight(&node.borrow().literal.clone())) {
                            let mut _tmp_2864 = node.borrow().literal.clone();
                            self.embeds.clone().borrow_mut().askRoom(&kind, &_tmp_2864, use_w, avail);
                        }
                    }
                }
            }
        }
        let y: f64 = self.reserve(h);
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 3;
        b.borrow_mut().x = x;
        b.borrow_mut().y = y;
        b.borrow_mut().w = use_w;
        b.borrow_mut().h = h;
        b.borrow_mut().embedKind = kind.clone();
        b.borrow_mut().embedSource = node.borrow().literal.clone();
        self.emit(b.clone());
        if false == drawn {
            b.borrow_mut().stroke = self.style.borrow().codeBorder.clone();
            b.borrow_mut().strokeWidth = 1.0;
            let mut note: String = self.embeds.borrow().noteFor(&kind, &node.borrow().literal.clone(), use_w);
            let nlen: i64 = MdText::str_len(&note) as i64;
            if nlen == 0 {
                note = format!("{} — no handler prepared this one", kind);
            }
            b.borrow_mut().embedNote = note.clone();
            // said as plainly as the text round it: a muted italic line was
            // read as decoration, not as the reason the chart is missing
            if nlen > 0 {
                note = format!("⚠ {}: {}", kind, note);
            }
            let mut label: Rc<RefCell<MdBox>> = MdBox::textBox(
                note.clone(),
                x + self.style.borrow().codePadding,
                y + self.style.borrow().codePadding,
                use_w - self.style.borrow().codePadding * 2.0,
                self.style.borrow().bodySize * self.style.borrow().lineHeight
            );
            label.borrow_mut().family = self.style.borrow().bodyFamily.clone();
            label.borrow_mut().size = self.style.borrow().bodySize;
            label.borrow_mut().color = self.style.borrow().textColor.clone();
            self.emit(label.clone());
        }
        self.cursorY = (y + h) + self.style.borrow().embedSpacing;
    }
    /// ----------------------------------------------------------------------
    /// Inlines → runs
    /// ----------------------------------------------------------------------
    /// Returning the array rather than filling one that was passed in.
    ///
    /// `push` into an array PARAMETER works on JavaScript and does nothing on
    /// Go, where the parameter is a slice and the append lands on a copy. The
    /// JavaScript suite passed and the Go suite laid out an empty page; there
    /// is no warning in between, so the rule here is that an array is a return
    /// value and never an out-parameter.
    /// The runs of a block's inline content, in the typography it holds
    /// (`typo`): bold or not, italic or not and the spacing on the base run
    /// every other copies, then the letters changed as `text-transform` says
    /// — not in code, which is what it is.
    pub fn inlineRuns(&mut self, node: &Rc<RefCell<MdNode>>, base: &Rc<RefCell<MdRun>>) -> Vec<Rc<RefCell<MdRun>>> {
        if self.typo.borrow().isEmpty() {
            return self.collectRuns(node, base);
        }
        if self.typo.borrow().weight == 1 {
            base.borrow_mut().bold = true;
        }
        if self.typo.borrow().weight == 0 {
            base.borrow_mut().bold = false;
        }
        if self.typo.borrow().italic == 1 {
            base.borrow_mut().italic = true;
        }
        if self.typo.borrow().italic == 0 {
            base.borrow_mut().italic = false;
        }
        if self.typo.borrow().spacingSet {
            let mut _tmp_2875 = self.typo.borrow().spacing.clone();
            let _tmp_2877 = MdLayout::spacingFor(_tmp_2875, base.borrow().size.clone());
            base.borrow_mut().spacing = _tmp_2877;
        }
        let mut runs: Vec<Rc<RefCell<MdRun>>> = self.collectRuns(node, base);
        let t: String = self.typo.borrow().transform.clone();
        if !t.is_empty() && t != "none" {
            for r in runs.iter().cloned() {
                if !r.borrow().code && r.borrow().math.is_empty() {
                    let _tmp_2879 = MdTypo::apply(&t, &r.borrow().text.clone());
                    r.borrow_mut().text = _tmp_2879;
                }
            }
        }
        runs
    }
    pub fn collectRuns(&mut self, node: &Rc<RefCell<MdNode>>, base: &Rc<RefCell<MdRun>>) -> Vec<Rc<RefCell<MdRun>>> {
        let mut out: Vec<Rc<RefCell<MdRun>>> = Vec::new();
        // the style inline HTML has switched on, innermost last
        let mut cur: Rc<RefCell<MdRun>> = base.clone();
        let mut outer: Vec<Rc<RefCell<MdRun>>> = Vec::new();
        let mut i: i64 = 0;
        let n: i64 = node.borrow().childCount();
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            let kind: String = c.borrow().kind.clone();
            if kind == "text" {
                let mut r: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                r.borrow_mut().text = c.borrow().literal.clone();
                r.borrow_mut().srcStart = c.borrow().srcStart;
                r.borrow_mut().srcEnd = c.borrow().srcEnd;
                r.borrow_mut().atom = c.borrow().subst;
                out.push(r.clone());
            }
            // markdown-it's spans: drawn as their HTML tags are
            if ((kind == "sub" || kind == "sup") || kind == "ins") || (kind == "mark" || kind == "abbr") {
                // An abbreviation has nowhere to show its title on a slide, so
                // it is its text; the title goes to HTML as <abbr title>.
                let mut xr: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                if kind != "abbr" {
                    xr = self.tagStyle(&cur, &kind);
                }
                let mut nested: Vec<Rc<RefCell<MdRun>>> = self.collectRuns(&c, &xr);
                let mut ni: i64 = 0;
                let nn: i64 = nested.len() as i64;
                while ni < nn {
                    out.push(nested[ni as usize].clone());
                    ni += 1;
                }
            }
            if kind == "softbreak" {
                let mut sb: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                sb.borrow_mut().text = " ".to_string();
                sb.borrow_mut().srcStart = c.borrow().srcStart;
                sb.borrow_mut().srcEnd = c.borrow().srcEnd;
                out.push(sb.clone());
            }
            if kind == "linebreak" {
                let mut lb: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                lb.borrow_mut().hardBreak = true;
                lb.borrow_mut().srcStart = c.borrow().srcStart;
                lb.borrow_mut().srcEnd = c.borrow().srcEnd;
                out.push(lb.clone());
            }
            if kind == "code" {
                let mut cr: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                cr.borrow_mut().family = self.style.borrow().monoFamily.clone();
                cr.borrow_mut().size = cur.borrow().size * 0.92;
                cr.borrow_mut().code = true;
                cr.borrow_mut().bold = false;
                cr.borrow_mut().italic = false;
                cr.borrow_mut().text = c.borrow().literal.clone();
                cr.borrow_mut().srcStart = c.borrow().srcStart;
                cr.borrow_mut().srcEnd = c.borrow().srcEnd;
                // the node spans its backticks; the run is the words between
                // them, so a character's offset is that character's. The
                // backticks and the one space each side a span may strip
                // come in equal numbers on both sides.
                let around: i64 = (c.borrow().srcEnd - c.borrow().srcStart) - (MdText::str_len(&c.borrow().literal) as i64);
                if c.borrow().srcStart >= 0 && (around >= 2 && around % 2 == 0) {
                    cr.borrow_mut().srcStart = c.borrow().srcStart + (around / 2);
                    let srcEnd_v = cr.borrow().srcStart + (MdText::str_len(&c.borrow().literal) as i64);
                    cr.borrow_mut().srcEnd = srcEnd_v;
                }
                out.push(cr.clone());
            }
            if kind == "math" {
                let mut mr: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                mr.borrow_mut().text = char::from_u32(65532 as u32).unwrap_or('\0').to_string();
                mr.borrow_mut().math = c.borrow().literal.clone();
                mr.borrow_mut().mathDisplay = c.borrow().info == "display";
                mr.borrow_mut().srcStart = c.borrow().srcStart;
                mr.borrow_mut().srcEnd = c.borrow().srcEnd;
                // its source is `$…$`, not the one character it is drawn as
                mr.borrow_mut().atom = true;
                out.push(mr.clone());
            }
            if kind == "html_inline" {
                // a tag switches a style on or off for what follows it; a
                // comment or a tag nobody draws is not shown
                let mut tg: Rc<RefCell<MdHtmlTag>> = MdHtml::tag(&c.borrow().literal.clone());
                let nm: String = tg.borrow().name.clone();
                if nm == "br" || nm == "wbr" {
                    if nm == "br" {
                        let mut brk: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                        brk.borrow_mut().hardBreak = true;
                        brk.borrow_mut().srcStart = c.borrow().srcStart;
                        brk.borrow_mut().srcEnd = c.borrow().srcEnd;
                        out.push(brk.clone());
                    }
                } else {
                    if tg.borrow().close {
                        if !outer.is_empty() {
                            cur = outer[((outer.len() as i64) - 1) as usize].clone();
                            outer.remove(((outer.len() as i64) - 1) as usize);
                        }
                        if nm == "q" {
                            let mut qc: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                            qc.borrow_mut().text = char::from_u32(8221 as u32).unwrap_or('\0').to_string();
                            out.push(qc.clone());
                        }
                    } else {
                        if MdLayout::isVoidTag(&nm) {
                            // nothing to close: it styles nothing after it
                            if nm == "img" {
                                self.htmlNote("an <img> inside a sentence is not drawn: put it on a line of its own");
                            } else {
                                self.htmlNote(&format!("<{}> is not drawn", nm));
                            }
                        }
                        if (!nm.is_empty() && !tg.borrow().selfClose) && !MdLayout::isVoidTag(&nm) {
                            outer.push(cur.clone());
                            cur = self.htmlStyle(&cur, &tg);
                            // a list in the middle of text (a table cell's
                            // `Points: <ul><li>…`): an item a line
                            if nm == "li" {
                                if !out.is_empty() {
                                    let mut lb_1: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                                    lb_1.borrow_mut().hardBreak = true;
                                    out.push(lb_1.clone());
                                }
                                let mut bu: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                                bu.borrow_mut().text = format!("{} ", char::from_u32(8226 as u32).unwrap_or('\0'));
                                out.push(bu.clone());
                            }
                            if nm == "q" {
                                let mut qo: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                                qo.borrow_mut().text = char::from_u32(8220 as u32).unwrap_or('\0').to_string();
                                out.push(qo.clone());
                            }
                        }
                    }
                }
            }
            if kind == "emph" {
                let mut er: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                er.borrow_mut().italic = true;
                let mut nested_1: Vec<Rc<RefCell<MdRun>>> = self.collectRuns(&c, &er);
                let mut ni_1: i64 = 0;
                let nn_1: i64 = nested_1.len() as i64;
                while ni_1 < nn_1 {
                    out.push(nested_1[ni_1 as usize].clone());
                    ni_1 += 1;
                }
            }
            if kind == "strong" {
                let mut sr: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                sr.borrow_mut().bold = true;
                let mut nested_2: Vec<Rc<RefCell<MdRun>>> = self.collectRuns(&c, &sr);
                let mut ni_2: i64 = 0;
                let nn_2: i64 = nested_2.len() as i64;
                while ni_2 < nn_2 {
                    out.push(nested_2[ni_2 as usize].clone());
                    ni_2 += 1;
                }
            }
            if kind == "strike" {
                let mut kr: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                kr.borrow_mut().color = self.style.borrow().mutedColor.clone();
                kr.borrow_mut().strike = true;
                let mut nested_3: Vec<Rc<RefCell<MdRun>>> = self.collectRuns(&c, &kr);
                let mut ni_3: i64 = 0;
                let nn_3: i64 = nested_3.len() as i64;
                while ni_3 < nn_3 {
                    out.push(nested_3[ni_3 as usize].clone());
                    ni_3 += 1;
                }
            }
            if kind == "link" {
                let mut lr: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                lr.borrow_mut().color = self.style.borrow().linkColor.clone();
                let _tmp_2907 = MdUrl::safeLink(&c.borrow().destination.clone());
                lr.borrow_mut().link = _tmp_2907;
                if lr.borrow().link.is_empty() {
                    self.htmlNote("a link to a script address (javascript:, vbscript:, data:) keeps its text but not its address");
                }
                let mut nested_4: Vec<Rc<RefCell<MdRun>>> = self.collectRuns(&c, &lr);
                let mut ni_4: i64 = 0;
                let nn_4: i64 = nested_4.len() as i64;
                while ni_4 < nn_4 {
                    out.push(nested_4[ni_4 as usize].clone());
                    ni_4 += 1;
                }
            }
            if kind == "footnote_ref" {
                // its number, raised, as a printed page sets it
                let mut fr: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                fr.borrow_mut().text = c.borrow().noteNumber.to_string();
                fr.borrow_mut().shiftOf = cur.borrow().size;
                if cur.borrow().shift != 0 {
                    fr.borrow_mut().shiftOf = cur.borrow().shiftOf;
                }
                fr.borrow_mut().size = cur.borrow().size * 0.75;
                fr.borrow_mut().shift = 1;
                fr.borrow_mut().color = self.style.borrow().linkColor.clone();
                fr.borrow_mut().noteRef = c.borrow().noteNumber;
                fr.borrow_mut().srcStart = c.borrow().srcStart;
                fr.borrow_mut().srcEnd = c.borrow().srcEnd;
                // the number is not its source `[^x]`: one stop on each side
                fr.borrow_mut().atom = true;
                out.push(fr.clone());
            }
            if kind == "image" {
                // An image the layout has no bytes for still says what it was.
                let mut ir: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
                ir.borrow_mut().italic = true;
                ir.borrow_mut().color = self.style.borrow().mutedColor.clone();
                ir.borrow_mut().quiet = true;
                let alt: String = c.borrow().plainText();
                ir.borrow_mut().text = format!("[{}]", alt);
                ir.borrow_mut().srcStart = c.borrow().srcStart;
                ir.borrow_mut().srcEnd = c.borrow().srcEnd;
                // "[alt]" stands for the whole `![alt](url)`, not its first letters
                ir.borrow_mut().atom = true;
                out.push(ir.clone());
            }
            i += 1;
        }
        out
    }
    /// ----------------------------------------------------------------------
    /// HTML blocks
    /// ----------------------------------------------------------------------
    ///
    ///     <div style="text-align: center">       <p align="right">…</p>
    ///     Centered text                          <center>…</center>
    ///     </div>                                 <h2 align="center">…</h2>
    ///
    /// A wrapper's content is read again as markdown and laid out in its
    /// place, aligned as the wrapper says. A wrapper whose content runs past a
    /// blank line arrives as two blocks — the opening tag, then markdown, then
    /// the closing tag — so its alignment is kept on a stack until the close.
    /// A <table> is drawn as a table, spans and all. Anything else is shown as
    /// markup.
    pub fn htmlBlock(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) -> bool {
        let lit: String = node.borrow().literal.clone();
        let n: i64 = MdText::str_len(&lit) as i64;
        let lead: i64 = MdInline::leadTrim(&lit);
        if lead >= n {
            return true;
        }
        if MdText::char_at(&lit, lead) != 60 {
            return false;
        }
        if lead + 3 < n && MdText::substr(&lit, lead, lead + 4) == "<!--" {
            // a comment is not shown; what follows it on its lines is
            let ce: i64 = MdText::index_of(&lit, "-->");
            if ce > 0 && !(MdText::substr(&lit, ce + 3, n).trim().to_string()).is_empty() {
                self.innerMarkdown(node, &MdText::substr(&lit, ce + 3, n), ce + 3, x, w);
            }
            return true;
        }
        let mut tg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&lit, lead);
        let nm: String = tg.borrow().name.clone();
        if nm.is_empty() {
            return false;
        }
        if tg.borrow().close {
            if MdLayout::isWrapper(&nm) {
                if !self.alignStack.is_empty() {
                    self.align = self.alignStack[((self.alignStack.len() as i64) - 1) as usize].clone();
                    self.alignStack.remove(((self.alignStack.len() as i64) - 1) as usize);
                }
                let rest0: String = MdText::substr(&lit, tg.borrow().end, n);
                if !rest0.trim().to_string().is_empty() {
                    self.innerMarkdown(node, &rest0, tg.borrow().end.clone(), x, w);
                }
                return true;
            }
            return false;
        }
        if nm == "table" {
            self.htmlTable(node, &lit, x, w);
            return true;
        }
        if nm == "hr" {
            let y: f64 = self.reserve(14.0);
            let arg_0 = MdBox::rect(
                x,
                y + 6.0,
                w,
                1.0,
                self.style.borrow().ruleColor.clone()
            ).clone();
            self.emit(arg_0);
            self.cursorY = y + 14.0;
            return true;
        }
        if nm == "br" {
            self.cursorY += self.style.borrow()
                .bodySize * self.style.borrow()
                .lineHeight;
            return true;
        }
        self.noteHandlers(&tg);
        if nm == "svg" {
            return self.svgPicture(node, x, w);
        }
        if nm == "ul" || nm == "ol" {
            // said again as a Markdown list, and laid out as one
            let mut hl: Rc<RefCell<MdHtmlList>> = MdHtmlMd::listItems(&lit, lead);
            let mut ldoc: Rc<RefCell<MdNode>> = MdBlock::parseText(&MdHtmlMd::listMarkdown(&hl));
            // its text is not the source's, so all of it stands for the
            // list's markup: a click on an item finds the <ul>
            let mut _tmp_2922 = hl.borrow().end.clone();
            MdLayout::pinSrc(
                &ldoc,
                node.borrow().srcOfLiteral(lead),
                node.borrow().srcOfLiteral(_tmp_2922)
            );
            self.blocks(&ldoc, x, w);
            let tail: String = MdText::substr(&lit, hl.borrow().end, n);
            if !tail.trim().to_string().is_empty() {
                self.innerMarkdown(node, &tail, hl.borrow().end.clone(), x, w);
            }
            return true;
        }
        if nm == "img" {
            let src: String = tg.borrow().attr("src");
            if src.is_empty() {
                return true;
            }
            // the address in <…>, so a space or a bracket in it holds
            let mut md: String = format!("![{}](<{}>)", tg.borrow().attr("alt"), src);
            let mut size: String = String::new();
            let wd: String = tg.borrow().attr("width").trim().to_string();
            if !wd.is_empty() {
                size = format!("width={}", wd);
            }
            let ht: String = tg.borrow().attr("height").trim().to_string();
            if !ht.is_empty() {
                if !size.is_empty() {
                    size = format!("{} ", size);
                }
                size = format!("{}height={}", size, ht);
            }
            if !size.is_empty() {
                md = format!("{}{}{}{}", md, "\n{".to_string(), size, "}".to_string());
            }
            self.innerMarkdown(node, &md, lead, x, w);
            return true;
        }
        if MdLayout::isMediaTag(&nm) {
            self.htmlNote(&format!("<{}> is not supported: a slide shows text and pictures, not video, sound or other pages; it is shown as its markup", nm));
            return false;
        }
        if !MdLayout::isWrapper(&nm) {
            if nm == "script" || nm == "style" {
                self.htmlNote(&format!("<{}> is shown as text and never run; a deck's styles go in its theme CSS", nm));
            } else {
                self.htmlNote(&format!("<{}> is not drawn as HTML: it is shown as its markup", nm));
            }
            return false;
        }
        let mut al: String = tg.borrow().align();
        if nm == "center" {
            al = "center".to_string();
        }
        let close_at: i64 = MdHtml::closingOf(&lit, &nm, tg.borrow().end.clone());
        if close_at < 0 {
            // the content runs on past this block: the alignment holds until
            // the closing tag's own block
            self.alignStack.push(self.align.clone());
            if !al.is_empty() {
                self.align = al.clone();
            }
            let after0: String = MdText::substr(&lit, tg.borrow().end, n);
            if !after0.trim().to_string().is_empty() {
                self.innerMarkdown(node, &after0, tg.borrow().end.clone(), x, w);
            }
            return true;
        }
        let mut inner: String = MdText::substr(&lit, tg.borrow().end, close_at);
        let mut close_tag: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&lit, close_at);
        let level: i64 = MdLayout::headingLevelOf(&nm);
        if level > 0 {
            let mut hashes: String = String::new();
            let mut k: i64 = 0;
            while k < level {
                hashes = format!("{}#", hashes);
                k += 1;
            }
            inner = format!("{} {}", hashes, MdLayout::oneLine(&inner));
        }
        if nm == "summary" {
            inner = format!("**{}**", MdLayout::oneLine(&inner).trim().to_string());
        }
        let save: String = self.align.clone();
        if !al.is_empty() {
            self.align = al.clone();
        }
        let mut wst: Rc<RefCell<MdHtmlStyle>> = MdHtmlStyle::read(&tg.borrow().attr("style"), "color background background-color padding border-radius text-align");
        let __it_wi = wst.borrow().ignored.clone();
        for wi in __it_wi.iter().cloned() {
            self.htmlNote(&format!("style {} on <{}> is not drawn: a block's style draws color, background-color, padding, border-radius and text-align", wi, nm));
        }
        if !wst.borrow().fill.is_empty() {
            // a background is a plate: `{container=box}` with its colour,
            // padding and rounding
            self.htmlPlate(node, &inner, tg.borrow().end.clone(), &wst, x, w);
        } else {
            let saved_ink: String = self.style.borrow().textColor.clone();
            if !wst.borrow().color.is_empty() {
                let mut ic: Rc<RefCell<EVGColor>> = CssValue::color(&wst.borrow().color.clone());
                if ic.borrow().is_set {
                    self.style.borrow_mut().textColor = MdCss::hexOf(&ic);
                }
            }
            self.innerMarkdown(node, &inner, tg.borrow().end.clone(), x, w);
            self.style.borrow_mut().textColor = saved_ink.clone();
        }
        self.align = save.clone();
        let after: String = MdText::substr(&lit, close_tag.borrow().end, n);
        if !after.trim().to_string().is_empty() {
            self.innerMarkdown(node, &after, close_tag.borrow().end.clone(), x, w);
        }
        true
    }
    /// `<div style="background:#123;padding:20px;border-radius:12px">`: its
    /// content on a plate, as `{container=box background=#123 padding=20px
    /// radius=12px}` under a block draws it
    pub fn htmlPlate(&mut self, outer: &Rc<RefCell<MdNode>>, text: &str, base: i64, st: &Rc<RefCell<MdHtmlStyle>>, x: f64, w: f64) {
        let mut doc: Rc<RefCell<MdNode>> = MdBlock::parseText(&MdLayout::dedent(text));
        MdLayout::remapSrc(&doc, outer, base);
        let mut r#box: Rc<RefCell<MdNode>> = MdNode::create("container".to_string());
        r#box.borrow_mut().srcStart = outer.borrow().srcStart;
        r#box.borrow_mut().srcEnd = outer.borrow().srcEnd;
        // the plate is part of the block its text is (the report's elements)
        if doc.borrow().childCount() > 0 {
            let mut first: Rc<RefCell<MdNode>> = doc.borrow().childAt(0);
            r#box.borrow_mut().srcStart = first.borrow().srcStart;
        }
        r#box.borrow_mut().setAttr("container", "box");
        r#box.borrow_mut().setAttr("background", &st.borrow().fill.clone());
        if !st.borrow().padding.is_empty() {
            r#box.borrow_mut().setAttr("padding", &st.borrow().padding.clone());
        }
        if !st.borrow().radius.is_empty() {
            r#box.borrow_mut().setAttr("radius", &st.borrow().radius.clone());
        }
        let mut i: i64 = 0;
        while i < doc.borrow().childCount() {
            r#box.borrow_mut().add(MdNode::childAt(&doc, i));
            i += 1;
        }
        let saved_ink: String = self.style.borrow().textColor.clone();
        if !st.borrow().color.is_empty() {
            let mut ic: Rc<RefCell<EVGColor>> = CssValue::color(&st.borrow().color.clone());
            if ic.borrow().is_set {
                self.style.borrow_mut().textColor = MdCss::hexOf(&ic);
            }
        }
        self.block(r#box.clone(), x, w);
        self.style.borrow_mut().textColor = saved_ink.clone();
    }
    /// A block that is one <svg> element: drawn as the picture the host made
    /// of it (`MdSvg`). Before the host has it, or with no host, nothing is
    /// drawn and it is noted.
    pub fn svgPicture(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) -> bool {
        let svg: String = MdSvg::ofBlock(node);
        if svg.is_empty() {
            self.htmlNote("an <svg> is drawn as a picture when the block is the <svg> element alone; this one is shown as its markup");
            return false;
        }
        let mut para: Rc<RefCell<MdNode>> = MdNode::create("paragraph".to_string());
        para.borrow_mut().srcStart = node.borrow().srcStart;
        para.borrow_mut().srcEnd = node.borrow().srcEnd;
        para.borrow_mut().takeAttrsFrom(node);
        // `<svg width="200">` is shown that wide, as `<img width>` is
        if !para.borrow().hasAttr("width") {
            let mut root: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&svg, 0);
            let mut sw: String = root.borrow().attr("width").trim().to_string();
            if (MdText::str_len(&sw) as i64) > 2 {
                if MdText::substr(&sw, (MdText::str_len(&sw) as i64) - 2, (MdText::str_len(&sw) as i64)) == "px" {
                    sw = MdText::substr(&sw, 0, (MdText::str_len(&sw) as i64) - 2);
                }
            }
            if !sw.is_empty() && ((sw.parse::<f64>().ok()).is_some()) {
                para.borrow_mut().setAttr("width", &sw);
            }
        }
        let mut img: Rc<RefCell<MdNode>> = MdNode::create("image".to_string());
        img.borrow_mut().destination = MdSvg::pathOf(&svg);
        img.borrow_mut().srcStart = node.borrow().srcStart;
        img.borrow_mut().srcEnd = node.borrow().srcEnd;
        para.borrow_mut().add(img.clone());
        if !self.pictureBlock(&para, x, w) {
            self.htmlNote("an inline <svg> was not drawn: the picture made of it is not there yet");
        }
        true
    }
    /// Markdown found inside an HTML block, laid out where the block is. Its
    /// nodes' source spans are moved back into the file through the block's
    /// own map, so a click on it still finds the text. Every line is taken
    /// off its indent first: HTML is indented for reading, and four spaces of
    /// it would make an indented code block of the content.
    pub fn innerMarkdown(&mut self, outer: &Rc<RefCell<MdNode>>, text: &str, base: i64, x: f64, w: f64) {
        let src: String = MdLayout::dedent(text);
        let mut doc: Rc<RefCell<MdNode>> = MdBlock::parseText(&src);
        MdLayout::remapSrc(&doc, outer, base);
        self.blocks(&doc, x, w);
    }
    /// ----------------------------------------------------------------------
    /// An HTML table
    /// ----------------------------------------------------------------------
    ///
    /// The markdown table's look — padding, border, header tint — with what
    /// only HTML says: cells that span rows and columns, header rows from
    /// <thead> or <th>, alignment per cell. Column widths are the markdown
    /// table's algorithm (min- and max-content, the room shared out), with a
    /// spanning cell's widths shared over the columns it covers. A cell is
    /// centred vertically in the rows it spans, as a browser does.
    pub fn cellRuns(&mut self, outer: &Rc<RefCell<MdNode>>, cell: &Rc<RefCell<MdHtmlCell>>) -> Vec<Rc<RefCell<MdRun>>> {
        let mut base: Rc<RefCell<MdRun>> = self.baseRun();
        if cell.borrow().header {
            base.borrow_mut().bold = true;
        }
        self.cellStyle(outer, cell, &base);
        let mut out: Vec<Rc<RefCell<MdRun>>> = Vec::new();
        let src: String = MdLayout::dedent(&cell.borrow().inner.clone()).trim().to_string();
        if src.is_empty() {
            return out;
        }
        let lead: i64 = MdInline::leadTrim(&cell.borrow().inner.clone());
        let mut doc: Rc<RefCell<MdNode>> = MdBlock::parseText(&src);
        MdLayout::remapSrc(&doc, outer, cell.borrow().start + lead);
        let mut i: i64 = 0;
        while i < doc.borrow().childCount() {
            let mut b: Rc<RefCell<MdNode>> = doc.borrow().childAt(i);
            if i > 0 {
                let mut brk: Rc<RefCell<MdRun>> = base.borrow().copyStyle();
                brk.borrow_mut().hardBreak = true;
                out.push(brk.clone());
            }
            let mut runs: Vec<Rc<RefCell<MdRun>>> = self.inlineRuns(&b, &base);
            if b.borrow().kind == "html_block" {
                let lit: String = b.borrow().literal.clone();
                let bl: i64 = MdInline::leadTrim(&lit);
                let mut btg: Rc<RefCell<MdHtmlTag>> = MdHtml::tagAt(&lit, bl);
                if btg.borrow().name == "ul" || btg.borrow().name == "ol" {
                    // a list in a cell: an item a line, with its marker
                    runs = self.cellListRuns(&MdHtmlMd::listItems(&lit, bl), &base);
                } else {
                    if btg.borrow().name == "table" {
                        runs = self.cellTableRuns(&lit, &base);
                    } else {
                        // a lone tag in a cell (<br>, <b>…): its text, plainly
                        let mut tr: Rc<RefCell<MdRun>> = base.borrow().copyStyle();
                        let _tmp_2947 = MdLayout::stripTags(&b.borrow().literal.clone());
                        tr.borrow_mut().text = _tmp_2947;
                        runs.push(tr.clone());
                    }
                }
            }
            let mut k: i64 = 0;
            while k < (runs.len() as i64) {
                out.push(runs[k as usize].clone());
                k += 1;
            }
            i += 1;
        }
        out
    }
    /// What a cell's `style=""` / `bgcolor=` and the table's cell tones
    /// (`{cells=…}`, `table { cell-tones }`) make of its text, and the colour
    /// it is filled with (`cell.fill`, drawn by `htmlGroup`).
    pub fn cellStyle(&mut self, outer: &Rc<RefCell<MdNode>>, cell: &Rc<RefCell<MdHtmlCell>>, base: &Rc<RefCell<MdRun>>) {
        let mut tones: Rc<RefCell<MdCellTones>> = MdCellTones::forTable(outer, &self.style.borrow().tableCellTones.clone());
        let tone: String = tones.borrow().toneOf(&MdLayout::stripTags(&cell.borrow().inner.clone()));
        if !tone.is_empty() {
            base.borrow_mut().bold = true;
            let _tmp_2951 = MdCellTones::inkOf(&tone, &self.tablePaper());
            base.borrow_mut().color = _tmp_2951;
            let _tmp_2953 = MdCellTones::fillOf(&tone, &self.tablePaper());
            cell.borrow_mut().fill = _tmp_2953;
        }
        let mut st: Rc<RefCell<MdHtmlStyle>> = MdHtmlStyle::read(&cell.borrow().style.clone(), "color background background-color font-weight font-style text-align vertical-align");
        let mut fill: String = st.borrow().fill.clone();
        if fill.is_empty() {
            fill = cell.borrow().bgcolor.clone();
        }
        if !fill.is_empty() {
            let mut fc: Rc<RefCell<EVGColor>> = CssValue::color(&fill);
            if fc.borrow().is_set {
                cell.borrow_mut().fill = MdCss::hexOf(&fc);
            }
        }
        if !st.borrow().color.is_empty() {
            let mut ic: Rc<RefCell<EVGColor>> = CssValue::color(&st.borrow().color.clone());
            if ic.borrow().is_set {
                base.borrow_mut().color = MdCss::hexOf(&ic);
            }
        }
        if st.borrow().weight == "bold" {
            base.borrow_mut().bold = true;
        }
        if st.borrow().weight == "normal" {
            base.borrow_mut().bold = false;
        }
        if st.borrow().slant == "italic" {
            base.borrow_mut().italic = true;
        }
        let __it_ig = st.borrow().ignored.clone();
        for ig in __it_ig.iter().cloned() {
            self.htmlNote(&format!("style {} on a table cell is not drawn: a cell's style draws color, background-color, font-weight, font-style and text-align", ig));
        }
    }
    pub fn cellListRuns(&mut self, list: &Rc<RefCell<MdHtmlList>>, base: &Rc<RefCell<MdRun>>) -> Vec<Rc<RefCell<MdRun>>> {
        let mut out: Vec<Rc<RefCell<MdRun>>> = Vec::new();
        let __n_k = list.borrow().items.len() as i64;
        for k in 0..__n_k {
            let mut it = list.borrow().items[k as usize].clone();
            if k > 0 {
                let mut brk: Rc<RefCell<MdRun>> = base.borrow().copyStyle();
                brk.borrow_mut().hardBreak = true;
                out.push(brk.clone());
            }
            let mut mark: Rc<RefCell<MdRun>> = base.borrow().copyStyle();
            let mut ind: String = String::new();
            let mut d: i64 = 0;
            while d < it.borrow().depth {
                ind = format!("{}    ", ind);
                d += 1;
            }
            if it.borrow().marker == "-" {
                mark.borrow_mut().text = format!("{}{} ", ind, char::from_u32(8226 as u32).unwrap_or('\0'));
            } else {
                mark.borrow_mut().text = format!("{}{} ", ind, it.borrow().marker);
            }
            out.push(mark.clone());
            let mut idoc: Rc<RefCell<MdNode>> = MdBlock::parseText(&it.borrow().text.clone());
            if idoc.borrow().childCount() > 0 {
                let __n_ik = self.inlineRuns(&idoc.borrow().childAt(0),
                    base
                ).len() as i64;
                for ik in 0..__n_ik {
                    let mut ir = self.inlineRuns(&idoc.borrow().childAt(0),
                        base
                    )[ik as usize].clone();
                    out.push(ir.clone());
                }
            }
        }
        out
    }
    /// A table inside a cell: a line a row, its cells' text between dots.
    pub fn cellTableRuns(&mut self, lit: &str, base: &Rc<RefCell<MdRun>>) -> Vec<Rc<RefCell<MdRun>>> {
        self.htmlNote("a table inside a table cell is drawn as lines of text, one a row");
        let mut out: Vec<Rc<RefCell<MdRun>>> = Vec::new();
        let mut t: Rc<RefCell<MdHtmlTable>> = MdHtml::table(lit);
        let __n_r = t.borrow().rows.len() as i64;
        for r in 0..__n_r {
            let mut row = t.borrow().rows[r as usize].clone();
            if r > 0 {
                let mut brk: Rc<RefCell<MdRun>> = base.borrow().copyStyle();
                brk.borrow_mut().hardBreak = true;
                out.push(brk.clone());
            }
            let mut parts: Vec<String> = Vec::new();
            let __it_c = row.borrow().cells.clone();
            for c in __it_c.iter().cloned() {
                parts.push(MdLayout::stripTags(&c.borrow().inner.clone()));
            }
            let mut tr: Rc<RefCell<MdRun>> = base.borrow().copyStyle();
            tr.borrow_mut().text = parts.join(&*"  ·  ".to_string());
            out.push(tr.clone());
        }
        out
    }
    pub fn linesHeight(&self, lines: &Vec<Rc<RefCell<MdLine>>>) -> f64 {
        let mut sum: f64 = 0.0;
        let mut i: i64 = 0;
        while i < (lines.len() as i64) {
            let mut ln: Rc<RefCell<MdLine>> = lines[i as usize].clone();
            sum += ln.borrow().height;
            i += 1;
        }
        sum
    }
    pub fn htmlTable(&mut self, outer: &Rc<RefCell<MdNode>>, lit: &str, x: f64, w: f64) {
        let mut t: Rc<RefCell<MdHtmlTable>> = MdHtml::table(lit);
        let rows: i64 = t.borrow().rowCount();
        let cols: i64 = t.borrow().cols;
        if rows == 0 || cols == 0 {
            return;
        }
        if !t.borrow().caption.is_empty() {
            let save: String = self.align.clone();
            self.align = "center".to_string();
            self.innerMarkdown(outer, &format!("*{}*", MdLayout::oneLine(&t.borrow().caption)).clone(), 0, x, w);
            self.align = save.clone();
            self.cursorY -= self.style.borrow().paraSpacing * 0.5;
        }
        let pad_x: f64 = self.style.borrow().cellPadX;
        let pad_y: f64 = self.style.borrow().cellPadY;
        // every cell, its runs, and its span clamped to the table
        let mut cells: Vec<Rc<RefCell<MdHtmlCell>>> = Vec::new();
        let mut cell_runs: Vec<Vec<Rc<RefCell<MdRun>>>> = Vec::new();
        let mut r: i64 = 0;
        while r < rows {
            let mut row: Rc<RefCell<MdHtmlRow>> = t.borrow().rows[r as usize].clone();
            let mut k: i64 = 0;
            while k < (row.borrow().cells.len() as i64) {
                let mut c: Rc<RefCell<MdHtmlCell>> = row.borrow().cells[k as usize].clone();
                if row.borrow().head {
                    c.borrow_mut().header = true;
                }
                if c.borrow().row + c.borrow().rowspan > rows {
                    let rowspan_v = rows - c.borrow().row;
                    c.borrow_mut().rowspan = rowspan_v;
                }
                cells.push(c.clone());
                cell_runs.push(self.cellRuns(outer, &c));
                k += 1;
            }
            r += 1;
        }
        // column widths: single-column cells first, then spans spread over
        // the columns they cover
        let mut natural: Vec<f64> = Vec::new();
        let mut minimum: Vec<f64> = Vec::new();
        let mut c0: i64 = 0;
        while c0 < cols {
            natural.push(pad_x * 2.0);
            minimum.push(pad_x * 2.0);
            c0 += 1;
        }
        let mut pass: i64 = 0;
        while pass < 2 {
            let mut i: i64 = 0;
            while i < (cells.len() as i64) {
                let mut cell: Rc<RefCell<MdHtmlCell>> = cells[i as usize].clone();
                let mut runs: Vec<Rc<RefCell<MdRun>>> = cell_runs[i as usize].clone();
                let single: bool = cell.borrow().colspan == 1;
                if single == (pass == 0) {
                    let nat: f64 = self.runsWidth(&runs) + pad_x * 2.0;
                    let mut least: f64 = 0.0;
                    let mut tight: Vec<Rc<RefCell<MdLine>>> = self.breakRuns(&runs, 1.0);
                    let mut ti: i64 = 0;
                    while ti < (tight.len() as i64) {
                        let mut tl: Rc<RefCell<MdLine>> = tight[ti as usize].clone();
                        if tl.borrow().width > least {
                            least = tl.borrow().width;
                        }
                        ti += 1;
                    }
                    least += pad_x * 2.0;
                    let mut have_nat: f64 = 0.0;
                    let mut have_min: f64 = 0.0;
                    let mut cc: i64 = cell.borrow().col;
                    while cc < cell.borrow().col + cell.borrow().colspan && cc < cols {
                        have_nat += natural[cc as usize];
                        have_min += minimum[cc as usize];
                        cc += 1;
                    }
                    let span_n: f64 = cell.borrow().colspan as f64;
                    cc = cell.borrow().col;
                    while cc < cell.borrow().col + cell.borrow().colspan && cc < cols {
                        if nat > have_nat {
                            natural[cc as usize] = natural[cc as usize] + (nat - have_nat) / span_n;
                        }
                        if least > have_min {
                            minimum[cc as usize] = minimum[cc as usize] + (least - have_min) / span_n;
                        }
                        cc += 1;
                    }
                }
                i += 1;
            }
            pass += 1;
        }
        let mut widths: Vec<f64> = MdLayout::shareWidths(&natural, &minimum, w);
        let mut col_x: Vec<f64> = Vec::new();
        let mut cx: f64 = x;
        let mut c1: i64 = 0;
        while c1 < cols {
            col_x.push(cx);
            cx += widths[c1 as usize];
            c1 += 1;
        }
        // row heights: single-row cells, then a spanning cell's remainder on
        // the last row it covers
        let mut row_h: Vec<f64> = Vec::new();
        let mut r1: i64 = 0;
        while r1 < rows {
            row_h.push(self.style.borrow().bodySize * self.style.borrow().lineHeight + pad_y * 2.0);
            r1 += 1;
        }
        let mut cell_lines: Vec<Vec<Rc<RefCell<MdLine>>>> = Vec::new();
        let mut cell_w: Vec<f64> = Vec::new();
        let mut j: i64 = 0;
        while j < (cells.len() as i64) {
            let mut cell2: Rc<RefCell<MdHtmlCell>> = cells[j as usize].clone();
            let mut cw: f64 = 0.0;
            let mut cc2: i64 = cell2.borrow().col;
            while cc2 < cell2.borrow().col + cell2.borrow().colspan && cc2 < cols {
                cw += widths[cc2 as usize];
                cc2 += 1;
            }
            cell_w.push(cw);
            let mut lines: Vec<Rc<RefCell<MdLine>>> = self.breakRuns(&cell_runs[j as usize].clone(),
                cw - pad_x * 2.0
            );
            cell_lines.push(lines.clone());
            j += 1;
        }
        let mut span_pass: i64 = 0;
        while span_pass < 2 {
            let mut j2: i64 = 0;
            while j2 < (cells.len() as i64) {
                let mut cell3: Rc<RefCell<MdHtmlCell>> = cells[j2 as usize].clone();
                if (cell3.borrow().rowspan == 1) == (span_pass == 0) {
                    let need: f64 = self.linesHeight(&cell_lines[j2 as usize].clone()) + pad_y * 2.0;
                    let mut have: f64 = 0.0;
                    let mut rr: i64 = cell3.borrow().row;
                    while rr < cell3.borrow().row + cell3.borrow().rowspan {
                        have += row_h[rr as usize];
                        rr += 1;
                    }
                    if need > have {
                        let last: i64 = (cell3.borrow().row + cell3.borrow().rowspan) - 1;
                        row_h[last as usize] = row_h[last as usize] + (need - have);
                    }
                }
                j2 += 1;
            }
            span_pass += 1;
        }
        // drawn a group of rows at a time: rows a span ties together go onto
        // the same slide, and the header groups (`<thead>` rows) are drawn
        // again at the top of every slide the table continues on
        let mut g_from: Vec<i64> = Vec::new();
        let mut g_to: Vec<i64> = Vec::new();
        let mut g_h: Vec<f64> = Vec::new();
        let mut g0: i64 = 0;
        while g0 < rows {
            let mut g1: i64 = g0 + 1;
            let mut grow: bool = true;
            while grow {
                grow = false;
                let mut q: i64 = 0;
                while q < (cells.len() as i64) {
                    let mut cq: Rc<RefCell<MdHtmlCell>> = cells[q as usize].clone();
                    if (cq.borrow().row >= g0 && cq.borrow().row < g1) && cq.borrow()
                        .row + cq.borrow()
                        .rowspan > g1 {
                        g1 = cq.borrow().row + cq.borrow().rowspan;
                        grow = true;
                    }
                    q += 1;
                }
            }
            let mut gh: f64 = 0.0;
            let mut rq: i64 = g0;
            while rq < g1 {
                gh += row_h[rq as usize];
                rq += 1;
            }
            g_from.push(g0);
            g_to.push(g1);
            g_h.push(gh);
            g0 = g1;
        }
        let mut heads: i64 = 0;
        let mut head_h: f64 = 0.0;
        let mut more: bool = true;
        while more && heads < (g_from.len() as i64) {
            let mut all: bool = true;
            let mut hr: i64 = g_from[heads as usize];
            while hr < g_to[heads as usize] {
                // a `<thead>` row, or one of `<th>` cells only
                let mut hrow: Rc<RefCell<MdHtmlRow>> = t.borrow().rows[hr as usize].clone();
                if !hrow.borrow().head {
                    let mut hk: i64 = 0;
                    while hk < (hrow.borrow().cells.len() as i64) {
                        let mut hc: Rc<RefCell<MdHtmlCell>> = hrow.borrow().cells[hk as usize].clone();
                        if !hc.borrow().header {
                            all = false;
                        }
                        hk += 1;
                    }
                    if hrow.borrow().cells.is_empty() {
                        all = false;
                    }
                }
                hr += 1;
            }
            if all {
                head_h += g_h[heads as usize];
                heads += 1;
            } else {
                more = false;
            }
        }
        let mut body: Vec<f64> = Vec::new();
        let mut bi: i64 = heads;
        while bi < (g_h.len() as i64) {
            body.push(g_h[bi as usize]);
            bi += 1;
        }
        let mut plan: Rc<RefCell<MdTablePlan>> = self.tablePlan(&body, head_h);
        let mut on: i64 = -1;
        let mut gi: i64 = heads;
        while gi < (g_from.len() as i64) {
            let s: i64 = plan.borrow().slideOf[(gi - heads) as usize];
            if s != on {
                if on >= 0 || s > 0 {
                    self.tableBreak();
                }
                on = s;
                let mut hg: i64 = 0;
                while hg < heads {
                    self.htmlGroup(&cells,
                        &cell_lines,
                        &cell_w,
                        &col_x,
                        &row_h,
                        g_from[hg as usize],
                        g_to[hg as usize],
                        g_h[hg as usize],
                        pad_x
                    );
                    hg += 1;
                }
            }
            self.htmlGroup(&cells,
                &cell_lines,
                &cell_w,
                &col_x,
                &row_h,
                g_from[gi as usize],
                g_to[gi as usize],
                g_h[gi as usize],
                pad_x
            );
            gi += 1;
        }
        if heads == (g_from.len() as i64) {
            let mut hg2: i64 = 0;
            while hg2 < heads {
                self.htmlGroup(&cells,
                    &cell_lines,
                    &cell_w,
                    &col_x,
                    &row_h,
                    g_from[hg2 as usize],
                    g_to[hg2 as usize],
                    g_h[hg2 as usize],
                    pad_x
                );
                hg2 += 1;
            }
        }
        self.cursorY += self.style.borrow().paraSpacing;
    }
    /// Rows g0..g1 of an HTML table, `gh` tall, at the cursor.
    pub fn htmlGroup(&mut self, cells: &Vec<Rc<RefCell<MdHtmlCell>>>, cell_lines: &Vec<Vec<Rc<RefCell<MdLine>>>>, cell_w: &[f64], col_x: &[f64], row_h: &[f64], g0: i64, g1: i64, gh: f64, pad_x: f64) {
        let mut row_y: Vec<f64> = Vec::new();
        let mut acc: f64 = 0.0;
        let mut rq: i64 = g0;
        while rq < g1 {
            row_y.push(acc);
            acc += row_h[rq as usize];
            rq += 1;
        }
        let y0: f64 = self.reserve(gh);
        let mut m: i64 = 0;
        while m < (cells.len() as i64) {
            let mut cm: Rc<RefCell<MdHtmlCell>> = cells[m as usize].clone();
            if cm.borrow().row >= g0 && cm.borrow().row < g1 {
                let cy: f64 = y0 + row_y[(cm.borrow().row - g0) as usize];
                let mut ch: f64 = 0.0;
                let mut rs: i64 = cm.borrow().row;
                while rs < cm.borrow().row + cm.borrow().rowspan {
                    ch += row_h[rs as usize];
                    rs += 1;
                }
                let cxm: f64 = col_x[(cm.borrow().col) as usize];
                let cwm: f64 = cell_w[m as usize];
                if cm.borrow().header {
                    let arg_0 = MdBox::rect(
                        cxm,
                        cy,
                        cwm,
                        ch,
                        self.style.borrow().tableHeadBackground.clone()
                    ).clone();
                    self.emit(arg_0);
                }
                if !cm.borrow().fill.is_empty() {
                    self.emit(MdBox::rect(cxm, cy, cwm, ch, cm.borrow().fill.clone()).clone());
                }
                let arg_0 = MdBox::outline(
                    cxm,
                    cy,
                    cwm,
                    ch,
                    self.style.borrow().tableBorder.clone(),
                    1.0
                ).clone();
                self.emit(arg_0);
                let mut lines2: Vec<Rc<RefCell<MdLine>>> = cell_lines[m as usize].clone();
                let inner: f64 = cwm - pad_x * 2.0;
                let mut ty: f64 = cy + (ch - self.linesHeight(&lines2)) / 2.0;
                let mut li: i64 = 0;
                while li < (lines2.len() as i64) {
                    let mut ln: Rc<RefCell<MdLine>> = lines2[li as usize].clone();
                    let mut lx: f64 = cxm + pad_x;
                    if cm.borrow().align == "right" {
                        lx = ((cxm + cwm) - pad_x) - ln.borrow().width;
                    }
                    if cm.borrow().align == "center" {
                        lx = (cxm + pad_x) + (inner - ln.borrow().width) / 2.0;
                    }
                    self.emitLine(&ln, lx, ty);
                    ty += ln.borrow().height;
                    li += 1;
                }
            }
            m += 1;
        }
        self.cursorY = y0 + gh;
    }
    /// What an inline HTML tag makes of the style around it. Tags nobody draws
    /// (span, abbr's title, a link's target) leave the style as it was.
    /// `style=""` gives the text its colour, a colour behind it, its size,
    /// weight and slant and a line under or through it; what else it says,
    /// and a script handler (`onclick`), is noted, not drawn.
    pub fn htmlStyle(&mut self, cur: &Rc<RefCell<MdRun>>, tg: &Rc<RefCell<MdHtmlTag>>) -> Rc<RefCell<MdRun>> {
        let nm: String = tg.borrow().name.clone();
        let mut r: Rc<RefCell<MdRun>> = self.tagStyle(cur, &nm);
        if MdLayout::isMediaTag(&nm) {
            self.htmlNote(&format!("<{}> is not supported: a slide shows text and pictures, not video, sound or other pages", nm));
        }
        if !MdLayout::isInlineTag(&nm) && !MdLayout::isMediaTag(&nm) {
            self.htmlNote(&format!("<{}> is not drawn: Sliqtly draws the inline tags b, strong, i, em, u, s, del, ins, mark, code, kbd, sub, sup, small, q, abbr, a and span", nm));
        }
        self.noteHandlers(tg);
        if nm == "abbr" || nm == "acronym" {
            if tg.borrow().has("title") {
                r.borrow_mut().underline = true;
            }
        }
        if nm == "a" {
            let href: String = tg.borrow().attr("href");
            if !href.is_empty() {
                r.borrow_mut().link = MdUrl::safeLink(&href);
                if !r.borrow().link.is_empty() {
                    r.borrow_mut().color = self.style.borrow().linkColor.clone();
                } else {
                    self.htmlNote("a link to a script address (javascript:, vbscript:, data:) keeps its text but not its address");
                }
            }
        }
        let mut st: Rc<RefCell<MdHtmlStyle>> = MdHtmlStyle::read(&tg.borrow().attr("style"), "color background background-color font-size font-weight font-style text-decoration text-decoration-line");
        let mut col: String = st.borrow().color.clone();
        if col.is_empty() {
            if nm == "font" {
                col = tg.borrow().attr("color");
            }
        }
        if !col.is_empty() {
            r.borrow_mut().color = col.clone();
        }
        if !st.borrow().fill.is_empty() {
            let mut fc: Rc<RefCell<EVGColor>> = CssValue::color(&st.borrow().fill.clone());
            if fc.borrow().is_set {
                r.borrow_mut().fill = MdCss::hexOf(&fc);
            } else {
                st.borrow_mut().ignored.push("background-color".to_string().to_string());
            }
        }
        if !st.borrow().size.is_empty() {
            let sz: f64 = self.spanSize(cur, &st.borrow().size.clone());
            if sz > 0.0 {
                r.borrow_mut().size = sz;
            } else {
                st.borrow_mut().ignored.push("font-size".to_string().to_string());
            }
        }
        if st.borrow().weight == "bold" {
            r.borrow_mut().bold = true;
        }
        if st.borrow().weight == "normal" {
            r.borrow_mut().bold = false;
        }
        if st.borrow().slant == "italic" {
            r.borrow_mut().italic = true;
        }
        if st.borrow().slant == "normal" {
            r.borrow_mut().italic = false;
        }
        if st.borrow().underline != 0 {
            r.borrow_mut().underline = st.borrow().underline > 0;
        }
        if st.borrow().strike != 0 {
            r.borrow_mut().strike = st.borrow().strike > 0;
        }
        let __it_ig = st.borrow().ignored.clone();
        for ig in __it_ig.iter().cloned() {
            self.htmlNote(&format!("style {} on <{}> is not drawn: inline styles draw color, background-color, font-size, font-weight, font-style and text-decoration", ig, nm));
        }
        r
    }
    /// `onclick` and its kin: nothing on a slide runs script, so a handler is
    /// said to be left out rather than kept silently
    pub fn noteHandlers(&mut self, tg: &Rc<RefCell<MdHtmlTag>>) {
        let __n_i = tg.borrow().keys.len() as i64;
        for i in 0..__n_i {
            let mut k = tg.borrow().keys[i as usize].clone();
            if (MdText::str_len(&k) as i64) > 2 && MdText::index_of(&k, "on") == 0 {
                self.htmlNote(&format!("{} on <{}> is left out: nothing in a slide runs script", k, tg.borrow().name).clone());
            }
        }
    }
    /// A span's `font-size` in the layout's points: absolute sizes against the
    /// document's own (`font-size: 32pt` where the body is 20pt is 1.6 times
    /// the body), `em`, `%` and `larger`/`smaller` against the text around it,
    /// the CSS words (`large`, `x-large`…) against the body, all kept within
    /// the sizes a slide can show (`MdCssLimits`). -1 when it is not a size.
    pub fn spanSize(&self, cur: &Rc<RefCell<MdRun>>, value: &str) -> f64 {
        let t: String = MdChar::lower(&value.trim().to_string());
        let n: i64 = MdText::str_len(&t) as i64;
        let mut words: Vec<String> = vec!["xx-small".to_string(), "x-small".to_string(), "small".to_string(), "medium".to_string(), "large".to_string(), "x-large".to_string(), "xx-large".to_string(), "xxx-large".to_string()];
        let mut scales: Vec<f64> = vec![0.6, 0.75, 0.89, 1.0, 1.2, 1.5, 2.0, 3.0];
        let wi: i64 = words.iter()
            .position( |__r| __r.clone() == t.clone() )
            .map(|__i| __i as i64)
            .unwrap_or(-1);
        let mut r: f64 = -1.0;
        let mut against: f64 = self.style.borrow().bodySize;
        if wi >= 0 {
            r = scales[wi as usize];
        }
        if t == "larger" {
            r = 1.2;
            against = cur.borrow().size;
        }
        if t == "smaller" {
            r = 0.83;
            against = cur.borrow().size;
        }
        if r < 0.0 && (n > 3 && MdText::substr(&t, n - 3, n) == "rem") {
            r = if ((MdText::substr(&t, 0, n - 3).trim().to_string().parse::<f64>().ok()).is_some()) { ((MdText::substr(&t, 0, n - 3).trim().to_string().parse::<f64>().ok()).unwrap()) } else { -1.0 };
        }
        if r < 0.0 && (n > 2 && MdText::substr(&t, n - 2, n) == "em") {
            r = if ((MdText::substr(&t, 0, n - 2).trim().to_string().parse::<f64>().ok()).is_some()) { ((MdText::substr(&t, 0, n - 2).trim().to_string().parse::<f64>().ok()).unwrap()) } else { -1.0 };
            against = cur.borrow().size;
        }
        if r < 0.0 && (n > 1 && MdText::substr(&t, n - 1, n) == "%") {
            r = (if ((MdText::substr(&t, 0, n - 1).trim().to_string().parse::<f64>().ok()).is_some()) { ((MdText::substr(&t, 0, n - 1).trim().to_string().parse::<f64>().ok()).unwrap()) } else { -100.0 }) / 100.0;
            against = cur.borrow().size;
        }
        if r < 0.0 {
            // a length: against the size the document's sheet gives its body
            let mut base: f64 = self.style.borrow().bodySize;
            let mut b: Rc<RefCell<CssComputed>> = self.sheet.borrow_mut().forTag("document".to_string());
            if CssComputed::has(&b, "font-size") {
                base = CssValue::font_points(&CssComputed::get(&b, "font-size"), self.style.borrow().bodySize);
            }
            let pts: f64 = CssValue::font_points(&t, -1.0);
            if pts <= 0.0 || base <= 0.0 {
                return -1.0;
            }
            r = pts / base;
        }
        if r <= 0.0 {
            return -1.0;
        }
        let want: f64 = against * r;
        let mut _tmp_2970 = self.style.borrow().bodySize.clone();
        let mut _tmp_2971 = self.style.borrow().pageWidth.clone();
        let mut _tmp_2972 = self.style.borrow().pageHeight.clone();
        let ratio: f64 = MdCssLimits::ratio(
            want / self.style.borrow().bodySize,
            _tmp_2970,
            _tmp_2971,
            _tmp_2972
        );
        self.style.borrow().bodySize * ratio
    }
    /// Said once per layout, with where it was first met.
    pub fn htmlNote(&mut self, text: &str) {
        if (self.htmlNotes.iter().position( |__r| __r.clone() == text.clone() ).map(|__i| __i as i64).unwrap_or(-1)) >= 0 {
            return;
        }
        self.htmlNotes.push(text.to_string());
        self.htmlNoteSrc.push(self.currentSrc);
    }
    /// What an element of this name does to the style around it, whether it
    /// came as HTML (`<sup>`) or as markdown (`^…^`, which is the same `sup`).
    pub fn tagStyle(&self, cur: &Rc<RefCell<MdRun>>, nm: &str) -> Rc<RefCell<MdRun>> {
        let mut r: Rc<RefCell<MdRun>> = cur.borrow().copyStyle();
        if nm == "b" || nm == "strong" {
            r.borrow_mut().bold = true;
        }
        if ((nm == "i" || nm == "em") || (nm == "cite" || nm == "var")) || nm == "dfn" {
            r.borrow_mut().italic = true;
        }
        if nm == "u" || nm == "ins" {
            r.borrow_mut().underline = true;
        }
        if nm == "s" || (nm == "strike" || nm == "del") {
            r.borrow_mut().strike = true;
            r.borrow_mut().color = self.style.borrow().mutedColor.clone();
        }
        if nm == "mark" {
            r.borrow_mut().mark = true;
            r.borrow_mut().color = self.style.borrow().markColor.clone();
        }
        if nm == "code" || (nm == "tt" || nm == "samp") {
            r.borrow_mut().family = self.style.borrow().monoFamily.clone();
            let size_v = cur.borrow().size * 0.92;
            r.borrow_mut().size = size_v;
            r.borrow_mut().code = true;
        }
        if nm == "kbd" {
            r.borrow_mut().family = self.style.borrow().monoFamily.clone();
            let size_v = cur.borrow().size * 0.85;
            r.borrow_mut().size = size_v;
            r.borrow_mut().kbd = true;
        }
        if nm == "sub" || nm == "sup" {
            let shiftOf_v = cur.borrow().size;
            r.borrow_mut().shiftOf = shiftOf_v;
            if cur.borrow().shift != 0 {
                let shiftOf_v = cur.borrow().shiftOf;
                r.borrow_mut().shiftOf = shiftOf_v;
            }
            let size_v = cur.borrow().size * 0.75;
            r.borrow_mut().size = size_v;
            r.borrow_mut().shift = 1;
            if nm == "sub" {
                r.borrow_mut().shift = -1;
            }
        }
        if nm == "small" {
            let size_v = cur.borrow().size * 0.85;
            r.borrow_mut().size = size_v;
        }
        if nm == "big" {
            let size_v = cur.borrow().size * 1.15;
            r.borrow_mut().size = size_v;
        }
        r
    }
    /// One broken line, placed with its left edge at `x0` and its top at `y`:
    /// a text box per segment, and what goes behind and over them — a code
    /// tint, a highlight, a key cap, a line under or through.
    pub fn emitLine(&mut self, ln: &Rc<RefCell<MdLine>>, x0: f64, y: f64) {
        let top: f64 = y + ln.borrow().extraTop;
        let lh: f64 = ln.borrow().height - ln.borrow().extraTop;
        // The line's baseline: the lowest one its segments would have on
        // their own, which is the largest face's. Every segment stands on
        // it — <small> print, a key cap and a code span no longer float up
        // to the top of the line box they are smaller than.
        let mut base: f64 = 0.0;
        let mut s0: i64 = 0;
        while s0 < (ln.borrow().segs.len() as i64) {
            let mut g0: Rc<RefCell<MdSeg>> = ln.borrow().segs[s0 as usize].clone();
            if g0.borrow().math.is_empty() && g0.borrow().shift == 0 {
                let b0: f64 = self.baselineIn(&g0);
                if b0 > base {
                    base = b0;
                }
            }
            s0 += 1;
        }
        let mut s: i64 = 0;
        let sn: i64 = ln.borrow().segs.len() as i64;
        while s < sn {
            let mut seg: Rc<RefCell<MdSeg>> = ln.borrow().segs[s as usize].clone();
            let sx: f64 = x0 + seg.borrow().x;
            let own: f64 = self.baselineIn(&seg);
            if !seg.borrow().math.is_empty() {
                let mut mt: f64 = top;
                if base > 0.0 {
                    mt = top + (base - own);
                }
                self.emit(self.mathSegBox(&seg, sx, mt));
                s += 1;
                continue;
            }
            // on the line's baseline, or — a sub- or superscript — moved off
            // it by a share of the size of the text it sits in
            let mut ty: f64 = top;
            if base > 0.0 {
                ty = top + (base - own);
            }
            if seg.borrow().shift != 0 {
                let mut big: f64 = base;
                if big <= 0.0 {
                    big = self.baselineAt(&seg.borrow().family,
                        seg.borrow().bold,
                        seg.borrow().italic,
                        seg.borrow().shiftOf
                    );
                }
                let mut want: f64 = big + seg.borrow().shiftOf * 0.2;
                if seg.borrow().shift > 0 {
                    want = big - seg.borrow().shiftOf * 0.38;
                }
                ty = top + (want - own);
            }
            if (seg.borrow().code || seg.borrow().mark) || !seg.borrow().fill.is_empty() {
                let pad: f64 = 2.0;
                let mut fill: String = self.style.borrow().codeBackground.clone();
                if seg.borrow().mark {
                    fill = self.style.borrow().markBackground.clone();
                }
                if !seg.borrow().fill.is_empty() {
                    fill = seg.borrow().fill.clone();
                }
                let arg_0 = MdBox::rect(
                    sx - pad,
                    top + 1.0,
                    seg.borrow().w + pad * 2.0,
                    lh - 2.0,
                    fill.clone()
                ).clone();
                self.emit(arg_0);
            }
            if seg.borrow().kbd {
                let kp: f64 = 3.0;
                let mut k_face: String = self.style.borrow().kbdBackground.clone();
                if k_face.is_empty() {
                    k_face = self.style.borrow().codeBackground.clone();
                }
                let mut k_edge: String = self.style.borrow().kbdBorder.clone();
                if k_edge.is_empty() {
                    k_edge = self.style.borrow().codeBorder.clone();
                }
                let arg_0 = MdBox::rect(
                    sx - kp,
                    top + 2.0,
                    seg.borrow().w + kp * 2.0,
                    lh - 4.0,
                    k_face.clone()
                ).clone();
                self.emit(arg_0);
                let arg_0 = MdBox::outline(
                    sx - kp,
                    top + 2.0,
                    seg.borrow().w + kp * 2.0,
                    lh - 4.0,
                    k_edge.clone(),
                    1.0
                ).clone();
                self.emit(arg_0);
            }
            let mut _tmp_2985 = seg.borrow().text.clone();
            let mut b: Rc<RefCell<MdBox>> = MdBox::textBox(_tmp_2985, sx, ty, seg.borrow().w.clone(), lh);
            b.borrow_mut().segSrc = seg.borrow().srcStart;
            b.borrow_mut().segLen = seg.borrow().srcLen;
            b.borrow_mut().segAtom = seg.borrow().atom;
            b.borrow_mut().family = seg.borrow().family.clone();
            b.borrow_mut().size = seg.borrow().size;
            b.borrow_mut().bold = seg.borrow().bold;
            b.borrow_mut().italic = seg.borrow().italic;
            b.borrow_mut().color = seg.borrow().color.clone();
            b.borrow_mut().link = seg.borrow().link.clone();
            b.borrow_mut().strike = seg.borrow().strike;
            b.borrow_mut().quiet = seg.borrow().quiet;
            b.borrow_mut().noteRef = seg.borrow().noteRef;
            b.borrow_mut().spacing = seg.borrow().spacing;
            self.emit(b.clone());
            if seg.borrow().underline || seg.borrow().strike {
                let mut _tmp_3000 = seg.borrow().family.clone();
                let mut _tmp_3001 = seg.borrow().bold.clone();
                let mut _tmp_3002 = seg.borrow().italic.clone();
                let mut _tmp_3003 = seg.borrow().size.clone();
                let base_2: f64 = ty + self.baselineAt(&_tmp_3000,
                    _tmp_3001,
                    _tmp_3002,
                    _tmp_3003
                );
                let mut th: f64 = seg.borrow().size * 0.06;
                if th < 1.0 {
                    th = 1.0;
                }
                if seg.borrow().underline {
                    let arg_0 = MdBox::rect(
                        sx,
                        base_2 + seg.borrow().size * 0.1,
                        seg.borrow().w,
                        th,
                        seg.borrow().color.clone()
                    ).clone();
                    self.emit(arg_0);
                }
                if seg.borrow().strike {
                    let arg_0 = MdBox::rect(
                        sx,
                        base_2 - seg.borrow().size * 0.3,
                        seg.borrow().w,
                        th,
                        seg.borrow().color.clone()
                    ).clone();
                    self.emit(arg_0);
                }
            }
            s += 1;
        }
    }
    fn mathBox(&mut self, tex: &str, display: bool) -> Rc<RefCell<MathBox>> {
        let mut key: String = tex.to_string();
        if display {
            key = format!("display:{}", tex);
        }
        if self.mathBoxes.contains_key(&key) {
            return self.mathBoxes.get(&key).cloned().unwrap();
        }
        let mut b: Rc<RefCell<MathBox>> = MathLayout::layout(tex, display);
        self.mathBoxes.insert(key.clone(), b.clone());
        b
    }
    /// Where the painter puts the baseline of a run of `seg`'s face, down from
    /// the top of its line box — the rule the painters draw text by, so a
    /// formula stands on the same line as the words beside it.
    pub fn baselineIn(&self, seg: &Rc<RefCell<MdSeg>>) -> f64 {
        let mut _tmp_3004 = seg.borrow().family.clone();
        let mut _tmp_3005 = seg.borrow().bold.clone();
        let mut _tmp_3006 = seg.borrow().italic.clone();
        return self.baselineAt(&_tmp_3004, _tmp_3005, _tmp_3006, seg.borrow().size.clone());
    }
    pub fn baselineAt(&self, family: &str, bold: bool, italic: bool, size: f64) -> f64 {
        let face: String = self.style.borrow().faceFor(family, bold, italic);
        let mut m: Rc<RefCell<EVGTextMetrics>> = EVGTextEngine::measureRun(
            &self.engine,
            "Hg",
            &face,
            size
        );
        // a TrueType measurer reports hhea's descender, which is negative
        let asc: f64 = (m.borrow().ascent).abs();
        let desc: f64 = (m.borrow().descent).abs();
        if asc + desc <= 0.0 {
            return size * 0.9;
        }
        (size - (asc + desc)) / 2.0 + asc
    }
    pub fn mathSegBox(&mut self, seg: &Rc<RefCell<MdSeg>>, x: f64, line_top: f64) -> Rc<RefCell<MdBox>> {
        let mut _tmp_3010 = seg.borrow().math.clone();
        let mut mb: Rc<RefCell<MathBox>> = self.mathBox(&_tmp_3010, seg.borrow().mathDisplay.clone());
        let px: f64 = MdLayout::mathPx(seg.borrow().size.clone());
        let base: f64 = line_top + self.baselineIn(seg);
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 5;
        b.borrow_mut().x = x;
        let y_v = base - mb.borrow().h * px;
        b.borrow_mut().y = y_v;
        b.borrow_mut().w = seg.borrow().w;
        let h_v = (mb.borrow().h + mb.borrow().d) * px;
        b.borrow_mut().h = h_v;
        b.borrow_mut().size = px;
        b.borrow_mut().color = seg.borrow().color.clone();
        b.borrow_mut().embedSource = seg.borrow().math.clone();
        b.borrow_mut().embedKind = "math".to_string();
        if seg.borrow().mathDisplay {
            b.borrow_mut().embedKind = "math-display".to_string();
        }
        b.borrow_mut().segSrc = seg.borrow().srcStart;
        b.borrow_mut().segLen = seg.borrow().srcLen;
        // one thing to an editor's caret: `$x^2$` is passed and deleted whole
        b.borrow_mut().segAtom = true;
        b
    }
    /// A $$ display: centred in the column, scaled down when it would not fit.
    pub fn mathBlock(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let mut mb: Rc<RefCell<MathBox>> = self.mathBox(&node.borrow().literal.clone(), true);
        let mut px: f64 = MdLayout::mathPx(self.style.borrow().bodySize.clone());
        if mb.borrow().w * px > w && mb.borrow().w > 0.0 {
            px = w / mb.borrow().w;
        }
        let bw: f64 = mb.borrow().w * px;
        let bh: f64 = (mb.borrow().h + mb.borrow().d) * px;
        let pad: f64 = self.style.borrow().bodySize * 0.35;
        let y: f64 = self.reserve(bh + 2.0 * pad);
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 5;
        b.borrow_mut().x = x + (w - bw) / 2.0;
        b.borrow_mut().y = y + pad;
        b.borrow_mut().w = bw;
        b.borrow_mut().h = bh;
        b.borrow_mut().size = px;
        b.borrow_mut().color = self.style.borrow().textColor.clone();
        if self.quoteDepth > 0 {
            b.borrow_mut().color = self.style.borrow().quoteColor.clone();
        }
        b.borrow_mut().embedSource = node.borrow().literal.clone();
        b.borrow_mut().embedKind = "math-display".to_string();
        self.emit(b.clone());
        self.cursorY = ((y + bh) + 2.0 * pad) + self.style.borrow().paraSpacing;
    }
    /// ----------------------------------------------------------------------
    /// A paragraph: runs in, positioned lines out
    /// ----------------------------------------------------------------------
    pub fn paragraph(&mut self, runs: &Vec<Rc<RefCell<MdRun>>>, x: f64, w: f64) {
        let mut lines: Vec<Rc<RefCell<MdLine>>> = self.breakRuns(runs, w);
        let first_box: i64 = self.boxes.len() as i64;
        let mut i: i64 = 0;
        let n: i64 = lines.len() as i64;
        while i < n {
            let mut ln: Rc<RefCell<MdLine>> = lines[i as usize].clone();
            // a line that first refers to a footnote needs the footnote's
            // room on its page too: without it the line goes on and its note
            // goes nowhere, so the two go to the next page together
            let y: f64 = self.reserve(ln.borrow().height + self.noteRoomFor(&ln));
            let mut dx: f64 = 0.0;
            if self.align == "center" {
                dx = (w - ln.borrow().width) / 2.0;
            }
            if self.align == "right" {
                dx = w - ln.borrow().width;
            }
            if dx < 0.0 {
                dx = 0.0;
            }
            self.emitLine(&ln, x + dx, y);
            self.cursorY = y + ln.borrow().height;
            i += 1;
        }
        if self.align == "center" || self.align == "right" {
            let mut k: i64 = first_box;
            while k < (self.boxes.len() as i64) {
                let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
                if b.borrow().kind == 0 {
                    b.borrow_mut().align = self.align.clone();
                    b.borrow_mut().alignX = x;
                    b.borrow_mut().alignW = w;
                }
                k += 1;
            }
        }
        // where the column its lines were set in ends (an export widens a
        // text box to there, not to the slide's edge: in columns, or beside
        // a picture, that would be over the next column)
        let mut m: i64 = first_box;
        while m < (self.boxes.len() as i64) {
            let mut cb: Rc<RefCell<MdBox>> = self.boxes[m as usize].clone();
            if cb.borrow().kind == 0 {
                cb.borrow_mut().colRight = x + w;
            }
            m += 1;
        }
    }
    /// ----------------------------------------------------------------------
    /// From a point on the page back to a character in the file
    /// ----------------------------------------------------------------------
    ///
    /// This is the question a caret is, and until the segments carried their
    /// own offsets it could not be asked: every box of a paragraph held that
    /// PARAGRAPH's start, so the finest answer available was "the block you
    /// clicked". The viewer's `sourceOffsetAt` returned exactly that.
    ///
    /// It lives here rather than in the web facade because it is layout, not
    /// chrome: it has to measure with the measurer the layout measured with, or
    /// the caret lands beside the glyphs rather than between them. `PptxEdit`
    /// and `DocxEditController` each learned that separately.
    /// A box's y in DOCUMENT coordinates. `sheetGap` >= 0 means the sheets are
    /// stacked with that much between them; below zero means one column.
    /// A box's y in DOCUMENT coordinates. `sheetGap` >= 0 means the sheets are
    /// stacked with that much between them; below zero means one column.
    ///
    /// The gap runs BEFORE the first sheet too. A document whose first page
    /// starts at y=0 sits against the top of the window with nothing above it,
    /// which reads as a page that has been cut rather than one that starts.
    pub fn boxTop(&self, b: &Rc<RefCell<MdBox>>, sheet_gap: f64) -> f64 {
        if sheet_gap >= 0.0 {
            return (b.borrow().y + sheet_gap) + (b.borrow().page as f64) * (self.style.borrow().pageHeight + sheet_gap);
        }
        return b.borrow().y;
    }
    pub fn measureStyled(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
        let __len: i64 = MdText::str_len(&text) as i64;
        if __len == 0 {
            return 0.0;
        }
        let face: String = self.style.borrow().faceFor(family, bold, italic);
        let mut m: Rc<RefCell<EVGTextMetrics>> = EVGTextEngine::measureRun(
            &self.engine,
            text,
            &face,
            size
        );
        return m.borrow().width;
    }
    /// Which character of a placed run the pointer at `x` is over, as a source
    /// offset. Splits on glyph midpoints, so a click lands on the nearer side
    /// of a character rather than always its left — the same rule
    /// `OfficeTextMetrics.offsetAtX` follows for the .docx caret.
    ///
    /// The walk is over the box's TEXT and the answer is in the FILE, and those
    /// are the same length only where the run holds no escape and no entity. So
    /// the result is clamped into the box's own span: an approximate offset
    /// inside a run is a caret one character out, while an unclamped one is a
    /// caret in the next paragraph.
    pub fn offsetInBox(&self, b: &Rc<RefCell<MdBox>>, x: f64) -> i64 {
        if b.borrow().segSrc < 0 {
            return -1;
        }
        let __len: i64 = MdText::str_len(&b.borrow().text) as i64;
        if __len == 0 {
            return b.borrow().segSrc;
        }
        let want: f64 = x - b.borrow().x;
        if want <= 0.0 {
            return b.borrow().segSrc;
        }
        let mut k: i64 = 0;
        let mut hit: i64 = __len;
        while k < __len {
            let mut _tmp_3030 = b.borrow().family.clone();
            let mut _tmp_3031 = b.borrow().size.clone();
            let mut _tmp_3032 = b.borrow().bold.clone();
            let mut _tmp_3033 = b.borrow().italic.clone();
            let w0: f64 = self.measureStyled(&MdText::substr(&b.borrow().text, 0, k),
                &_tmp_3030,
                _tmp_3031,
                _tmp_3032,
                _tmp_3033
            );
            let mut _tmp_3034 = b.borrow().family.clone();
            let mut _tmp_3035 = b.borrow().size.clone();
            let mut _tmp_3036 = b.borrow().bold.clone();
            let mut _tmp_3037 = b.borrow().italic.clone();
            let w1: f64 = self.measureStyled(&MdText::substr(&b.borrow().text, 0, k + 1),
                &_tmp_3034,
                _tmp_3035,
                _tmp_3036,
                _tmp_3037
            );
            if want < (w0 + w1) * 0.5 {
                hit = k;
                k = __len;
            } else {
                k += 1;
            }
        }
        if b.borrow().segAtom {
            // the nearer END of what the glyph stands for
            if hit * 2 < __len {
                return b.borrow().segSrc;
            }
            if hit * 2 == __len {
                if want * 2.0 < b.borrow().w {
                    return b.borrow().segSrc;
                }
            }
            return b.borrow().segSrc + b.borrow().segLen;
        }
        let mut at: i64 = b.borrow().segSrc + hit;
        let stop: i64 = b.borrow().segSrc + b.borrow().segLen;
        if at > stop {
            at = stop;
        }
        at
    }
    /// The text box at a point in document coordinates, or -1. A click to the
    /// right of the last word on a line belongs to that line rather than to
    /// nothing, so the nearest box on the line the pointer is ON wins — which
    /// is what puts a caret at the end of a line when a reader clicks past it.
    pub fn boxAtPoint(&self, x: f64, y: f64, sheet_gap: f64) -> i64 {
        let mut best: i64 = -1;
        let mut best_dx: f64 = 0.0;
        let mut i: i64 = 0;
        let n: i64 = self.boxes.len() as i64;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b.borrow().kind == 0 && b.borrow().segSrc >= 0 {
                let top: f64 = self.boxTop(&b, sheet_gap);
                if y >= top && y < top + b.borrow().h {
                    let mut dx: f64 = 0.0;
                    if x < b.borrow().x {
                        dx = b.borrow().x - x;
                    }
                    if x > b.borrow().x + b.borrow().w {
                        dx = x - (b.borrow().x + b.borrow().w);
                    }
                    if best < 0 {
                        best = i;
                        best_dx = dx;
                    } else {
                        if dx < best_dx {
                            best = i;
                            best_dx = dx;
                        }
                    }
                }
            }
            i += 1;
        }
        best
    }
    /// The whole question in one call: a point on the page, a character in the
    /// file, or -1 when nothing that carries a source was drawn there.
    pub fn srcAtPoint(&self, x: f64, y: f64, sheet_gap: f64) -> i64 {
        let i: i64 = self.boxAtPoint(x, y, sheet_gap);
        if i < 0 {
            return -1;
        }
        let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
        self.offsetInBox(&b, x)
    }
    pub fn measureRun(&mut self, text: &str, r: &Rc<RefCell<MdRun>>) -> f64 {
        let __len: i64 = MdText::str_len(&text) as i64;
        if __len == 0 {
            return 0.0;
        }
        if !r.borrow().math.is_empty() {
            let mut _tmp_3038 = r.borrow().math.clone();
            let mut mb: Rc<RefCell<MathBox>> = self.mathBox(&_tmp_3038, r.borrow().mathDisplay.clone());
            return mb.borrow().w * MdLayout::mathPx(r.borrow().size.clone());
        }
        let mut _tmp_3042 = r.borrow().family.clone();
        let mut _tmp_3043 = r.borrow().bold.clone();
        let face: String = self.style.borrow().faceFor(&_tmp_3042, _tmp_3043, r.borrow().italic.clone());
        let mut _tmp_3046 = r.borrow().size.clone();
        let mut m: Rc<RefCell<EVGTextMetrics>> = EVGTextEngine::measureRun(
            &self.engine,
            text,
            &face,
            _tmp_3046
        );
        if r.borrow().spacing != 0.0 {
            return m.borrow().width + EVGTextEngine::trackingWidth(text, r.borrow().spacing.clone());
        }
        return m.borrow().width;
    }
    /// ----------------------------------------------------------------------
    /// Breaking a line across several faces
    /// ----------------------------------------------------------------------
    ///
    /// The algorithm `EVGTextEngine` uses, with one change: the accumulator is
    /// a list of segments rather than a string, so a change of face is a new
    /// segment instead of the end of the line. Each segment is measured WHOLE
    /// — not as a sum of word widths plus a guessed space — which is what
    /// makes the break land where the painter will draw it.
    pub fn breakRuns(&mut self, runs: &Vec<Rc<RefCell<MdRun>>>, max_width: f64) -> Vec<Rc<RefCell<MdLine>>> {
        let mut lines: Vec<Rc<RefCell<MdLine>>> = Vec::new();
        // --- atoms: words, runs of spaces, and forced breaks ---------------
        let mut a_text: Vec<String> = Vec::new();
        let mut a_run: Vec<i64> = Vec::new();
        let mut a_space: Vec<bool> = Vec::new();
        let mut a_break: Vec<bool> = Vec::new();
        // Where each atom starts inside its run's text. The atoms are what a
        // segment is assembled from, so this is the only place the offset is
        // still known — by the time `flushOpen` sees `openText` it is a string
        // with no memory of where it was cut from.
        let mut a_off: Vec<i64> = Vec::new();
        let mut ri: i64 = 0;
        let rn: i64 = runs.len() as i64;
        while ri < rn {
            let mut r: Rc<RefCell<MdRun>> = runs[ri as usize].clone();
            if r.borrow().hardBreak {
                a_text.push(String::new().to_string());
                a_run.push(ri);
                a_space.push(false);
                a_break.push(true);
                a_off.push(0);
            } else {
                let t: String = r.borrow().text.clone();
                let tn: i64 = MdText::str_len(&t) as i64;
                let mut i: i64 = 0;
                while i < tn {
                    let start: i64 = i;
                    let c: i64 = MdText::char_at(&t, i);
                    let is_space: bool = MdChar::isSpaceOrTab(c);
                    let mut scanning: bool = true;
                    while scanning {
                        if i >= tn {
                            scanning = false;
                        }
                        if scanning {
                            let c2: i64 = MdText::char_at(&t, i);
                            let s2: bool = MdChar::isSpaceOrTab(c2);
                            if s2 == is_space {
                                i += 1;
                            } else {
                                scanning = false;
                            }
                        }
                    }
                    a_text.push(MdText::substr(&t, start, i));
                    a_run.push(ri);
                    a_space.push(is_space);
                    a_break.push(false);
                    a_off.push(start);
                }
            }
            ri += 1;
        }
        // --- the greedy fill ----------------------------------------------
        let mut cur: Rc<RefCell<MdLine>> = Rc::new(RefCell::new(MdLine::new()));
        let mut used_width: f64 = 0.0;
        let mut open_run: i64 = -1;
        let mut open_text: String = String::new();
        let mut open_off: i64 = 0;
        // What `openText` measures, carried forward instead of measured again.
        // `prevAtom` is the last atom that went in, and `prevAtomW` what it
        // measures alone — the pair that lets the next atom be charged its
        // advance AND its kern without re-reading the whole segment.
        let mut open_w: f64 = 0.0;
        let mut prev_atom: String = String::new();
        let mut prev_atom_w: f64 = 0.0;
        let mut ai: i64 = 0;
        let an: i64 = a_text.len() as i64;
        let mut guard: i64 = 0;
        while ai < an {
            guard += 1;
            if guard > 2000000 {
                ai = an;
            }
            if ai < an {
                let hard: bool = a_break[ai as usize];
                let is_space_1: bool = a_space[ai as usize];
                let seg_count: i64 = cur.borrow().segs.len() as i64;
                let line_empty: bool = seg_count == 0 && open_text.is_empty();
                if hard {
                    used_width = self.flushOpen(&cur,
                        runs,
                        open_run,
                        open_text.clone(),
                        open_off,
                        used_width
                    );
                    open_run = -1;
                    open_text = String::new();
                    open_off = 0;
                    open_w = 0.0;
                    prev_atom = String::new();
                    prev_atom_w = 0.0;
                    let mut done: Rc<RefCell<MdLine>> = self.finishLine(&cur);
                    lines.push(done.clone());
                    cur = Rc::new(RefCell::new(MdLine::new()));
                    used_width = 0.0;
                    ai += 1;
                } else {
                    if is_space_1 && line_empty {
                        // a wrapped line does not begin with the space it
                        // wrapped at
                        ai += 1;
                    } else {
                        let r_1: i64 = a_run[ai as usize];
                        let t_1: String = a_text[ai as usize].clone();
                        let mut run: Rc<RefCell<MdRun>> = runs[r_1 as usize].clone();
                        let mut candidate: f64 = 0.0;
                        // What the line measures with this atom on it, and —
                        // when it is taken — what the open segment then
                        // measures.
                        //
                        // Measuring `openText + t` outright is what this did
                        // first, and it re-read the whole open segment once per
                        // word: quadratic in the segment, with a cache entry
                        // per prefix. A kern pair depends only on the two
                        // codepoints either side of the join, so `prevAtom + t`
                        // is charged the same advance and the same pair as the
                        // whole string would be — the growth is
                        // `measure(prevAtom + t) - measure(prevAtom)`, and the
                        // running total stays the number a whole-string
                        // measurement gives.
                        let mut grown: f64 = 0.0;
                        if r_1 == open_run {
                            let joined: f64 = self.measureRun(&format!("{}{}", prev_atom, t_1),
                                &run
                            );
                            grown = (open_w + joined) - prev_atom_w;
                            candidate = used_width + grown;
                            // Near the break, throw the running total away and
                            // ask for the whole segment. A sum of parts and a
                            // single measurement agree to the last bit only by
                            // luck, and this is the one place where the last
                            // bit decides where the line ends.
                            let mut slack: f64 = candidate - max_width;
                            if slack < 0.0 {
                                slack = 0.0 - slack;
                            }
                            if slack < 1.0 {
                                grown = self.measureRun(&format!("{}{}", open_text, t_1), &run);
                                candidate = used_width + grown;
                            }
                        } else {
                            let mut open_run_obj: Rc<RefCell<MdRun>> = run.clone();
                            if open_run >= 0 {
                                open_run_obj = runs[open_run as usize].clone();
                            }
                            let mut open_exact: f64 = 0.0;
                            if open_run >= 0 {
                                open_exact = self.measureRun(&open_text, &open_run_obj);
                            }
                            grown = self.measureRun(&t_1, &run);
                            candidate = (used_width + open_exact) + grown;
                        }
                        let starts_segment: bool = r_1 != open_run;
                        if is_space_1 && starts_segment {
                            // A space that would OPEN a segment becomes pure
                            // advance instead of text.
                            //
                            // It has no glyph, so nothing is lost — and every
                            // consumer that trims the edges of a text run (an
                            // HTML preview does; so does anything that treats
                            // a run as a line box) then draws the same
                            // geometry as the PDF. Before this, `[LICENSE]`
                            // followed by ` for details` lost the space
                            // between them in the preview while the PDF kept
                            // it: one layout, two pictures.
                            used_width = self.flushOpen(&cur,
                                runs,
                                open_run,
                                open_text.clone(),
                                open_off,
                                used_width
                            );
                            open_run = -1;
                            open_text = String::new();
                            open_off = 0;
                            open_w = 0.0;
                            prev_atom = String::new();
                            prev_atom_w = 0.0;
                            used_width += self.measureRun(&t_1, &run);
                            ai += 1;
                        } else {
                            let overflows: bool = (candidate > max_width && false == line_empty) && false == is_space_1;
                            if overflows {
                                used_width = self.flushOpen(&cur,
                                    runs,
                                    open_run,
                                    open_text.clone(),
                                    open_off,
                                    used_width
                                );
                                open_run = -1;
                                open_text = String::new();
                                open_off = 0;
                                open_w = 0.0;
                                prev_atom = String::new();
                                prev_atom_w = 0.0;
                                let mut wrapped: Rc<RefCell<MdLine>> = self.finishLine(&cur);
                                lines.push(wrapped.clone());
                                cur = Rc::new(RefCell::new(MdLine::new()));
                                used_width = 0.0;
                            } else {
                                if r_1 == open_run {
                                    open_text = format!("{}{}", open_text, t_1);
                                } else {
                                    used_width = self.flushOpen(&cur,
                                        runs,
                                        open_run,
                                        open_text.clone(),
                                        open_off,
                                        used_width
                                    );
                                    open_run = r_1;
                                    open_text = t_1.clone();
                                    open_off = a_off[ai as usize];
                                }
                                open_w = grown;
                                prev_atom = t_1.clone();
                                prev_atom_w = self.measureRun(&t_1, &run);
                                ai += 1;
                            }
                        }
                    }
                }
            }
        }
        used_width = self.flushOpen(&cur,
            runs,
            open_run,
            open_text.clone(),
            open_off,
            used_width
        );
        let leftover: i64 = cur.borrow().segs.len() as i64;
        let any: i64 = lines.len() as i64;
        if leftover > 0 || any == 0 {
            let mut last: Rc<RefCell<MdLine>> = self.finishLine(&cur);
            lines.push(last.clone());
        }
        lines
    }
    /// The open segment, measured and placed. Returns the new used width.
    pub fn flushOpen(&mut self, cur: &Rc<RefCell<MdLine>>, runs: &Vec<Rc<RefCell<MdRun>>>, open_run: i64, open_text: String, open_off: i64, used_width: f64) -> f64 {
        if open_run < 0 {
            return used_width;
        }
        let __len: i64 = MdText::str_len(&open_text) as i64;
        if __len == 0 {
            return used_width;
        }
        let mut run: Rc<RefCell<MdRun>> = runs[open_run as usize].clone();
        let mut seg: Rc<RefCell<MdSeg>> = Rc::new(RefCell::new(MdSeg::new()));
        seg.borrow_mut().text = open_text.clone();
        // Where this piece sits in the document. `openOff` is how far into the
        // run's TEXT it started, and text and source are the same length only
        // where the run holds no escape and no entity — so the answer is
        // clamped to the run's own span rather than allowed to run past the
        // node it belongs to. `MdSrcMapScan` is what says how often the two
        // are the same string: on CommonMark's own corpus, every time it can
        // be checked at all.
        if run.borrow().srcStart >= 0 {
            let mut at: i64 = run.borrow().srcStart + open_off;
            if run.borrow().srcEnd >= run.borrow().srcStart {
                if at > run.borrow().srcEnd {
                    at = run.borrow().srcEnd;
                }
            }
            seg.borrow_mut().srcStart = at;
            let mut upto: i64 = at + __len;
            if run.borrow().srcEnd >= run.borrow().srcStart {
                if upto > run.borrow().srcEnd {
                    upto = run.borrow().srcEnd;
                }
            }
            seg.borrow_mut().srcLen = upto - at;
            if seg.borrow().srcLen < 0 {
                seg.borrow_mut().srcLen = 0;
            }
            if run.borrow().atom {
                // all of it or none of it: the run's text is not its source
                seg.borrow_mut().atom = true;
                seg.borrow_mut().srcStart = run.borrow().srcStart;
                seg.borrow_mut().srcLen = run.borrow().srcEnd - run.borrow().srcStart;
            }
        }
        seg.borrow_mut().family = run.borrow().family.clone();
        seg.borrow_mut().size = run.borrow().size;
        seg.borrow_mut().bold = run.borrow().bold;
        seg.borrow_mut().italic = run.borrow().italic;
        seg.borrow_mut().color = run.borrow().color.clone();
        seg.borrow_mut().code = run.borrow().code;
        seg.borrow_mut().link = run.borrow().link.clone();
        seg.borrow_mut().math = run.borrow().math.clone();
        seg.borrow_mut().mathDisplay = run.borrow().mathDisplay;
        seg.borrow_mut().underline = run.borrow().underline;
        seg.borrow_mut().strike = run.borrow().strike;
        seg.borrow_mut().quiet = run.borrow().quiet;
        seg.borrow_mut().mark = run.borrow().mark;
        seg.borrow_mut().fill = run.borrow().fill.clone();
        seg.borrow_mut().kbd = run.borrow().kbd;
        seg.borrow_mut().shift = run.borrow().shift;
        seg.borrow_mut().shiftOf = run.borrow().shiftOf;
        seg.borrow_mut().noteRef = run.borrow().noteRef;
        seg.borrow_mut().spacing = run.borrow().spacing;
        seg.borrow_mut().x = used_width;
        seg.borrow_mut().w = self.measureRun(&open_text, &run);
        cur.borrow_mut().segs.push(seg.clone());
        return used_width + seg.borrow().w;
    }
    /// A line's height and baseline are the maximum over its segments, so a
    /// small code span inside larger prose does not pull the line up.
    pub fn finishLine(&mut self, cur: &Rc<RefCell<MdLine>>) -> Rc<RefCell<MdLine>> {
        let n: i64 = cur.borrow().segs.len() as i64;
        let mut height: f64 = self.style.borrow()
            .bodySize * self.style.borrow()
            .lineHeight;
        let mut ascent: f64 = self.style.borrow().bodySize * 0.9;
        let mut width: f64 = 0.0;
        if n > 0 {
            // the trailing space a wrap left behind is not part of the line
            let mut last: Rc<RefCell<MdSeg>> = cur.borrow().segs[(n - 1) as usize].clone();
            let trimmed: String = MdLayout::trimTrailing(&last.borrow().text.clone());
            let tlen: i64 = MdText::str_len(&trimmed) as i64;
            let olen: i64 = MdText::str_len(&last.borrow().text) as i64;
            if tlen != olen {
                last.borrow_mut().text = trimmed.clone();
                // …and the source it covers shrinks with it, or a caret
                // dragged to the end of a wrapped line lands on a space the
                // line no longer draws.
                if last.borrow().srcLen > olen - tlen {
                    let srcLen_v = last.borrow().srcLen - (olen - tlen);
                    last.borrow_mut().srcLen = srcLen_v;
                } else {
                    last.borrow_mut().srcLen = 0;
                }
                let mut r: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
                r.borrow_mut().family = last.borrow().family.clone();
                r.borrow_mut().size = last.borrow().size;
                r.borrow_mut().bold = last.borrow().bold;
                r.borrow_mut().italic = last.borrow().italic;
                last.borrow_mut().w = self.measureRun(&trimmed, &r);
            }
            height = 0.0;
            let mut above: f64 = 0.0;
            let mut below: f64 = 0.0;
            let mut i: i64 = 0;
            while i < n {
                let mut seg: Rc<RefCell<MdSeg>> = cur.borrow().segs[i as usize].clone();
                let mut _tmp_3078 = seg.borrow().family.clone();
                let mut _tmp_3079 = seg.borrow().bold.clone();
                let face: String = self.style.borrow().faceFor(&_tmp_3078, _tmp_3079, seg.borrow().italic.clone());
                let mut _tmp_3082 = seg.borrow().size.clone();
                let mut m: Rc<RefCell<EVGTextMetrics>> = EVGTextEngine::measureRun(
                    &self.engine,
                    "Hg",
                    &face,
                    _tmp_3082
                );
                let h: f64 = seg.borrow().size * self.style.borrow().lineHeight;
                if h > height {
                    height = h;
                }
                if m.borrow().ascent > ascent {
                    ascent = m.borrow().ascent;
                }
                width = seg.borrow().x + seg.borrow().w;
                if !seg.borrow().math.is_empty() {
                    // how far the formula reaches past the text's own
                    // baseline, up and down
                    let mut _tmp_3083 = seg.borrow().math.clone();
                    let mut mb: Rc<RefCell<MathBox>> = self.mathBox(&_tmp_3083, seg.borrow().mathDisplay.clone());
                    let px: f64 = MdLayout::mathPx(seg.borrow().size.clone());
                    let base: f64 = self.baselineIn(&seg);
                    let up: f64 = mb.borrow().h * px - base;
                    let down: f64 = (base + mb.borrow().d * px) - h;
                    if up > above {
                        above = up;
                    }
                    if down > below {
                        below = down;
                    }
                }
                i += 1;
            }
            cur.borrow_mut().extraTop = above;
            height = (height + above) + below;
        }
        cur.borrow_mut().height = height;
        cur.borrow_mut().ascent = ascent;
        cur.borrow_mut().width = width;
        cur
    }
    /// ----------------------------------------------------------------------
    /// Tables
    /// ----------------------------------------------------------------------
    pub fn table(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let rows: i64 = node.borrow().childCount();
        if rows == 0 {
            return;
        }
        let mut head: Rc<RefCell<MdNode>> = node.borrow().childAt(0);
        let cols: i64 = head.borrow().childCount();
        if cols == 0 {
            return;
        }
        // `{width=60%}` under it: that share of the column, and the text
        // after it beside it (startFloat)
        let tw: f64 = MdEmbedKinds::widthOf(&self.widthAttr(node), w);
        let was: f64 = self.tableTextOn();
        let mut widths: Vec<f64> = self.gridWidths(node, tw);
        // cells coloured by their text (`{cells=…}`, `table { cell-tones }`)
        let mut tones: Rc<RefCell<MdCellTones>> = MdCellTones::forTable(node, &self.style.borrow().tableCellTones.clone());
        // the header rows, said again on every slide the table continues on
        let mut heads: i64 = 0;
        while heads < rows && MdLayout::isHeadRow(&node.borrow().childAt(heads)) {
            heads += 1;
        }
        let mut heights: Vec<f64> = Vec::new();
        let mut head_h: f64 = 0.0;
        let mut r0: i64 = 0;
        while r0 < rows {
            let rh: f64 = self.tableRowHeight(&node.borrow().childAt(r0),
                &widths,
                cols,
                &tones
            );
            if r0 < heads {
                head_h += rh;
            } else {
                heights.push(rh);
            }
            r0 += 1;
        }
        let mut plan: Rc<RefCell<MdTablePlan>> = self.tablePlan(&heights, head_h);
        let mut on: i64 = -1;
        let mut r2: i64 = heads;
        while r2 < rows {
            let s: i64 = plan.borrow().slideOf[(r2 - heads) as usize];
            if s != on {
                if on >= 0 {
                    self.tableBreak();
                } else {
                    if s > 0 {
                        self.tableBreak();
                    }
                }
                on = s;
                let mut hr: i64 = 0;
                while hr < heads {
                    self.tableRow(&node.borrow().childAt(hr).clone(), x, &widths, cols, &tones);
                    hr += 1;
                }
            }
            self.tableRow(&node.borrow().childAt(r2).clone(), x, &widths, cols, &tones);
            r2 += 1;
        }
        if heads == rows {
            let mut hr2: i64 = 0;
            while hr2 < heads {
                self.tableRow(&node.borrow().childAt(hr2).clone(), x, &widths, cols, &tones);
                hr2 += 1;
            }
        }
        self.style.borrow_mut().bodySize = was;
        self.cursorY += self.style.borrow().paraSpacing;
    }
    pub fn tablePlan(&mut self, groups: &[f64], head_h: f64) -> Rc<RefCell<MdTablePlan>> {
        if !self.paginate || self.page >= MdCssLimits::pageBudget() {
            let mut flat: Rc<RefCell<MdTablePlan>> = Rc::new(RefCell::new(MdTablePlan::new()));
            let mut i: i64 = 0;
            while i < (groups.len() as i64) {
                flat.borrow_mut().slideOf.push(0);
                i += 1;
            }
            return flat;
        }
        let mut title_h: f64 = 0.0;
        if self.slides {
            title_h = self.continuationTitleHeight(true);
        }
        let first_room: f64 = self.contentBottom - self.cursorY;
        let next_room: f64 = (self.contentBottom - self.contentTop) - title_h;
        let mut _tmp_3088 = self.slides.clone();
        let mut plan: Rc<RefCell<MdTablePlan>> = MdTableSplit::plan(
            groups,
            head_h,
            first_room,
            next_room,
            _tmp_3088
        );
        if self.slides && plan.borrow().slides == 2 {
            // the header said again is there because of the cut, like the
            // title over it
            let arg_0 = self.page + 1;
            self.setSpillExcess(arg_0, (plan.borrow().tail - plan.borrow().greedyTail) + head_h);
        }
        plan
    }
    /// A table's own break: the next slide, the title again, and the count a
    /// cut block adds to, exactly as `reserve` does when a block runs over.
    pub fn tableBreak(&mut self) {
        if self.page >= MdCssLimits::pageBudget() {
            return;
        }
        self.page += 1;
        self.pageCount = self.page + 1;
        self.cursorY = self.contentTop;
        if self.slides {
            self.slideOverflows += 1;
            self.emitSlideTitle();
        }
    }
    pub fn setSpillExcess(&mut self, p: i64, v: f64) {
        while (self.spillExcess.len() as i64) <= p {
            self.spillExcess.push(0.0);
        }
        self.spillExcess[p as usize] = v;
    }
    pub fn spillExcessAt(&self, p: i64) -> f64 {
        if p < 0 || p >= (self.spillExcess.len() as i64) {
            return 0.0;
        }
        self.spillExcess[p as usize]
    }
    /// Slide p + 1 was folded into p (`shrinkSlide`): the pages noted for
    /// the slides after p move up one, as their boxes did. A hidden title
    /// written after the shrunk slide was still noted on the page below its
    /// own, so it named the slide before it and its slide began at the
    /// first thing drawn instead of at its heading (deleting that slide by
    /// number left the heading behind).
    pub fn pagesMovedUp(&mut self, p: i64) {
        MdLayout::shiftAfter(&mut self.hiddenPage, p);
        MdLayout::shiftAfter(&mut self.headPage, p);
        MdLayout::shiftAfter(&mut self.namedPage, p);
        MdLayout::shiftAfter(&mut self.metaPage, p);
        self.shiftSuspect(p);
        MdLayout::shiftAfter(&mut self.overlayPage, p);
        MdLayout::shiftAfter(&mut self.noteFirstPage, p);
        if self.hiddenOnPage > p {
            self.hiddenOnPage -= 1;
        }
    }
    /// the slide `p` went: what was recorded for the slides after it moves up
    pub fn dropSpillExcess(&mut self, p: i64) {
        let mut kept: Vec<f64> = Vec::new();
        let mut i: i64 = 0;
        while i < (self.spillExcess.len() as i64) {
            if i != p {
                kept.push(self.spillExcess[i as usize]);
            }
            i += 1;
        }
        self.spillExcess = kept.clone();
    }
    /// ----------------------------------------------------------------------
    /// A picture
    /// ----------------------------------------------------------------------
    ///
    /// `![alt](logo.png)` on a paragraph of its own becomes a picture; anywhere
    /// else it stays ALT TEXT.
    ///
    /// That split is deliberate rather than a first step. A picture in the
    /// middle of a sentence has to sit on a text baseline, which is a different
    /// question from putting a block on a page and one markdown almost never
    /// asks — every `![]()` in this repository's own documents is on a line of
    /// its own. A block picture is the case that exists, and it is the one that
    /// reaches a deck, a PDF and a slide.
    ///
    /// A picture nobody can find stays alt text too. That is not a fallback
    /// that hides a problem: it is exactly what the reader sees in the preview,
    /// so the preview and the export are missing the same picture rather than
    /// disagreeing about it. `missingImages` counts them.
    /// One picture the store does not have, counted once per layout.
    pub fn missed(&mut self, key: &str) {
        let mut i: i64 = 0;
        while i < (self.missingKeys.len() as i64) {
            if self.missingKeys[i as usize].clone() == key {
                return;
            }
            i += 1;
        }
        self.missingKeys.push(key.to_string());
        self.missingImages += 1;
    }
    pub fn pictureBlock(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) -> bool {
        if false == self.haveImages {
            return false;
        }
        let mut img: Rc<RefCell<MdNode>> = MdLayout::loneImage(node);
        if img.borrow().kind != "image" {
            return false;
        }
        let mut _tmp_3092 = img.borrow().destination.clone();
        let path: String = Vfs::resolve(&self.images, &_tmp_3092, &self.docDir.clone());
        if path.is_empty() {
            self.missed(&format!("block@{}", node.borrow().srcStart).clone());
            return false;
        }
        let mut st: Rc<RefCell<VfsStat>> = Vfs::stat(&self.images, &path);
        if false == st.borrow().found {
            self.missed(&format!("block@{}", node.borrow().srcStart).clone());
            return false;
        }
        // drawn by the host over the page: no room here (a pass that only
        // measures what follows a picture does not place it)
        if self.isOverlayType(&st.borrow().contentType.clone()) {
            if self.paginate {
                let mut _tmp_3096 = node.borrow().srcStart.clone();
                self.noteOverlay(_tmp_3096, &path, self.page.clone());
            }
            return true;
        }
        // Its own size, never scaled UP — a 64pt icon in a 468pt column is a
        // 64pt icon — and fitted down when it is wider than the column.
        let nat_w: f64 = st.borrow().pixelW as f64;
        let nat_h: f64 = st.borrow().pixelH as f64;
        let mut use_w: f64 = nat_w;
        let mut use_h: f64 = nat_h;
        if nat_w <= 0.0 {
            use_w = w;
            use_h = w * 0.5;
        }
        let want_attr: String = self.widthAttr(node);
        let asked: f64 = MdEmbedKinds::widthOf(&want_attr, w);
        // On a slide a `width` is the width it is shown at, larger than its
        // pixels too (the handles on the slide set it); a document page
        // only ever fits it down.
        let grows: bool = (self.slides && self.paginate) && !want_attr.is_empty();
        if asked < use_w || grows {
            if use_w > 0.0 {
                use_h *= asked / use_w;
                use_w = asked;
            }
        }
        if use_w > w {
            use_h *= w / use_w;
            use_w = w;
        }
        // `{height=200}` / `{height=50%}` (of the page's content height), and
        // the theme's `img { max-height }`: the picture keeps its shape
        let full_h: f64 = self.contentBottom - self.contentTop;
        let mut max_h: f64 = 0.0;
        let h_attr: String = node.borrow().attrValue("height");
        if !h_attr.is_empty() {
            let mut h_base: f64 = full_h;
            if h_base <= 0.0 {
                h_base = use_h;
            }
            max_h = MdEmbedKinds::widthOf(&h_attr, h_base);
        }
        if self.style.borrow()
            .imgMaxHeight > 0.0 && (max_h <= 0.0 || self.style.borrow().imgMaxHeight < max_h) {
            if h_attr.is_empty() {
                max_h = self.style.borrow().imgMaxHeight;
            }
        }
        if max_h > 0.0 && use_h > max_h {
            use_w *= max_h / use_h;
            use_h = max_h;
        }
        // …and on a slide a `height` (without a `width`) the height it is
        // shown at, as wide as that makes it up to the column
        if (((self.slides && self.paginate) && !h_attr.is_empty()) && want_attr.is_empty()) && (use_h > 0.0 && (max_h > use_h && use_w > 0.0)) {
            let mut hk: f64 = max_h / use_h;
            if use_w * hk > w {
                hk = w / use_w;
            }
            use_w *= hk;
            use_h *= hk;
        }
        // On a slide a picture is shown at the size of its room: the column's
        // width and the height under its heading, less what is written under
        // it on the same slide (`followH`), so a large picture (an SVG is
        // drawn 2560 wide) leaves its caption on the slide and a small one
        // (a 160×90 PNG) is not a stamp in a corner. Set larger at most 4
        // times; a picture given a `width` or `height` is as asked. A
        // document page keeps a picture's own size.
        // A picture narrow enough for the text after it to go beside it
        // (startFloat) leaves no room under itself for that text.
        let mut under: f64 = self.followH;
        if (self.followBeside && !want_attr.is_empty()) && use_w <= self.contentW * 0.75 {
            if (self.contentW - use_w) - self.style.borrow()
                .bodySize * 1.2 >= self.contentW * 0.25 {
                under = 0.0;
            }
        }
        if (self.slides && self.paginate) && (want_attr.is_empty() && h_attr.is_empty()) {
            let room_h: f64 = (self.contentBottom - self.cursorY) - self.followH;
            let up: f64 = MdLayout::pictureScale(use_w, use_h, w, room_h, 4.0);
            use_w *= up;
            use_h *= up;
        }
        // On a slide a picture taller than the room under its heading is
        // given that room (shrunk, its shape kept) rather than running off
        // the bottom (and above the text under it, when there is room for
        // both); on a full slide it moves on as it did.
        if self.slides && self.paginate {
            let mut avail: f64 = self.contentBottom - self.cursorY;
            if avail - under >= full_h * 0.25 {
                avail -= under;
            }
            let title_only: bool = self.slideTitleOnly && avail >= full_h * 0.2;
            if use_h > avail && (avail >= full_h * 0.35 || title_only) {
                use_w *= avail / use_h;
                use_h = avail;
            }
            if use_h > full_h {
                use_w *= full_h / use_h;
                use_h = full_h;
            }
        }
        // `{float=top-right}`: in that corner of the slide, out of the flow
        let corner: String = MdSlideLayout::cornerOf(&node.borrow().attrValue("float"));
        if !corner.is_empty() && self.slides {
            if self.paginate {
                self.pinPicture(node, &corner, path.clone(), &img, &st);
            }
            return true;
        }
        // where it sits in the column: `{align=center}`, else the theme's
        // `img { justify-content }`, else centred on a slide
        let mut align_1: String = MdCss::alignWord(&node.borrow().attrValue("align"));
        if align_1.is_empty() {
            align_1 = self.style.borrow().imgAlign.clone();
        }
        if align_1.is_empty() && self.slides {
            align_1 = "center".to_string();
        }
        let mut px: f64 = x;
        if align_1 == "center" {
            px = x + (w - use_w) * 0.5;
        }
        if align_1 == "end" {
            px = (x + w) - use_w;
        }
        let y: f64 = self.reserve(use_h);
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 4;
        b.borrow_mut().x = px;
        b.borrow_mut().y = y;
        b.borrow_mut().w = use_w;
        b.borrow_mut().h = use_h;
        b.borrow_mut().imagePath = path.clone();
        b.borrow_mut().imageAlt = img.borrow().plainText();
        b.borrow_mut().imageNaturalW = st.borrow().pixelW;
        b.borrow_mut().imageNaturalH = st.borrow().pixelH;
        self.emit(b.clone());
        self.cursorY = (y + use_h) + self.style.borrow().paraSpacing;
        true
    }
    pub fn gallery(&mut self, node: &Rc<RefCell<MdNode>>, x: f64, w: f64) {
        let mut g: Rc<RefCell<MdGallery>> = MdGallery::of(node);
        if g.borrow().items.is_empty() {
            return;
        }
        let mut look: Rc<RefCell<MdGalleryLook>> = g.borrow().look(&self.sheet.clone());
        if g.borrow().layout == "full" && self.paginate {
            self.galleryFull(&g, &look);
            return;
        }
        self.galleryGrid(&g, &look, x, w);
    }
    pub fn galleryGrid(&mut self, g: &Rc<RefCell<MdGallery>>, look: &Rc<RefCell<MdGalleryLook>>, x: f64, w: f64) {
        let mut cols: i64 = g.borrow().columnsFor(look);
        let mut gap: f64 = look.borrow().gap;
        if gap > w * 0.2 {
            gap = w * 0.2;
        }
        // on a slide, as many across as show the most of the pictures in
        // the room there is (portraits side by side, not cropped wide)
        if self.slides && self.paginate {
            let mut aspects: Vec<f64> = Vec::new();
            let mut any_cap: bool = false;
            let mut ai: i64 = 0;
            while ai < (g.borrow().items.len() as i64) {
                let mut it: Rc<RefCell<MdGalleryItem>> = g.borrow().items[ai as usize].clone();
                let mut a: f64 = 0.0;
                if !it.borrow().isText {
                    let mut pic: Rc<RefCell<MdBox>> = self.galleryPicture(&it);
                    if pic.borrow().imageNaturalW > 0 && pic.borrow().imageNaturalH > 0 {
                        a = (pic.borrow().imageNaturalW as f64) / (pic.borrow().imageNaturalH as f64);
                    }
                    if !it.borrow().caption.is_empty() {
                        any_cap = true;
                    }
                }
                aspects.push(a);
                ai += 1;
            }
            let mut cap_h: f64 = 0.0;
            if any_cap && g.borrow().caption == "below" {
                cap_h = self.style.borrow()
                    .bodySize * self.style.borrow()
                    .lineHeight;
            }
            let full_r: f64 = self.contentBottom - self.contentTop;
            let mut room_r: f64 = self.contentBottom - self.cursorY;
            if room_r < full_r * 0.35 {
                room_r = full_r;
            }
            cols = g.borrow().columnsForRoom(look, &aspects, w, room_r, gap, cap_h, self.galleryMinRow);
        }
        let mut rows: Vec<Vec<i64>> = g.borrow().rows(cols);
        let n_rows: i64 = rows.len() as i64;
        let mut row_h: f64 = ((w - gap * ((cols - 1) as f64)) / (cols as f64)) * 0.66;
        if self.slides && self.paginate {
            let full_h: f64 = self.contentBottom - self.contentTop;
            let mut room: f64 = self.contentBottom - self.cursorY;
            if room < full_h * 0.35 {
                // too little under what is there: the gallery starts a slide
                room = full_h;
            }
            let mut least: f64 = self.galleryMinRow;
            if least > full_h * 0.45 {
                least = full_h * 0.45;
            }
            let per: i64 = MdGallery::rowsPerPage(room, gap, least, n_rows);
            row_h = (room - gap * ((per - 1) as f64)) / (per as f64);
            if row_h > full_h {
                row_h = full_h;
            }
        }
        let mut r: i64 = 0;
        while r < n_rows {
            let mut row: Vec<i64> = rows[r as usize].clone();
            let mut widths: Vec<f64> = g.borrow().rowWidths(&row, cols, w, gap);
            let y: f64 = self.reserve(row_h);
            let last: bool = r == n_rows - 1;
            if !look.borrow().background.is_empty() {
                let mut bg_h: f64 = row_h;
                if !last {
                    bg_h = row_h + gap;
                }
                self.emit(MdBox::rect(x, y, w, bg_h, look.borrow().background.clone()).clone());
            }
            let mut cx: f64 = x;
            let mut k: i64 = 0;
            while k < (row.len() as i64) {
                let idx: i64 = row[k as usize];
                let cw: f64 = widths[k as usize];
                self.galleryCell(g, idx, cx, y, cw, row_h);
                cx = (cx + cw) + gap;
                k += 1;
            }
            self.cursorY = y + row_h;
            if !last {
                self.cursorY += gap;
            }
            r += 1;
        }
        self.cursorY += self.style.borrow().paraSpacing;
    }
    /// One cell of a grid: its background, the picture (or a text cell's
    /// words), the caption and the border, in that order.
    pub fn galleryCell(&mut self, g: &Rc<RefCell<MdGallery>>, idx: i64, x: f64, y: f64, w: f64, h: f64) {
        let mut it: Rc<RefCell<MdGalleryItem>> = g.borrow().items[idx as usize].clone();
        let mut _tmp_3103 = self.sheet.clone();
        let mut cl: Rc<RefCell<MdCellLook>> = g.borrow().cellLook(&_tmp_3103, &it, &self.style.clone());
        let mut _tmp_3105 = self.sheet.clone();
        let mut cap: Rc<RefCell<MdCellLook>> = g.borrow().captionLook(&_tmp_3105, &it, &self.style.clone());
        let mut ground: String = cl.borrow().background.clone();
        if ground.is_empty() {
            let mut look: Rc<RefCell<MdGalleryLook>> = g.borrow().look(&self.sheet.clone());
            ground = look.borrow().background.clone();
        }
        self.readableCaption(&cap, g.borrow().caption == "overlay", &ground);
        let cap_text: String = it.borrow().caption.clone();
        let cap_w: f64 = (((w - cl.borrow().padL) - cl.borrow().padR) - cap.borrow().padL) - cap.borrow().padR;
        let mut cap_h: f64 = 0.0;
        if !it.borrow().isText && (!cap_text.is_empty() && g.borrow().caption != "none") {
            let cap_lines: Vec<Rc<RefCell<MdLine>>> = self.galleryLines(cap_text.clone(), &cap, cap_w);
            cap_h = self.linesHeight(&cap_lines);
        }
        let mut pic: Rc<RefCell<MdBox>> = self.galleryPicture(&it);
        let mut c: Rc<RefCell<MdGalleryCell>> = g.borrow().placeCell(&it, idx, x, y, w, h, &cl, &cap, cap_h, pic.borrow().imageNaturalW as f64, pic.borrow().imageNaturalH as f64);
        if !cl.borrow().background.is_empty() {
            let mut bg: Rc<RefCell<MdBox>> = MdBox::rect(x, y, w, h, cl.borrow().background.clone());
            bg.borrow_mut().radius = cl.borrow().radius;
            self.emit(bg.clone());
        }
        if it.borrow().isText {
            let mut _tmp_3110 = it.borrow().text.clone();
            let mut _tmp_3111 = c.borrow().capX.clone();
            let mut _tmp_3112 = c.borrow().capY.clone();
            let mut _tmp_3113 = c.borrow().capW.clone();
            let mut _tmp_3114 = c.borrow().capH.clone();
            self.galleryText(&_tmp_3110,
                &cl,
                _tmp_3111,
                _tmp_3112,
                _tmp_3113,
                _tmp_3114,
                true
            );
        } else {
            let mut rounded: f64 = 0.0;
            if ((cl.borrow().padT + cl.borrow().padR) + cl.borrow().padB) + cl.borrow().padL < 0.01 {
                rounded = cl.borrow().radius;
            }
            self.galleryPlace(pic.clone(), &c, rounded);
            if c.borrow().dim {
                let mut _tmp_3115 = self.dimColor(&cap);
                let arg_0 = MdBox::rect(
                    c.borrow().dimX,
                    c.borrow().dimY,
                    c.borrow().dimW,
                    c.borrow().dimH,
                    _tmp_3115
                ).clone();
                self.emit(arg_0);
            }
            if cap_h > 0.0 {
                let mut _tmp_3116 = c.borrow().capX.clone();
                let mut _tmp_3117 = c.borrow().capY.clone();
                let mut _tmp_3118 = c.borrow().capW.clone();
                let arg_0 = c.borrow().capH + 0.5;
                self.galleryText(&cap_text,
                    &cap,
                    _tmp_3116,
                    _tmp_3117,
                    _tmp_3118,
                    arg_0,
                    false
                );
            }
        }
        if cl.borrow().borderWidth > 0.0 && !cl.borrow().borderColor.is_empty() {
            let mut _tmp_3119 = cl.borrow().borderColor.clone();
            let mut o: Rc<RefCell<MdBox>> = MdBox::outline(x, y, w, h, _tmp_3119, cl.borrow().borderWidth.clone());
            o.borrow_mut().radius = cl.borrow().radius;
            self.emit(o.clone());
        }
    }
    /// `layout=full`: each picture on a slide of its own, over the whole
    /// page — and in print over the bleed past it — with no margins and no
    /// header or footer. The first goes on the slide the gallery is on when
    /// that slide holds nothing yet (a hidden title is nothing).
    /// A hidden title written just before a full-page gallery names the
    /// gallery's first slide, not the slide the break leaves behind: the
    /// titles noted on `from` after the last thing drawn there move to `to`.
    pub fn carryHidden(&mut self, from: i64, to: i64) {
        if self.hiddenOnPage != from {
            return;
        }
        let mut last: i64 = -1;
        let mut i: i64 = 0;
        while i < (self.boxes.len() as i64) {
            let mut b: Rc<RefCell<MdBox>> = self.boxes[i as usize].clone();
            if b.borrow().page == from {
                let mut s: i64 = b.borrow().srcStart;
                if b.borrow().segSrc >= 0 {
                    s = b.borrow().segSrc;
                }
                if s > last {
                    last = s;
                }
            }
            i += 1;
        }
        let mut h: i64 = 0;
        while h < (self.hiddenSrc.len() as i64) {
            if self.hiddenPage[h as usize] == from && self.hiddenSrc[h as usize] > last {
                self.hiddenPage[h as usize] = to;
            }
            h += 1;
        }
        self.hiddenOnPage = to;
        let n: i64 = self.headPage.len() as i64;
        if n > 0 && self.headPage[(n - 1) as usize] == from {
            self.headPage[(n - 1) as usize] = to;
        }
    }
    pub fn galleryFull(&mut self, g: &Rc<RefCell<MdGallery>>, look: &Rc<RefCell<MdGalleryLook>>) {
        let pw: f64 = self.style.borrow().pageWidth;
        let ph: f64 = self.style.borrow().pageHeight;
        let mut b: f64 = 0.0;
        if self.style.borrow().forPrint {
            b = self.style.borrow().bleed;
        }
        // the text keeps this far from the edge: half the page's margin,
        // and in print the safe area the printing house asked for
        let mut inset: f64 = self.style.borrow().margin * 0.5;
        if self.style.borrow()
            .forPrint && self.style.borrow()
            .safeArea > inset {
            inset = self.style.borrow().safeArea;
        }
        let mut k: i64 = 0;
        while k < (g.borrow().items.len() as i64) {
            if k > 0 || self.cursorY - self.contentTop > 0.5 {
                let from: i64 = self.page;
                self.nextSlide();
                if k == 0 {
                    self.carryHidden(from, self.page.clone());
                }
            }
            self.headerOff.push(self.page);
            self.footerOff.push(self.page);
            let mut it: Rc<RefCell<MdGalleryItem>> = g.borrow().items[k as usize].clone();
            let mut _tmp_3123 = self.page.clone();
            self.namePage(_tmp_3123, &self.fullPageName(&it, k));
            let mut _tmp_3125 = self.sheet.clone();
            let mut cl: Rc<RefCell<MdCellLook>> = g.borrow().cellLook(&_tmp_3125, &it, &self.style.clone());
            let mut _tmp_3127 = self.sheet.clone();
            let mut cap: Rc<RefCell<MdCellLook>> = g.borrow().captionLook(&_tmp_3127, &it, &self.style.clone());
            let mut ground: String = look.borrow().background.clone();
            if !cl.borrow().background.is_empty() {
                ground = cl.borrow().background.clone();
            }
            self.readableCaption(&cap, g.borrow().caption == "overlay", &ground);
            if !ground.is_empty() {
                let mut gb: Rc<RefCell<MdBox>> = MdBox::rect(
                    0.0 - b,
                    0.0 - b,
                    pw + b * 2.0,
                    ph + b * 2.0,
                    ground.clone()
                );
                gb.borrow_mut().bleeds = b > 0.0;
                self.emit(gb.clone());
            }
            if it.borrow().isText {
                let mut _tmp_3129 = it.borrow().text.clone();
                let arg_0 = inset + cl.borrow().padL;
                let arg_1 = inset + cl.borrow().padT;
                let arg_2 = ((pw - inset * 2.0) - cl.borrow().padL) - cl.borrow().padR;
                let arg_3 = ((ph - inset * 2.0) - cl.borrow().padT) - cl.borrow().padB;
                self.galleryText(&_tmp_3129,
                    &cl,
                    arg_0,
                    arg_1,
                    arg_2,
                    arg_3,
                    true
                );
            } else {
                let mut cap_text: String = it.borrow().caption.clone();
                if g.borrow().caption == "none" {
                    cap_text = String::new();
                }
                let cap_w: f64 = ((pw - inset * 2.0) - cap.borrow().padL) - cap.borrow().padR;
                let mut cap_h: f64 = 0.0;
                if !cap_text.is_empty() {
                    let cap_lines: Vec<Rc<RefCell<MdLine>>> = self.galleryLines(cap_text.clone(), &cap, cap_w);
                    cap_h = self.linesHeight(&cap_lines);
                }
                let mut band: f64 = 0.0;
                if cap_h > 0.0 {
                    band = ((cap_h + cap.borrow().padT) + cap.borrow().padB) + inset;
                    if band > ph * 0.4 {
                        band = ph * 0.4;
                    }
                }
                let mut pic: Rc<RefCell<MdBox>> = self.galleryPicture(&it);
                let mut c: Rc<RefCell<MdGalleryCell>> = Rc::new(RefCell::new(MdGalleryCell::new()));
                c.borrow_mut().item = k;
                // the picture's place: the page and the bleed round it, less
                // the band a caption below takes; inside a cell's padding
                let px: f64 = (0.0 - b) + cl.borrow().padL;
                let py: f64 = (0.0 - b) + cl.borrow().padT;
                let pwid: f64 = ((pw + b * 2.0) - cl.borrow().padL) - cl.borrow().padR;
                let mut phei: f64 = ((ph + b * 2.0) - cl.borrow().padT) - cl.borrow().padB;
                if g.borrow().caption == "below" && band > 0.0 {
                    phei = ((ph + b) - band) - cl.borrow().padT;
                }
                let mut _tmp_3130 = g.borrow().fit.clone();
                let mut _tmp_3131 = it.borrow().focusX.clone();
                let mut _tmp_3132 = it.borrow().focusY.clone();
                MdGallery::fitPicture(
                    &c,
                    &_tmp_3130,
                    _tmp_3131,
                    _tmp_3132,
                    pic.borrow().imageNaturalW as f64,
                    pic.borrow().imageNaturalH as f64,
                    px,
                    py,
                    pwid,
                    phei
                );
                self.galleryPlace(pic.clone(), &c, cl.borrow().radius.clone());
                // it is cut at the page's edge only where it reaches it
                let last: i64 = (self.boxes.len() as i64) - 1;
                if (b > 0.0 && last >= 0) && !pic.borrow().imagePath.is_empty() {
                    let mut lb: Rc<RefCell<MdBox>> = self.boxes[last as usize].clone();
                    lb.borrow_mut().bleeds = true;
                }
                if cap_h > 0.0 {
                    let cy: f64 = ((ph - inset) - cap.borrow().padB) - cap_h;
                    if g.borrow().caption == "overlay" {
                        let dy: f64 = (cy - cap.borrow().padT) - inset * 0.5;
                        let mut _tmp_3134 = self.dimColor(&cap);
                        let mut dim: Rc<RefCell<MdBox>> = MdBox::rect(
                            0.0 - b,
                            dy,
                            pw + b * 2.0,
                            (ph - dy) + b,
                            _tmp_3134
                        );
                        dim.borrow_mut().bleeds = b > 0.0;
                        self.emit(dim.clone());
                    }
                    let arg_0 = inset + cap.borrow().padL;
                    self.galleryText(&cap_text,
                        &cap,
                        arg_0,
                        cy,
                        cap_w,
                        cap_h + 0.5,
                        false
                    );
                }
            }
            k += 1;
        }
        self.cursorY = self.contentBottom;
        self.fullPageDone = true;
    }
    /// The picture an item names, as a box that knows its file and size;
    /// `imagePath` is "" when the store has no such picture.
    pub fn galleryPicture(&mut self, it: &Rc<RefCell<MdGalleryItem>>) -> Rc<RefCell<MdBox>> {
        let mut b: Rc<RefCell<MdBox>> = Rc::new(RefCell::new(MdBox::new()));
        b.borrow_mut().kind = 4;
        b.borrow_mut().imageAlt = it.borrow().caption.clone();
        if b.borrow().imageAlt.is_empty() {
            b.borrow_mut().imageAlt = it.borrow().src.clone();
        }
        if false == self.haveImages {
            self.missed(&format!("gallery:{}", it.borrow().src).clone());
            return b;
        }
        let mut _tmp_3139 = it.borrow().src.clone();
        let path: String = Vfs::resolve(&self.images, &_tmp_3139, &self.docDir.clone());
        if path.is_empty() {
            self.missed(&format!("gallery:{}", it.borrow().src).clone());
            return b;
        }
        let mut st: Rc<RefCell<VfsStat>> = Vfs::stat(&self.images, &path);
        if false == st.borrow().found {
            self.missed(&format!("gallery:{}", it.borrow().src).clone());
            return b;
        }
        b.borrow_mut().imagePath = path.clone();
        b.borrow_mut().imageNaturalW = st.borrow().pixelW;
        b.borrow_mut().imageNaturalH = st.borrow().pixelH;
        b
    }
    /// The picture where the cell says, or its name in brackets in that
    /// place when there are no bytes for it.
    pub fn galleryPlace(&mut self, pic: Rc<RefCell<MdBox>>, c: &Rc<RefCell<MdGalleryCell>>, radius: f64) {
        if pic.borrow().imagePath.is_empty() {
            let mut lk: Rc<RefCell<MdCellLook>> = Rc::new(RefCell::new(MdCellLook::new()));
            lk.borrow_mut().fontSize = self.style.borrow().bodySize * 0.8;
            lk.borrow_mut().family = self.style.borrow().bodyFamily.clone();
            lk.borrow_mut().color = self.style.borrow().mutedColor.clone();
            lk.borrow_mut().align = "center".to_string();
            let arg_0 = MdBox::outline(
                c.borrow().picX,
                c.borrow().picY,
                c.borrow().picW,
                c.borrow().picH,
                self.style.borrow().ruleColor.clone(),
                1.0
            ).clone();
            self.emit(arg_0);
            let first: i64 = self.boxes.len() as i64;
            let mut _tmp_3147 = c.borrow().picX.clone();
            let mut _tmp_3148 = c.borrow().picY.clone();
            let mut _tmp_3149 = c.borrow().picW.clone();
            let mut _tmp_3150 = c.borrow().picH.clone();
            self.galleryText(&format!("[{}]", pic.borrow().imageAlt).clone(),
                &lk,
                _tmp_3147,
                _tmp_3148,
                _tmp_3149,
                _tmp_3150,
                true
            );
            let mut q: i64 = first;
            while q < (self.boxes.len() as i64) {
                let mut qb: Rc<RefCell<MdBox>> = self.boxes[q as usize].clone();
                qb.borrow_mut().quiet = true;
                q += 1;
            }
            return;
        }
        pic.borrow_mut().x = c.borrow().picX;
        pic.borrow_mut().y = c.borrow().picY;
        pic.borrow_mut().w = c.borrow().picW;
        pic.borrow_mut().h = c.borrow().picH;
        pic.borrow_mut().hasCrop = c.borrow().cropped;
        pic.borrow_mut().cropX = c.borrow().cropX;
        pic.borrow_mut().cropY = c.borrow().cropY;
        pic.borrow_mut().cropW = c.borrow().cropW;
        pic.borrow_mut().cropH = c.borrow().cropH;
        pic.borrow_mut().radius = radius;
        self.emit(pic.clone());
    }
    /// The band under an overlay caption: the caption's own background when
    /// the sheet gives one, else the page's colour at 55 %, as `bg-dim`
    /// lays the page over a slide's picture.
    /// A caption that reads (4.5:1). On a picture (`overlay`) the band under
    /// it is made just dark (or light) enough for its ink whatever the
    /// picture is, from the paper at 55 % up to 90 %; below a picture the
    /// ink must read on `ground`. A colour from the stylesheet is kept.
    pub fn readableCaption(&self, cap: &Rc<RefCell<MdCellLook>>, overlay: bool, ground: &str) {
        let mut paper: String = self.style.borrow().pageBackground.clone();
        if paper.is_empty() || paper == "transparent" {
            paper = "#ffffff".to_string();
        }
        if overlay {
            // a photo's caption: a dark see-through band with white text,
            // which reads over light and dark pictures alike. A caption the
            // sheet coloured dark gets a light band instead.
            if !cap.borrow().colorSet {
                cap.borrow_mut().color = "#FFFFFF".to_string();
            }
            if cap.borrow().background.is_empty() {
                let mut shade: i64 = 0;
                if MdContainer::luminance(&cap.borrow().color.clone()) <= 0.4 {
                    shade = 255;
                }
                let mut a: f64 = 0.45;
                let mut fill: String = String::new();
                while a < 0.901 && fill.is_empty() {
                    let try_fill: String = MdContainer::hexOf(&EVGColor::rgba(shade, shade, shade, a));
                    if MdContainer::worstContrast(&cap.borrow().color.clone(), &try_fill) >= 4.5 {
                        fill = try_fill.clone();
                    }
                    a += 0.05;
                }
                if fill.is_empty() {
                    fill = MdContainer::hexOf(&EVGColor::rgba(shade, shade, shade, 0.9));
                }
                cap.borrow_mut().background = fill.clone();
            }
            if !cap.borrow().colorSet {
                let mut _tmp_3162 = cap.borrow().background.clone();
                let ink: String = MdContainer::inkOn(&_tmp_3162, &cap.borrow().color.clone());
                if !ink.is_empty() {
                    cap.borrow_mut().color = ink.clone();
                }
            }
            return;
        }
        if cap.borrow().colorSet {
            return;
        }
        let mut under: String = ground.to_string();
        if under.is_empty() {
            under = paper.clone();
        }
        if MdContainer::contrast(&cap.borrow().color.clone(), &under) >= 4.5 {
            return;
        }
        if MdContainer::contrast(&self.style.borrow().textColor.clone(), &under) >= 4.5 {
            cap.borrow_mut().color = self.style.borrow().textColor.clone();
            return;
        }
        let ink2: String = MdContainer::inkOn(&under, &cap.borrow().color.clone());
        if !ink2.is_empty() {
            cap.borrow_mut().color = ink2.clone();
        }
    }
    pub fn dimColor(&self, cap: &Rc<RefCell<MdCellLook>>) -> String {
        if !cap.borrow().background.is_empty() {
            return cap.borrow().background.clone();
        }
        let mut paper: String = self.style.borrow().pageBackground.clone();
        if paper.is_empty() || paper == "transparent" {
            paper = "#ffffff".to_string();
        }
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(&paper);
        let hex: String = EVGColor::to_hex_string(&c);
        format!("{}8C", hex)
    }
    pub fn galleryLines(&mut self, text: String, lk: &Rc<RefCell<MdCellLook>>, w: f64) -> Vec<Rc<RefCell<MdLine>>> {
        let mut r: Rc<RefCell<MdRun>> = Rc::new(RefCell::new(MdRun::new()));
        r.borrow_mut().text = text.clone();
        r.borrow_mut().family = lk.borrow().family.clone();
        if r.borrow().family.is_empty() {
            r.borrow_mut().family = self.style.borrow().bodyFamily.clone();
        }
        r.borrow_mut().size = lk.borrow().fontSize;
        r.borrow_mut().bold = lk.borrow().bold;
        r.borrow_mut().color = lk.borrow().color.clone();
        let mut runs: Vec<Rc<RefCell<MdRun>>> = Vec::new();
        runs.push(r.clone());
        let mut width: f64 = w;
        if width < 8.0 {
            width = 8.0;
        }
        self.breakRuns(&runs, width)
    }
    /// Words set in `x, y, w, h` in the look's type and alignment, as many
    /// lines as fit; `middle`: centred down the box (a text cell).
    pub fn galleryText(&mut self, text: &str, lk: &Rc<RefCell<MdCellLook>>, x: f64, y: f64, w: f64, h: f64, middle: bool) {
        let mut lines: Vec<Rc<RefCell<MdLine>>> = self.galleryLines(text.to_string(),
            lk,
            w
        );
        let total: f64 = self.linesHeight(&lines);
        let mut ty: f64 = y;
        if middle && total < h {
            ty = y + (h - total) * 0.5;
        }
        let first: i64 = self.boxes.len() as i64;
        let mut i: i64 = 0;
        while i < (lines.len() as i64) {
            let mut ln: Rc<RefCell<MdLine>> = lines[i as usize].clone();
            if (ty + ln.borrow().height) - y > h + 0.5 {
                // what does not fit is left out rather than drawn over the
                // next cell
                i = lines.len() as i64;
            } else {
                let mut dx: f64 = 0.0;
                if lk.borrow().align == "center" {
                    dx = (w - ln.borrow().width) * 0.5;
                }
                if lk.borrow().align == "end" {
                    dx = w - ln.borrow().width;
                }
                if dx < 0.0 {
                    dx = 0.0;
                }
                self.emitLine(&ln, x + dx, ty);
                ty += ln.borrow().height;
                i += 1;
            }
        }
        if lk.borrow().align != "start" {
            let mut word: String = "center";
            if lk.borrow().align == "end" {
                word = "right";
            }
            let mut k: i64 = first;
            while k < (self.boxes.len() as i64) {
                let mut b: Rc<RefCell<MdBox>> = self.boxes[k as usize].clone();
                if b.borrow().kind == 0 {
                    b.borrow_mut().align = word.to_string();
                    b.borrow_mut().alignX = x;
                    b.borrow_mut().alignW = w;
                }
                k += 1;
            }
        }
    }
    /// How wide each column is, in the space the table has.
    ///
    /// Split out of `table` so that the DECK can ask it: `MdToPptx` builds a
    /// real PowerPoint table and has to give it the same grid, and a second
    /// implementation of this would be a second table algorithm to keep in
    /// step with the first. One algorithm, two callers.
    pub fn tableWidths(&mut self, node: &Rc<RefCell<MdNode>>, w: f64) -> Vec<f64> {
        let was: f64 = self.tableTextOn();
        let mut out: Vec<f64> = self.gridWidths(node, w);
        self.style.borrow_mut().bodySize = was;
        out
    }
    /// A table's text is set at `table { font-size }` when the sheet gives
    /// one: the body's size is that while the table is measured and drawn,
    /// and what it was is returned to be put back.
    pub fn tableTextOn(&self) -> f64 {
        let was: f64 = self.style.borrow().bodySize;
        if self.style.borrow().tableSize > 0.0 {
            let bodySize_v = self.style.borrow().tableSize;
            self.style.borrow_mut().bodySize = bodySize_v;
        }
        was
    }
    pub fn gridWidths(&mut self, node: &Rc<RefCell<MdNode>>, w: f64) -> Vec<f64> {
        let mut widths: Vec<f64> = Vec::new();
        let rows: i64 = node.borrow().childCount();
        if rows == 0 {
            return widths;
        }
        let mut head: Rc<RefCell<MdNode>> = node.borrow().childAt(0);
        let cols: i64 = head.borrow().childCount();
        if cols == 0 {
            return widths;
        }
        // TWO widths per column, not one: what the cell wants if nothing
        // wrapped (MAX-CONTENT) and what it needs before a word has to be
        // broken in half (MIN-CONTENT).
        //
        // Scaling the max-content widths by one factor — which is what this
        // did — is only right when every column shrinks equally well, and a
        // column of one-word labels beside a column of paragraphs is the case
        // where it is not. `Area | Current expectation` in this repository's
        // own README came out with a first column narrower than the word
        // `Host/runtime`, which then ran out of its cell and across the text
        // in the next one: unreadable, and unreadable in a way a wider table
        // would not have been.
        //
        // So: no column below its min-content width, and the room that is left
        // shared out in proportion to how much each column would still like.
        // That is the table algorithm HTML has had since HTML 4 §17.5.2, and
        // it is what a browser does with this same markdown.
        let mut natural: Vec<f64> = Vec::new();
        let mut minimum: Vec<f64> = Vec::new();
        let mut tones: Rc<RefCell<MdCellTones>> = MdCellTones::forTable(node, &self.style.borrow().tableCellTones.clone());
        let mut c: i64 = 0;
        while c < cols {
            let mut wid: f64 = 0.0;
            let mut least: f64 = 0.0;
            let mut r: i64 = 0;
            while r < rows {
                let mut row: Rc<RefCell<MdNode>> = node.borrow().childAt(r);
                let rc: i64 = row.borrow().childCount();
                if c < rc {
                    let mut cell: Rc<RefCell<MdNode>> = row.borrow().childAt(c);
                    let mut runs: Vec<Rc<RefCell<MdRun>>> = self.tableCellRuns(&cell,
                        &tones
                    );
                    let cw: f64 = self.runsWidth(&runs);
                    if cw > wid {
                        wid = cw;
                    }
                    // The narrowest column this cell can be set in, asked of
                    // the line breaker itself rather than guessed from the
                    // text: break it into a column of one point and the widest
                    // line that comes back is the widest thing that cannot be
                    // broken. Guessing it from spaces would disagree with the
                    // breaker exactly where it matters — `ts_parser` is three
                    // runs and one word.
                    let mut tight: Vec<Rc<RefCell<MdLine>>> = self.breakRuns(&runs, 1.0);
                    let mut ti: i64 = 0;
                    let tn: i64 = tight.len() as i64;
                    while ti < tn {
                        let mut tl: Rc<RefCell<MdLine>> = tight[ti as usize].clone();
                        if tl.borrow().width > least {
                            least = tl.borrow().width;
                        }
                        ti += 1;
                    }
                }
                r += 1;
            }
            natural.push(wid + self.style.borrow().cellPadX * 2.0);
            minimum.push(least + self.style.borrow().cellPadX * 2.0);
            c += 1;
        }
        let mut total: f64 = 0.0;
        let mut least_total: f64 = 0.0;
        let mut k: i64 = 0;
        while k < cols {
            total += natural[k as usize];
            least_total += minimum[k as usize];
            k += 1;
        }
        let mut j: i64 = 0;
        if total <= w {
            // It fits as written. On a slide it spans its column (or the
            // `width` it was given), the room left shared out as each column
            // is wide: a five-column table of short cells filled the left
            // half of the slide and left the right half empty.
            let mut extra: f64 = 0.0;
            if self.slides && total > 0.0 {
                extra = w - total;
            }
            while j < cols {
                let nat: f64 = natural[j as usize];
                widths.push(nat + extra * (nat / total));
                j += 1;
            }
        } else {
            if least_total >= w {
                // Even unbreakable, it does not fit. The table is then wider
                // than the column it sits in, which is visible and honest;
                // squeezing below this point does not make it fit, it only
                // moves the overflow inside the cells where it lands on top of
                // the neighbouring text.
                while j < cols {
                    widths.push(minimum[j as usize]);
                    j += 1;
                }
            } else {
                let room: f64 = w - least_total;
                let want: f64 = total - least_total;
                while j < cols {
                    let lo: f64 = minimum[j as usize];
                    let hi: f64 = natural[j as usize];
                    let share: f64 = ((hi - lo) / want) * room;
                    widths.push(lo + share);
                    j += 1;
                }
            }
        }
        widths
    }
    pub fn runsWidth(&mut self, runs: &Vec<Rc<RefCell<MdRun>>>) -> f64 {
        let mut total: f64 = 0.0;
        let mut i: i64 = 0;
        let n: i64 = runs.len() as i64;
        while i < n {
            let mut r: Rc<RefCell<MdRun>> = runs[i as usize].clone();
            total += self.measureRun(&r.borrow().text, &r);
            i += 1;
        }
        total
    }
    /// A table cell's text as runs: bold in the header row, and bold in its
    /// tone's text colour where the table's mapping names what it says.
    /// Measuring and drawing ask this one function, so a cell is never
    /// measured in one face and drawn in another.
    pub fn tableCellRuns(&mut self, cell: &Rc<RefCell<MdNode>>, tones: &Rc<RefCell<MdCellTones>>) -> Vec<Rc<RefCell<MdRun>>> {
        let mut base: Rc<RefCell<MdRun>> = self.baseRun();
        if cell.borrow().isHeader {
            base.borrow_mut().bold = true;
            if !self.style.borrow().tableHeadColor.is_empty() {
                base.borrow_mut().color = self.style.borrow().tableHeadColor.clone();
            }
        }
        let tone: String = tones.borrow().toneOfCell(cell);
        if !tone.is_empty() {
            base.borrow_mut().bold = true;
            let _tmp_3177 = MdCellTones::inkOf(&tone, &self.tablePaper());
            base.borrow_mut().color = _tmp_3177;
        }
        self.inlineRuns(cell, &base)
    }
    /// the colour a table's tinted cells are worked out against
    pub fn tablePaper(&self) -> String {
        let mut _tmp_3178 = self.style.borrow().pageBackground.clone();
        MdCellTones::paperOf(&_tmp_3178, &self.style.borrow().textColor.clone())
    }
    pub fn tableRowHeight(&mut self, row: &Rc<RefCell<MdNode>>, widths: &[f64], cols: i64, tones: &Rc<RefCell<MdCellTones>>) -> f64 {
        let mut row_h: f64 = 0.0;
        let mut c: i64 = 0;
        let rc: i64 = row.borrow().childCount();
        while c < cols {
            let mut h: f64 = self.style.borrow()
                .bodySize * self.style.borrow()
                .lineHeight + self.style.borrow()
                .cellPadY * 2.0;
            if c < rc {
                let mut cell: Rc<RefCell<MdNode>> = row.borrow().childAt(c);
                let mut runs: Vec<Rc<RefCell<MdRun>>> = self.tableCellRuns(&cell, tones);
                let inner: f64 = widths[c as usize] - self.style.borrow()
                    .cellPadX * 2.0;
                let mut lines: Vec<Rc<RefCell<MdLine>>> = self.breakRuns(&runs, inner);
                h = self.linesHeight(&lines) + self.style.borrow()
                    .cellPadY * 2.0;
            }
            if h > row_h {
                row_h = h;
            }
            c += 1;
        }
        row_h
    }
    pub fn tableRow(&mut self, row: &Rc<RefCell<MdNode>>, x: f64, widths: &[f64], cols: i64, tones: &Rc<RefCell<MdCellTones>>) {
        // Every cell is broken to its own column first, so the row is as tall
        // as the tallest of them.
        let mut heights: Vec<f64> = Vec::new();
        let mut row_h: f64 = 0.0;
        let mut c: i64 = 0;
        let rc: i64 = row.borrow().childCount();
        while c < cols {
            let mut h: f64 = self.style.borrow()
                .bodySize * self.style.borrow()
                .lineHeight + self.style.borrow()
                .cellPadY * 2.0;
            if c < rc {
                let mut cell: Rc<RefCell<MdNode>> = row.borrow().childAt(c);
                let mut runs: Vec<Rc<RefCell<MdRun>>> = self.tableCellRuns(&cell, tones);
                let inner: f64 = widths[c as usize] - self.style.borrow()
                    .cellPadX * 2.0;
                let mut lines: Vec<Rc<RefCell<MdLine>>> = self.breakRuns(&runs, inner);
                let mut sum: f64 = 0.0;
                let mut i: i64 = 0;
                let n: i64 = lines.len() as i64;
                while i < n {
                    let mut ln: Rc<RefCell<MdLine>> = lines[i as usize].clone();
                    sum += ln.borrow().height;
                    i += 1;
                }
                h = sum + self.style.borrow().cellPadY * 2.0;
            }
            heights.push(h);
            if h > row_h {
                row_h = h;
            }
            c += 1;
        }
        let y: f64 = self.reserve(row_h);
        let mut cx: f64 = x;
        let mut c2: i64 = 0;
        while c2 < cols {
            let cw: f64 = widths[c2 as usize];
            let mut is_head: bool = false;
            let mut tone: String = String::new();
            if c2 < rc {
                let mut cell0: Rc<RefCell<MdNode>> = row.borrow().childAt(c2);
                is_head = cell0.borrow().isHeader;
                tone = tones.borrow().toneOfCell(&cell0);
            }
            if is_head {
                let arg_0 = MdBox::rect(
                    cx,
                    y,
                    cw,
                    row_h,
                    self.style.borrow().tableHeadBackground.clone()
                ).clone();
                self.emit(arg_0);
            }
            let toned: bool = !tone.is_empty();
            if toned {
                let mut _tmp_3180 = self.tablePaper();
                let arg_0 = MdBox::rect(
                    cx,
                    y,
                    cw,
                    row_h,
                    MdCellTones::fillOf(&tone, &_tmp_3180)
                ).clone();
                self.emit(arg_0);
            }
            let arg_0 = MdBox::outline(
                cx,
                y,
                cw,
                row_h,
                self.style.borrow().tableBorder.clone(),
                1.0
            ).clone();
            self.emit(arg_0);
            if c2 < rc {
                let mut cell_1: Rc<RefCell<MdNode>> = row.borrow().childAt(c2);
                let mut runs_1: Vec<Rc<RefCell<MdRun>>> = self.tableCellRuns(&cell_1,
                    tones
                );
                let inner_1: f64 = cw - self.style.borrow().cellPadX * 2.0;
                let mut lines_1: Vec<Rc<RefCell<MdLine>>> = self.breakRuns(&runs_1,
                    inner_1
                );
                let mut ty: f64 = y + self.style.borrow().cellPadY;
                // a toned cell is a value, set in the middle of its cell
                if toned {
                    ty = y + (row_h - self.linesHeight(&lines_1)) / 2.0;
                }
                let mut i_1: i64 = 0;
                let n_1: i64 = lines_1.len() as i64;
                while i_1 < n_1 {
                    let mut ln_1: Rc<RefCell<MdLine>> = lines_1[i_1 as usize].clone();
                    let mut lx: f64 = cx + self.style.borrow().cellPadX;
                    if cell_1.borrow().align == "right" {
                        lx = ((cx + cw) - self.style.borrow().cellPadX) - ln_1.borrow().width;
                    }
                    if cell_1.borrow().align == "center" || toned {
                        lx = (cx + self.style.borrow().cellPadX) + (inner_1 - ln_1.borrow().width) / 2.0;
                    }
                    self.emitLine(&ln_1, lx, ty);
                    ty += ln_1.borrow().height;
                    i_1 += 1;
                }
            }
            cx += cw;
            c2 += 1;
        }
        self.cursorY = y + row_h;
    }
}
