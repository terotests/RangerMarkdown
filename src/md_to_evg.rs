// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_embed::MdEmbedEntry;
use crate::md_layout::MdBox;
use crate::md_layout::MdLayout;
use crate::rgr::MdMath;
use crate::rgr::{EVGBox, EVGColor, EVGDisplayList, EVGDrawCmd, EVGElement, EVGLayout, EVGUnit};
use crate::rgr::{Vfs, VfsStat};

/// ============================================================================
/// MdToEvg.rgr — laid-out boxes into the one tree every painter already reads.
/// ============================================================================
///
/// `MdLayout` decided where everything goes. This turns that into `EVGElement`s
/// and then does nothing else: the PDF writer, the WebGL painter, the SVG and
/// HTML exporters and the SDL host all take it from here, and none of them has
/// to learn what a markdown document is.
///
///   [MdBox] ──► EVGElement ──► EVGPDFRenderer   → PDF, fonts embedded
///                          ──► EVGHTMLRenderer  → HTML
///                          ──► EVGDisplayList   → WebGL 2 / OpenGL
///
/// Everything is ABSOLUTELY POSITIONED, which is the same shape
/// `FlowScene.toEvgTree` hands the exporters for a diagram. That is deliberate:
/// the layout above is the flow engine, so asking EVG to flow it again would be
/// two answers to one question, and the PDF and the canvas would each get a
/// different one. EVG is being used here as a painter, not as a layout engine.
///
/// Two shapes come out:
///
///   buildPage    one `<div>` as tall as the document — what the canvas scrolls
///   buildPrint   `<print>` / `<section>` / `<page>` — what the PDF writer
///                already understands as a multi-page document
/// ============================================================================
pub struct MdToEvg {
    /// Colours, parsed once. A document is a few colours repeated a few
    /// thousand times, and `EVGColor.parse` is a scan of a string.
    pub colorKeys: Vec<String>,
    colorVals: Vec<Rc<RefCell<EVGColor>>>,
}

impl MdToEvg {
    pub fn new() -> Self {
        Self {
            colorKeys: Vec::new(),
            colorVals: Vec::new(),
        }
    }
    /// ----------------------------------------------------------------------
    /// Straight to the draw commands
    /// ----------------------------------------------------------------------
    ///
    /// The canvas does not need an element tree. It needs a list of marks, and
    /// every box this module produced is already one: a rectangle, an outline
    /// or a run of text, in page coordinates, with its face and its colour
    /// decided. Building three thousand `EVGElement`s in order to walk them
    /// back into three thousand `EVGDrawCmd`s is a round trip through a shape
    /// neither end wanted — `markdown:bench` put it at 133 ms of a 342 ms
    /// document, more than the parse.
    ///
    /// The PDF still takes the tree: `EVGPDFRenderer` reads elements, and it is
    /// the tree that carries the `<print>` / `<page>` structure it paginates by.
    /// So there are two roads out of the same boxes, and the checked claim is
    /// that they draw the same thing — `MarkdownTest.presized` compares them
    /// command by command.
    ///
    /// A DIAGRAM is the exception in both directions: its geometry lives in a
    /// `FlowScene`'s own element subtree, so that one goes through the engine
    /// and its commands are appended where the slot is.
    fn toDisplayList(layout: &Rc<RefCell<MdLayout>>, width: f64) -> Rc<RefCell<EVGDisplayList>> {
        let mut b: Rc<RefCell<MdToEvg>> = Rc::new(RefCell::new(MdToEvg::new()));
        return b.borrow_mut().paint(layout, width, 0.0, layout.borrow().documentHeight.clone(), false);
    }
    fn sheetsToDisplayList(layout: &Rc<RefCell<MdLayout>>, gap: f64) -> Rc<RefCell<EVGDisplayList>> {
        let mut b: Rc<RefCell<MdToEvg>> = Rc::new(RefCell::new(MdToEvg::new()));
        let ph: f64 = layout.borrow().style.borrow().pageHeight;
        let n: f64 = layout.borrow().pageCount as f64;
        // …and one gap above the first sheet, so the top of the document is a
        // margin rather than a cut.
        let total: f64 = n * (ph + gap) + gap;
        return b.borrow_mut().paint(layout, layout.borrow().style.borrow().pageWidth.clone(), gap, total, true);
    }
    /// ----------------------------------------------------------------------
    /// One page, as tall as the document. The screen's shape.
    /// ----------------------------------------------------------------------
    fn buildPage(layout: &Rc<RefCell<MdLayout>>, width: f64) -> Rc<RefCell<EVGElement>> {
        let mut root: Rc<RefCell<EVGElement>> = EVGElement::create_div();
        root.borrow_mut().width = Some(EVGUnit::px(width));
        let _tmp_3873 = EVGUnit::px(layout.borrow().documentHeight.clone());
        root.borrow_mut().height = Some(_tmp_3873);
        root.borrow_mut().page_width = width;
        root.borrow_mut().page_height = layout.borrow().documentHeight;
        root.borrow_mut().position = "relative".to_string();
        root.borrow_mut().background_color = Some(EVGColor::parse("#ffffff"));
        MdToEvg::put(&root, 0.0, 0.0, width, layout.borrow().documentHeight.clone());
        root.borrow_mut().position = "relative".to_string();
        MdToEvg::fill(&root, layout, 0, 0.0);
        root
    }
    /// ----------------------------------------------------------------------
    /// Sheets, stacked. What a reader scrolls through before printing.
    /// ----------------------------------------------------------------------
    ///
    /// The same pages the PDF gets, one under the other with a gap, so the
    /// breaks a reader sees on screen are the breaks the printer makes. One
    /// tree rather than one per page: the canvas and the HTML preview both want
    /// a single thing to lay out, and a sheet is an absolutely placed box like
    /// any other.
    fn buildSheets(layout: &Rc<RefCell<MdLayout>>, gap: f64) -> Rc<RefCell<EVGElement>> {
        let pw: f64 = layout.borrow().style.borrow().pageWidth;
        let ph: f64 = layout.borrow().style.borrow().pageHeight;
        let n: i64 = layout.borrow().pageCount;
        let total: f64 = (n as f64) * (ph + gap) - gap;
        let mut root: Rc<RefCell<EVGElement>> = EVGElement::create_div();
        root.borrow_mut().width = Some(EVGUnit::px(pw));
        root.borrow_mut().height = Some(EVGUnit::px(total));
        root.borrow_mut().page_width = pw;
        root.borrow_mut().page_height = total;
        root.borrow_mut().position = "relative".to_string();
        root.borrow_mut().background_color = Some(EVGColor::parse("#eef0f4"));
        MdToEvg::put(&root, 0.0, 0.0, pw, total);
        root.borrow_mut().position = "relative".to_string();
        let mut p: i64 = 0;
        while p < n {
            let dy: f64 = (p as f64) * (ph + gap);
            // The paper is a rectangle in the same tree as everything else,
            // and the page's boxes are offset onto it rather than nested
            // inside it. Nesting reads better and costs the diagrams: a
            // scene's paths are placed by the box their container gives them,
            // and a container one level deeper does not give them one.
            let mut paper: Rc<RefCell<EVGElement>> = EVGElement::create_div();
            MdToEvg::put(&paper, 0.0, dy, pw, ph);
            paper.borrow_mut().background_color = Some(EVGColor::parse("#ffffff"));
            EVGElement::add_child(&root, paper.clone());
            MdToEvg::fill(&root, layout, p, dy);
            p += 1;
        }
        root
    }
    pub fn sheetGap() -> f64 {
        24.0
    }
    /// Does this page still need `EVGLayout` run over it?
    ///
    /// Only for the diagrams. Their subtrees come from `FlowScene` and are
    /// sized by the engine — a scene's paths are placed by the box their
    /// container gives them and have no left or top of their own. Everything
    /// else on the page arrived pre-sized above.
    pub fn needsEngine(layout: &Rc<RefCell<MdLayout>>) -> bool {
        let mut i: i64 = 0;
        let n: i64 = layout.borrow().boxes.len() as i64;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = layout.borrow().boxes[i as usize].clone();
            if b.borrow().kind == 3 {
                return true;
            }
            if b.borrow().kind == 5 {
                return true;
            }
            i += 1;
        }
        false
    }
    /// ----------------------------------------------------------------------
    /// Sheets, separately. What is printed.
    /// ----------------------------------------------------------------------
    fn buildPrint(layout: &Rc<RefCell<MdLayout>>) -> Rc<RefCell<EVGElement>> {
        let mut root: Rc<RefCell<EVGElement>> = EVGElement::create_div();
        root.borrow_mut().tag_name = "print".to_string();
        let mut section: Rc<RefCell<EVGElement>> = EVGElement::create_div();
        section.borrow_mut().tag_name = "section".to_string();
        let _tmp_3882 = EVGUnit::px(layout.borrow().style.borrow().pageWidth.clone());
        section.borrow_mut().width = Some(_tmp_3882);
        let _tmp_3884 = EVGUnit::px(layout.borrow().style.borrow().pageHeight.clone());
        section.borrow_mut().height = Some(_tmp_3884);
        // The margin is already in every coordinate this layout produced, so
        // the section's own must be zero — stated, not defaulted, because the
        // renderer's default is 40 and every box would move by it.
        let sbox: Rc<RefCell<EVGBox>> = section.borrow().r#box.clone().unwrap();
        sbox.borrow_mut().margin_top = Some(EVGUnit::px(0.0));
        sbox.borrow_mut().margin_right = Some(EVGUnit::px(0.0));
        sbox.borrow_mut().margin_bottom = Some(EVGUnit::px(0.0));
        sbox.borrow_mut().margin_left = Some(EVGUnit::px(0.0));
        EVGElement::add_child(&root, section.clone());
        let mut p: i64 = 0;
        while p < layout.borrow().pageCount {
            let mut page: Rc<RefCell<EVGElement>> = EVGElement::create_div();
            page.borrow_mut().tag_name = "page".to_string();
            let _tmp_3890 = EVGUnit::px(layout.borrow().style.borrow().pageWidth.clone());
            page.borrow_mut().width = Some(_tmp_3890);
            let _tmp_3892 = EVGUnit::px(layout.borrow().style.borrow().pageHeight.clone());
            page.borrow_mut().height = Some(_tmp_3892);
            page.borrow_mut().page_width = layout.borrow().style.borrow().pageWidth;
            page.borrow_mut().page_height = layout.borrow().style.borrow().pageHeight;
            page.borrow_mut().position = "relative".to_string();
            page.borrow_mut().background_color = Some(EVGColor::parse("#ffffff"));
            MdToEvg::fill(&page, layout, p, 0.0);
            EVGElement::add_child(&section, page.clone());
            p += 1;
        }
        root
    }
    /// ----------------------------------------------------------------------
    /// The boxes of one page
    /// ----------------------------------------------------------------------
    fn fill(parent: &Rc<RefCell<EVGElement>>, layout: &Rc<RefCell<MdLayout>>, page: i64, y_offset: f64) {
        let mut i: i64 = 0;
        let n: i64 = layout.borrow().boxes.len() as i64;
        while i < n {
            let mut b: Rc<RefCell<MdBox>> = layout.borrow().boxes[i as usize].clone();
            if b.borrow().page == page {
                if b.borrow().kind == 3 {
                    MdToEvg::fillEmbed(parent, layout, &b, y_offset);
                } else {
                    let mut el: Rc<RefCell<EVGElement>> = MdToEvg::element(&b, y_offset);
                    EVGElement::add_child(&parent, el.clone());
                    if b.borrow().kind == 1 && b.borrow().tail > 0.0 {
                        EVGElement::add_child(&parent, MdToEvg::tailElement(&b, y_offset));
                    }
                }
            }
            i += 1;
        }
    }
    /// A prepared diagram, placed on the page.
    ///
    /// The scene's ROOT is kept, not unwrapped. It is tempting to flatten the
    /// diagram's elements into the page with their coordinates shifted, and it
    /// silently loses every edge: a scene's paths carry their geometry in an
    /// `svgPath` and a `viewBox` and are placed by the BOX their container
    /// gives them, not by a left and a top of their own. Lifted out of that
    /// container they have no box, and a painter draws them 24 pixels wide in
    /// the corner. So the container moves, and everything inside it stays put.
    fn fillEmbed(parent: &Rc<RefCell<EVGElement>>, layout: &Rc<RefCell<MdLayout>>, b: &Rc<RefCell<MdBox>>, y_offset: f64) {
        let idx: i64 = layout.borrow().embeds.borrow().find(&b.borrow().embedKind, &b.borrow().embedSource, b.borrow().w);
        if idx < 0 {
            let mut slot: Rc<RefCell<EVGElement>> = MdToEvg::element(b, y_offset);
            EVGElement::add_child(&parent, slot.clone());
            return;
        }
        let mut e: Rc<RefCell<MdEmbedEntry>> = (layout.borrow().embeds).borrow().entryAt(idx);
        if false == e.borrow().ok {
            let mut slot2: Rc<RefCell<EVGElement>> = MdToEvg::element(b, y_offset);
            EVGElement::add_child(&parent, slot2.clone());
            return;
        }
        let mut root: Rc<RefCell<EVGElement>> = e.borrow().root.clone().unwrap();
        // a drawing narrower than its column sits where the chart asked
        let dx: f64 = e.borrow().shiftIn(b.borrow().w.clone());
        let mut _tmp_3898 = b.borrow().h.clone();
        let mut _tmp_3899 = b.borrow().w.clone();
        MdToEvg::put(
            &root,
            b.borrow().x + dx,
            b.borrow().y + y_offset,
            e.borrow().widthIn(_tmp_3899),
            _tmp_3898
        );
        // The scene's own paper is dropped. A diagram in a document sits on
        // the document's page; painting the editor's canvas behind it puts a
        // grey slab across a white sheet.
        root.borrow_mut().background_color = Some(EVGColor::no_color());
        EVGElement::add_child(&parent, root.clone());
    }
    fn element(b: &Rc<RefCell<MdBox>>, y_offset: f64) -> Rc<RefCell<EVGElement>> {
        if b.borrow().kind == 0 {
            return MdToEvg::textElement(b, y_offset);
        }
        if b.borrow().kind == 1 {
            let mut r: Rc<RefCell<EVGElement>> = EVGElement::create_div();
            MdToEvg::place(&r, b, y_offset);
            let _tmp_3902 = EVGColor::parse(&b.borrow().fill.clone());
            r.borrow_mut().background_color = Some(_tmp_3902);
            if b.borrow().radius > 0.0 {
                EVGElement::set_attribute(
                    &r,
                    "border-radius",
                    format!("{}px", b.borrow().radius.to_string())
                );
            }
            return r;
        }
        if b.borrow().kind == 2 {
            let mut o: Rc<RefCell<EVGElement>> = EVGElement::create_div();
            MdToEvg::place(&o, b, y_offset);
            let mut _tmp_3903 = b.borrow().stroke.clone();
            MdToEvg::border(&o, &_tmp_3903, b.borrow().strokeWidth.clone());
            return o;
        }
        if b.borrow().kind == 4 {
            // A picture. The element NAMES it and does not carry it, exactly
            // as the display list below will: whoever draws it asks the same
            // byte store the layout asked.
            let mut img: Rc<RefCell<EVGElement>> = EVGElement::create_img();
            MdToEvg::place(&img, b, y_offset);
            img.borrow_mut().src = b.borrow().imagePath.clone();
            img.borrow_mut().alt = b.borrow().imageAlt.clone();
            // `contain`, not the default `cover`: the box was sized from the
            // picture's own proportions, so there is nothing to crop — and a
            // `cover` that cropped a diagram's edge would be silent.
            img.borrow_mut().object_fit = "contain".to_string();
            // a gallery's crop: an element has no source rectangle, so it
            // fills its box from the middle (the display list keeps the
            // exact part, `focus=` and all)
            if b.borrow().hasCrop {
                img.borrow_mut().object_fit = "cover".to_string();
            }
            img.borrow_mut().source_width = b.borrow().imageNaturalW as f64;
            img.borrow_mut().source_height = b.borrow().imageNaturalH as f64;
            return img;
        }
        if b.borrow().kind == 5 {
            let mut m: Rc<RefCell<EVGElement>> = MdToEvg::mathElement(b);
            MdToEvg::place(&m, b, y_offset);
            return m;
        }
        // an embed's slot: the outline, and what it was going to be
        let mut slot: Rc<RefCell<EVGElement>> = EVGElement::create_div();
        MdToEvg::place(&slot, b, y_offset);
        let mut _tmp_3909 = b.borrow().stroke.clone();
        MdToEvg::border(&slot, &_tmp_3909, b.borrow().strokeWidth.clone());
        slot
    }
    /// A formula box: its outlines at the size the layout set it at.
    fn mathElement(b: &Rc<RefCell<MdBox>>) -> Rc<RefCell<EVGElement>> {
        let mut _tmp_3911 = b.borrow().embedSource.clone();
        let mut _tmp_3912 = b.borrow().size.clone();
        let mut _tmp_3913 = b.borrow().color.clone();
        return MdMath::element(
            &_tmp_3911,
            b.borrow().embedKind == "math-display",
            _tmp_3912,
            &_tmp_3913
        ).clone();
    }
    /// the same tail as `tail` draws, as a path element of its own
    fn tailElement(b: &Rc<RefCell<MdBox>>, y_offset: f64) -> Rc<RefCell<EVGElement>> {
        let t: f64 = b.borrow().tail;
        let mut x0: f64 = b.borrow().x + b.borrow().radius * 0.9;
        if x0 > b.borrow().x + b.borrow().w * 0.3 {
            x0 = b.borrow().x + b.borrow().w * 0.3;
        }
        let left: f64 = x0 - t * 0.3;
        let mut p: Rc<RefCell<EVGElement>> = EVGElement::create_path();
        MdToEvg::put(&p, left, (b.borrow().y + b.borrow().h) + y_offset, t * 1.6, t);
        let w: String = (t * 1.6).to_string();
        let h: String = t.to_string();
        p.borrow_mut().svg_path = format!("M {} 0 L {} 0 L 0 {} Z", t * 0.6, w, h);
        p.borrow_mut().view_box = format!("0 0 {} {}", w, h);
        let _tmp_3915 = EVGColor::parse(&b.borrow().fill.clone());
        p.borrow_mut().fill_color = Some(_tmp_3915);
        p
    }
    fn place(el: &Rc<RefCell<EVGElement>>, b: &Rc<RefCell<MdBox>>, y_offset: f64) {
        let mut _tmp_3916 = b.borrow().x.clone();
        let mut _tmp_3917 = b.borrow().w.clone();
        MdToEvg::put(el, _tmp_3916, b.borrow().y + y_offset, _tmp_3917, b.borrow().h.clone());
    }
    /// ----------------------------------------------------------------------
    /// Placed AND pre-sized
    /// ----------------------------------------------------------------------
    ///
    /// The box is written twice: once as the style a layout engine would
    /// resolve, and once as the result it would resolve it to.
    ///
    /// That looks redundant and is the point. Everything this module hands over
    /// is absolutely placed with a left, a top, a width and a height that the
    /// layout above already decided — so running `EVGLayout` over it is asking
    /// a layout engine to re-derive numbers it is being given. `markdown:bench`
    /// put that at 218 ms of a 382 ms document, more than the parse and the
    /// layout together. Writing the answer down as well as the question lets
    /// the pass be skipped for the elements this module owns; the style stays
    /// because an EMBEDDED diagram's subtree still goes through the engine, and
    /// it is the same tree.
    /// The whole of a chart's words that were cut to fit (an axis title too long
    /// even on two lines), under (px, py) on the page, or "" when the point is
    /// on none. The chart is placed exactly as `paintBox` places it, so what a
    /// reader points at is what was drawn there.
    pub fn cutTextAt(layout: &Rc<RefCell<MdLayout>>, r#box: &Rc<RefCell<MdBox>>, px: f64, py: f64) -> String {
        if r#box.borrow().kind != 3 {
            return String::new();
        }
        let idx: i64 = layout.borrow().embeds.borrow().find(&r#box.borrow().embedKind, &r#box.borrow().embedSource, r#box.borrow().w);
        if idx < 0 {
            return String::new();
        }
        let mut e: Rc<RefCell<MdEmbedEntry>> = (layout.borrow().embeds).borrow().entryAt(idx);
        if false == e.borrow().ok {
            return String::new();
        }
        let mut root: Rc<RefCell<EVGElement>> = e.borrow().root.clone().unwrap();
        let dx: f64 = e.borrow().shiftIn(r#box.borrow().w.clone());
        let mut _tmp_3921 = r#box.borrow().y.clone();
        let mut _tmp_3922 = r#box.borrow().h.clone();
        let mut _tmp_3923 = r#box.borrow().w.clone();
        MdToEvg::put(
            &root,
            r#box.borrow().x + dx,
            _tmp_3921,
            e.borrow().widthIn(_tmp_3923),
            _tmp_3922
        );
        let mut le: Rc<RefCell<EVGLayout>> = Rc::new(RefCell::new(EVGLayout::new()));
        let mut _tmp_3924 = r#box.borrow().w.clone();
        EVGLayout::set_page_size(&le, _tmp_3924, r#box.borrow().h.clone());
        EVGLayout::layout(&le, root.clone());
        MdToEvg::altAt(layout, &root, px, py)
    }
    /// The `alt` of the text element under (px, py), turned with it: the point
    /// is turned back about the element's own origin and then tested against
    /// the ink of its words (the box around them is wider, see MdVegaRender).
    fn altAt(layout: &Rc<RefCell<MdLayout>>, el: &Rc<RefCell<EVGElement>>, px: f64, py: f64) -> String {
        let mut i: i64 = 0;
        while i < (el.borrow().children.len() as i64) {
            let mut c: Rc<RefCell<EVGElement>> = el.borrow().children[i as usize].clone();
            if !c.borrow().alt.is_empty() && c.borrow().tag_name == "text" {
                let x0: f64 = c.borrow().calculated_x;
                let y0: f64 = c.borrow().calculated_y;
                let mut qx: f64 = px;
                let mut qy: f64 = py;
                if c.borrow().rotate != 0.0 {
                    let mut ox: f64 = x0 + c.borrow().calculated_width * 0.5;
                    let mut oy: f64 = y0 + c.borrow().calculated_height * 0.5;
                    if c.borrow().transform_origin_x.borrow().is_set {
                        ox = x0 + c.borrow().transform_origin_x.borrow().value;
                    }
                    if c.borrow().transform_origin_y.borrow().is_set {
                        oy = y0 + c.borrow().transform_origin_y.borrow().value;
                    }
                    let a: f64 = (0.0 - c.borrow().rotate) * 0.017453292519943295;
                    let rx: f64 = px - ox;
                    let ry: f64 = py - oy;
                    qx = (ox + rx * a.cos()) - ry * a.sin();
                    qy = (oy + rx * a.sin()) + ry * a.cos();
                }
                let size: f64 = c.borrow().font_size.as_ref().unwrap().borrow().value;
                let mut _tmp_3926 = c.borrow().text_content.clone();
                let ink: f64 = layout.borrow().measureStyled(&_tmp_3926, &c.borrow().font_family.clone(), size, c.borrow().font_weight == "bold", false);
                let mut left: f64 = x0;
                if c.borrow().text_align == "center" {
                    left = (x0 + c.borrow().calculated_width * 0.5) - ink * 0.5;
                }
                if c.borrow().text_align == "right" {
                    left = (x0 + c.borrow().calculated_width) - ink;
                }
                if ((qx >= left && qx <= left + ink) && qy >= y0) && qy <= y0 + size * 1.25 {
                    return c.borrow().alt.clone();
                }
            }
            let inner: String = MdToEvg::altAt(layout, &c, px, py);
            if !inner.is_empty() {
                return inner;
            }
            i += 1;
        }
        String::new()
    }
    fn put(el: &Rc<RefCell<EVGElement>>, x: f64, y: f64, w: f64, h: f64) {
        el.borrow_mut().position = "absolute".to_string();
        el.borrow_mut().left = Some(EVGUnit::px(x));
        el.borrow_mut().top = Some(EVGUnit::px(y));
        el.borrow_mut().width = Some(EVGUnit::px(w));
        el.borrow_mut().height = Some(EVGUnit::px(h));
        el.borrow_mut().calculated_x = x;
        el.borrow_mut().calculated_y = y;
        el.borrow_mut().calculated_width = w;
        el.borrow_mut().calculated_height = h;
        el.borrow_mut().calculated_inner_width = w;
        el.borrow_mut().calculated_inner_height = h;
    }
    /// A border is written in two places in EVG and each painter reads one of
    /// them, so both are set. `effectiveBorderWidthPx` exists for the same
    /// reason and is the reader; this is the writer.
    fn border(el: &Rc<RefCell<EVGElement>>, color: &str, width: f64) {
        let mut c: Rc<RefCell<EVGColor>> = EVGColor::parse(color);
        el.borrow_mut().border_width = Some(EVGUnit::px(width));
        el.borrow_mut().border_color = Some(c.clone());
        el.borrow_mut().r#box.as_ref().unwrap().borrow_mut().border_width = Some(EVGUnit::px(width));
        el.borrow_mut().r#box.as_ref().unwrap().borrow_mut().border_width_px = width;
        el.borrow_mut().r#box.as_ref().unwrap().borrow_mut().border_color = Some(c.clone());
    }
    fn textElement(b: &Rc<RefCell<MdBox>>, y_offset: f64) -> Rc<RefCell<EVGElement>> {
        let mut t: Rc<RefCell<EVGElement>> = EVGElement::create_span();
        // `tagName == "text"` is what the PDF and HTML renderers switch on to
        // paint a string. A div with textContent lays out and prints nothing.
        t.borrow_mut().tag_name = "text".to_string();
        let mut _tmp_3934 = b.borrow().x.clone();
        let mut _tmp_3935 = b.borrow().w.clone();
        MdToEvg::put(&t, _tmp_3934, b.borrow().y + y_offset, _tmp_3935, b.borrow().h.clone());
        // The line is ALREADY broken — that is what the layout above did, and
        // it did it across four faces, which is more than a re-break here
        // could do. Saying so stops every painter re-wrapping the run against
        // its own box, which is both wasted work and a chance to disagree.
        t.borrow_mut().white_space = "nowrap".to_string();
        t.borrow_mut().text_content = b.borrow().text.clone();
        t.borrow_mut().letter_spacing = b.borrow().spacing;
        let _tmp_3940 = EVGUnit::px(b.borrow().size.clone());
        t.borrow_mut().font_size = Some(_tmp_3940);
        t.borrow_mut().inherited_font_size = b.borrow().size;
        t.borrow_mut().line_height = 1.0;
        let _tmp_3943 = EVGColor::parse(&b.borrow().color.clone());
        t.borrow_mut().color = Some(_tmp_3943);
        // Where the word goes when it is followed. The layout has carried it
        // on every segment since the first version and both roads out dropped
        // it, so a link in the PDF was blue ink and nothing else.
        if !b.borrow().link.is_empty() {
            t.borrow_mut().href = b.borrow().link.clone();
        }
        // The face is named in FULL rather than as a family plus a weight
        // flag, and `fontWeight` is left alone.
        //
        // `fontWeight: bold` would be resolved by the measurer into
        // `Open Sans-Bold` and by the HTML exporter into the family `Open
        // Sans` with the browser synthesising the weight — two different
        // faces, so a run measured at one width is drawn at another and every
        // segment after it on the line sits a gap too far right. Naming the
        // face makes the measurer, the font manager and the `@font-face` rule
        // agree, and it is the only spelling that survives bold-italic:
        // `Open Sans-Italic` plus a bold flag asks for `Open Sans-Italic-Bold`
        // and gets nothing.
        if b.borrow().bold && b.borrow().italic {
            t.borrow_mut().font_family = format!("{}-BoldItalic", b.borrow().family);
        } else {
            if b.borrow().bold {
                t.borrow_mut().font_family = format!("{}-Bold", b.borrow().family);
            } else {
                if b.borrow().italic {
                    t.borrow_mut().font_family = format!("{}-Italic", b.borrow().family);
                } else {
                    t.borrow_mut().font_family = b.borrow().family.clone();
                }
            }
        }
        t
    }
    fn paint(&mut self, layout: &Rc<RefCell<MdLayout>>, width: f64, gap: f64, total: f64, paged: bool) -> Rc<RefCell<EVGDisplayList>> {
        let mut out: Rc<RefCell<EVGDisplayList>> = Rc::new(RefCell::new(EVGDisplayList::new()));
        let ph: f64 = layout.borrow().style.borrow().pageHeight;
        // The paper. One sheet in continuous mode, one per page otherwise.
        // The page's own ground. Stated by the template when it says so, and
        // white when it does not — which is what this did for every document
        // before `page { background-color }` existed.
        let mut paper: String = "#ffffff".to_string();
        if !layout.borrow().style.borrow().pageBackground.is_empty() {
            paper = layout.borrow().style.borrow().pageBackground.clone();
        }
        if paged {
            self.rect(&out, 0.0, 0.0, width, total, "#eef0f4");
            let mut p: i64 = 0;
            while p < layout.borrow().pageCount {
                let dy: f64 = gap + (p as f64) * (ph + gap);
                // a slide's own colour (`.dark page { background-color }`)
                let own: String = layout.borrow().paperOf(p);
                let mut sheet_paper: String = paper.clone();
                if !own.is_empty() {
                    sheet_paper = own.clone();
                }
                self.rect(&out, 0.0, dy, width, ph, &sheet_paper);
                self.paperAt(&out, layout, &layout.borrow().pictureOf(p).clone(), 0.0, dy, width, ph);
                p += 1;
            }
        } else {
            self.rect(&out, 0.0, 0.0, width, total, &paper);
            self.paper(&out, layout, 0.0, 0.0, width, total);
        }
        let mut i: i64 = 0;
        let n: i64 = layout.borrow().boxes.len() as i64;
        while i < n {
            let mut r#box: Rc<RefCell<MdBox>> = layout.borrow().boxes[i as usize].clone();
            let mut dy_1: f64 = 0.0;
            if paged {
                dy_1 = gap + (r#box.borrow().page as f64) * (ph + gap);
            }
            self.paintBox(&out, layout, &r#box, dy_1);
            i += 1;
        }
        out
    }
    /// A picture behind the page, when the template names one and the store has
    /// it. Never a reason for a page to have nothing on it: a background the
    /// store does not have is simply not drawn, and the text is where it was.
    fn paper(&self, out: &Rc<RefCell<EVGDisplayList>>, layout: &Rc<RefCell<MdLayout>>, x: f64, y: f64, w: f64, h: f64) {
        self.paperAt(out, layout, &layout.borrow().style.borrow().pageBackgroundImage.clone(), x, y, w, h);
    }
    fn paperAt(&self, out: &Rc<RefCell<EVGDisplayList>>, layout: &Rc<RefCell<MdLayout>>, name: &str, x: f64, y: f64, w: f64, h: f64) {
        if name.is_empty() {
            return;
        }
        if false == layout.borrow().haveImages {
            return;
        }
        let path: String = Vfs::resolve(&layout.borrow().images, name, &layout.borrow().docDir.clone());
        if path.is_empty() {
            return;
        }
        let mut st: Rc<RefCell<VfsStat>> = Vfs::stat(&layout.borrow().images, &path);
        if false == st.borrow().found {
            return;
        }
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 2;
        c.borrow_mut().x = x;
        c.borrow_mut().y = y;
        c.borrow_mut().w = w;
        c.borrow_mut().h = h;
        c.borrow_mut().src = path.clone();
        c.borrow_mut().a = 1.0;
        out.borrow_mut().cmds.push(c.clone());
    }
    /// An effect over a block (`{fx=…}`, MdLayout.blockEffect): what EVG makes
    /// of an element with `evg-surface-effect` — an instance in the list and a
    /// transparent rectangle that carries it, so a painter without effects
    /// draws nothing. Its id is its place among the list's instances.
    fn effect(&self, out: &Rc<RefCell<EVGDisplayList>>, r#box: &Rc<RefCell<MdBox>>, y: f64) {
        let id: String = format!("fx-b{}", out.borrow().fx_ids.len() as i64);
        out.borrow_mut().fx_ids.push(id.clone());
        out.borrow_mut().fx_kinds.push(r#box.borrow().embedKind.clone());
        out.borrow_mut().fx_triggers.push("always".to_string().to_string());
        out.borrow_mut().fx_x.push(r#box.borrow().x);
        out.borrow_mut().fx_y.push(y);
        out.borrow_mut().fx_w.push(r#box.borrow().w);
        out.borrow_mut().fx_h.push(r#box.borrow().h);
        out.borrow_mut().fx_radius.push(r#box.borrow().radius);
        out.borrow_mut().fx_param_start.push(out.borrow().fx_param_names.len() as i64);
        let mut i: i64 = 0;
        while i < (r#box.borrow().fxNames.len() as i64) {
            out.borrow_mut().fx_param_names.push(r#box.borrow().fxNames[i as usize].clone());
            out.borrow_mut().fx_param_values.push(r#box.borrow().fxValues[i as usize]);
            i += 1;
        }
        out.borrow_mut().fx_param_count.push(r#box.borrow().fxNames.len() as i64);
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 0;
        c.borrow_mut().x = r#box.borrow().x;
        c.borrow_mut().y = y;
        c.borrow_mut().w = r#box.borrow().w;
        c.borrow_mut().h = r#box.borrow().h;
        c.borrow_mut().radius = r#box.borrow().radius;
        c.borrow_mut().a = 0.0;
        c.borrow_mut().effect_id = id.clone();
        out.borrow_mut().cmds.push(c.clone());
    }
    fn paintBox(&mut self, out: &Rc<RefCell<EVGDisplayList>>, layout: &Rc<RefCell<MdLayout>>, r#box: &Rc<RefCell<MdBox>>, dy: f64) {
        let y: f64 = r#box.borrow().y + dy;
        if r#box.borrow().kind == 1 {
            let mut _tmp_3956 = r#box.borrow().x.clone();
            let mut _tmp_3957 = r#box.borrow().w.clone();
            let mut _tmp_3958 = r#box.borrow().h.clone();
            self.rect(out, _tmp_3956, y, _tmp_3957, _tmp_3958, &r#box.borrow().fill.clone());
            if r#box.borrow().radius > 0.0 {
                let mut last: Rc<RefCell<EVGDrawCmd>> = out.borrow().cmds[((out.borrow().cmds.len() as i64) - 1) as usize].clone();
                last.borrow_mut().radius = r#box.borrow().radius;
            }
            if r#box.borrow().tail > 0.0 {
                self.tail(out, r#box, y);
            }
            return;
        }
        if r#box.borrow().kind == 2 {
            let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
            c.borrow_mut().kind = 1;
            c.borrow_mut().x = r#box.borrow().x;
            c.borrow_mut().y = y;
            c.borrow_mut().w = r#box.borrow().w;
            c.borrow_mut().h = r#box.borrow().h;
            c.borrow_mut().thickness = r#box.borrow().strokeWidth;
            c.borrow_mut().radius = r#box.borrow().radius;
            self.tint(&c, &r#box.borrow().stroke.clone());
            out.borrow_mut().cmds.push(c.clone());
            return;
        }
        if r#box.borrow().kind == 4 {
            // A picture. The command NAMES it and does not carry it, which is
            // what a display list does with every picture: the browser host
            // keys a blob URL by the name, the PDF renderer opens it, and
            // `PptxFromEvg` asks the byte store the list was built against.
            let mut p: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
            p.borrow_mut().kind = 2;
            p.borrow_mut().x = r#box.borrow().x;
            p.borrow_mut().y = y;
            p.borrow_mut().w = r#box.borrow().w;
            p.borrow_mut().h = r#box.borrow().h;
            p.borrow_mut().src = r#box.borrow().imagePath.clone();
            p.borrow_mut().text = r#box.borrow().imageAlt.clone();
            p.borrow_mut().a = 1.0;
            // a gallery's `cover`: the part of the picture that fills the box
            if r#box.borrow().hasCrop {
                p.borrow_mut().has_crop = true;
                p.borrow_mut().crop_x = r#box.borrow().cropX;
                p.borrow_mut().crop_y = r#box.borrow().cropY;
                p.borrow_mut().crop_w = r#box.borrow().cropW;
                p.borrow_mut().crop_h = r#box.borrow().cropH;
            }
            p.borrow_mut().radius = r#box.borrow().radius;
            out.borrow_mut().cmds.push(p.clone());
            return;
        }
        if r#box.borrow().kind == 0 {
            let mut t: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
            t.borrow_mut().kind = 3;
            t.borrow_mut().x = r#box.borrow().x;
            t.borrow_mut().y = y;
            t.borrow_mut().w = r#box.borrow().w;
            t.borrow_mut().h = r#box.borrow().h;
            // The command's height is the FONT'S line box, not the line's.
            // `textElement` sets `lineHeight = 1.0` and the walk resolves that
            // to the size, so a painter reading `h` gets the size — and a
            // direct list that handed it the leading instead would put every
            // baseline a few points low. Checked by `MarkdownTest.presized`,
            // which is how this was found rather than seen.
            t.borrow_mut().h = r#box.borrow().size;
            t.borrow_mut().text = r#box.borrow().text.clone();
            t.borrow_mut().font_size = r#box.borrow().size;
            let mut _tmp_3983 = r#box.borrow().family.clone();
            let mut _tmp_3984 = r#box.borrow().bold.clone();
            let _tmp_3986 = (layout.borrow().style).borrow().faceFor(&_tmp_3983, _tmp_3984, r#box.borrow().italic.clone());
            t.borrow_mut().font_family = _tmp_3986;
            t.borrow_mut().letter_spacing = r#box.borrow().spacing;
            self.tint(&t, &r#box.borrow().color.clone());
            out.borrow_mut().cmds.push(t.clone());
            return;
        }
        if r#box.borrow().kind == 6 {
            self.effect(out, r#box, y);
            return;
        }
        if r#box.borrow().kind == 5 {
            let mut mroot: Rc<RefCell<EVGElement>> = MdToEvg::mathElement(r#box);
            let mut _tmp_3989 = r#box.borrow().x.clone();
            let mut _tmp_3990 = r#box.borrow().w.clone();
            MdToEvg::put(&mroot, _tmp_3989, y, _tmp_3990, r#box.borrow().h.clone());
            let mut ml: Rc<RefCell<EVGLayout>> = Rc::new(RefCell::new(EVGLayout::new()));
            let mut _tmp_3992 = r#box.borrow().w.clone();
            EVGLayout::set_page_size(&ml, _tmp_3992, r#box.borrow().h.clone());
            EVGLayout::layout(&ml, mroot.clone());
            let mut mone: Rc<RefCell<EVGDisplayList>> = Rc::new(RefCell::new(EVGDisplayList::new()));
            EVGDisplayList::build(&mone, mroot.clone());
            EVGDisplayList::append_from(&out, &mone);
            return;
        }
        // An embed: the scene's own subtree, through the engine, appended
        // where the slot is.
        let idx: i64 = layout.borrow().embeds.borrow().find(&r#box.borrow().embedKind, &r#box.borrow().embedSource, r#box.borrow().w);
        if idx < 0 {
            return;
        }
        let mut e: Rc<RefCell<MdEmbedEntry>> = (layout.borrow().embeds).borrow().entryAt(idx);
        if false == e.borrow().ok {
            return;
        }
        let mut root: Rc<RefCell<EVGElement>> = e.borrow().root.clone().unwrap();
        let dx: f64 = e.borrow().shiftIn(r#box.borrow().w.clone());
        let mut _tmp_3996 = r#box.borrow().h.clone();
        MdToEvg::put(&root, r#box.borrow().x + dx, y, e.borrow().widthIn(r#box.borrow().w.clone()), _tmp_3996);
        let mut le: Rc<RefCell<EVGLayout>> = Rc::new(RefCell::new(EVGLayout::new()));
        let mut _tmp_3998 = r#box.borrow().w.clone();
        EVGLayout::set_page_size(&le, _tmp_3998, r#box.borrow().h.clone());
        EVGLayout::layout(&le, root.clone());
        let mut one: Rc<RefCell<EVGDisplayList>> = Rc::new(RefCell::new(EVGDisplayList::new()));
        EVGDisplayList::build(&one, root.clone());
        EVGDisplayList::append_from(&out, &one);
    }
    fn rect(&mut self, out: &Rc<RefCell<EVGDisplayList>>, x: f64, y: f64, w: f64, h: f64, fill: &str) {
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 0;
        c.borrow_mut().x = x;
        c.borrow_mut().y = y;
        c.borrow_mut().w = w;
        c.borrow_mut().h = h;
        self.tint(&c, fill);
        out.borrow_mut().cmds.push(c.clone());
    }
    /// A `{container=bubble}` plate's tail: a triangle hanging from the bottom
    /// edge near the left, in the plate's colour. It meets the edge exactly:
    /// an overlap would show twice as dark on a see-through plate.
    fn tail(&mut self, out: &Rc<RefCell<EVGDisplayList>>, r#box: &Rc<RefCell<MdBox>>, y: f64) {
        let t: f64 = r#box.borrow().tail;
        let bottom: f64 = y + r#box.borrow().h;
        let mut x0: f64 = r#box.borrow().x + r#box.borrow().radius * 0.9;
        if x0 > r#box.borrow().x + r#box.borrow().w * 0.3 {
            x0 = r#box.borrow().x + r#box.borrow().w * 0.3;
        }
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 6;
        c.borrow_mut().x = x0 - t * 0.3;
        c.borrow_mut().y = bottom;
        c.borrow_mut().w = t * 1.6;
        c.borrow_mut().h = t + 1.0;
        c.borrow_mut().pts.push(x0 + t * 0.3);
        c.borrow_mut().pts.push(bottom);
        c.borrow_mut().pts.push(x0 + t * 1.3);
        c.borrow_mut().pts.push(bottom);
        c.borrow_mut().pts.push(x0 - t * 0.3);
        c.borrow_mut().pts.push((bottom + t) + 1.0);
        c.borrow_mut().ring_ends.push(6);
        self.tint(&c, &r#box.borrow().fill.clone());
        out.borrow_mut().cmds.push(c.clone());
    }
    fn tint(&mut self, c: &Rc<RefCell<EVGDrawCmd>>, value: &str) {
        let mut col: Rc<RefCell<EVGColor>> = self.colorOf(value);
        c.borrow_mut().r = EVGColor::red(&col);
        c.borrow_mut().g = EVGColor::green(&col);
        c.borrow_mut().b = EVGColor::blue(&col);
        c.borrow_mut().a = EVGColor::alpha(&col);
    }
    fn colorOf(&mut self, value: &str) -> Rc<RefCell<EVGColor>> {
        let mut i: i64 = 0;
        let n: i64 = self.colorKeys.len() as i64;
        while i < n {
            if self.colorKeys[i as usize].clone() == value {
                return self.colorVals[i as usize].clone();
            }
            i += 1;
        }
        let mut col: Rc<RefCell<EVGColor>> = EVGColor::parse(value);
        self.colorKeys.push(value.to_string());
        self.colorVals.push(col.clone());
        col
    }
}
