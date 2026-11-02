// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_style::MdStyle;
use crate::rgr::MdText;

/// ============================================================================
/// MdCssLimits.rgr — how far a value in a sheet may go before the layout stops
/// ============================================================================
///
/// A sheet is typed by a person, and a person types `font-size: 2020px` (or a
/// slider runs to the end of its travel). The layout used to take any number
/// it was given: a heading two thousand points tall is one glyph per slide, a
/// slide split into dozens, and a picture of each glyph thousands of pixels
/// tall for the renderer to keep. Nothing about that is an error the author
/// can see — the page simply stops answering.
///
/// So every number the layout reads from a sheet (or from the front matter, or
/// from the reader's page size) has a SAFETY RANGE, and the layout clamps to it
/// at the point it reads the style. The source is not touched: the sheet still
/// says 2020px, the editor still shows it, and the slide is drawn as if it said
/// the most it may. These are not the ranges a slider offers (`PresRange` in
/// Sliqtly, a sixth of the slide for a margin): they are the edge past which a
/// value is not a design any more, so they are wide.
///
/// The range of a length is measured against the PAGE, because what breaks a
/// layout is a value that is large next to the page it is laid out on:
///
///   page     the page's own width and height: 1in .. 200in
///   font     a font size: 1pt .. the page's shorter side
///   inset    the page's padding: up to 0.4 of the shorter side (a fifth of the
///            page stays for the content)
///   space    margins, gaps, indents, a picture's height: up to half the
///            longer side
///   pad      a block's own padding: up to a quarter of the shorter side
///   stroke   a border: up to an eighth of the shorter side
///   ratio    a line height: 0.5 .. 5
///
/// A value that is not a number at all (NaN, infinite) is the low end of its
/// range — the layout must never see one.
/// ============================================================================
#[derive(Clone)]
pub struct MdCssLimits {
}

impl MdCssLimits {
    pub fn new() -> Self {
        Self {
        }
    }
    /// Pages a layout may run to by overflowing, past the slides and sheets a
    /// document asks for. A block taller than any page used to be cut onto as
    /// many as it took; past this the rest stays on the last page.
    pub fn pageBudget() -> i64 {
        1000
    }
    /// What the property does, from its CSS name: "page", "font", "inset",
    /// "space", "pad", "stroke", "ratio" or "" (not a length this knows).
    /// `sel` is the selector it is written under (page, chart, h1, …).
    pub fn kindOf(prop: &str, sel: &str) -> String {
        let n: i64 = MdText::str_len(&prop) as i64;
        if n >= 9 && MdText::substr(&prop, n - 9, n) == "font-size" {
            return "font".to_string();
        }
        if prop == "header-size" || prop == "footer-size" {
            return "font".to_string();
        }
        if prop == "line-height" {
            return "ratio".to_string();
        }
        if sel == "page" && (prop == "width" || prop == "height") {
            return "page".to_string();
        }
        if MdText::index_of(&prop, "border") == 0 {
            if MdText::index_of(&prop, "width") >= 0 {
                return "stroke".to_string();
            }
            return String::new();
        }
        if prop == "outline-width" || prop == "stroke-width" {
            return "stroke".to_string();
        }
        let is_pad: bool = MdText::index_of(&prop, "padding") == 0;
        if is_pad && (sel == "page" || sel == "deck") {
            return "inset".to_string();
        }
        if is_pad && MdCssLimits::isIndent(prop, sel) {
            return "space".to_string();
        }
        if is_pad {
            return "pad".to_string();
        }
        if MdText::index_of(&prop, "margin") == 0 {
            return "space".to_string();
        }
        if MdText::index_of(&prop, "gap") >= 0 {
            return "space".to_string();
        }
        if MdText::index_of(&prop, "height") >= 0 {
            return "space".to_string();
        }
        if MdText::index_of(&prop, "width") >= 0 {
            return "space".to_string();
        }
        if prop == "text-indent" || prop == "indent" {
            return "space".to_string();
        }
        String::new()
    }
    /// A list's or a quote's left padding is the indent beside the block, not
    /// padding inside it.
    pub fn isIndent(prop: &str, sel: &str) -> bool {
        if prop != "padding-left" {
            return false;
        }
        sel == "list" || (sel == "ul" || (sel == "ol" || sel == "blockquote"))
    }
    pub fn shortOf(w: f64, h: f64) -> f64 {
        if h < w {
            return h;
        }
        w
    }
    pub fn longOf(w: f64, h: f64) -> f64 {
        if h > w {
            return h;
        }
        w
    }
    /// The page a range is measured against, itself kept sane: a page of 0 or
    /// of a million inches must not make every other range 0 or a million.
    pub fn sanePage(v: f64) -> f64 {
        let _tmp_1867 = MdCssLimits::lo("page", 0.0, 0.0);
        MdCssLimits::into(v, _tmp_1867, MdCssLimits::hi("page", 0.0, 0.0))
    }
    /// The low end of a kind's range, in points (a plain ratio for "ratio").
    pub fn lo(kind: &str, page_w: f64, page_h: f64) -> f64 {
        if kind == "page" {
            return 72.0;
        }
        if kind == "font" {
            return 1.0;
        }
        if kind == "ratio" {
            return 0.5;
        }
        0.0
    }
    /// The high end, against a page of pageW × pageH points.
    pub fn hi(kind: &str, page_w: f64, page_h: f64) -> f64 {
        if kind == "page" {
            return 14400.0;
        }
        if kind == "ratio" {
            return 5.0;
        }
        let w: f64 = MdCssLimits::sanePage(page_w);
        let h: f64 = MdCssLimits::sanePage(page_h);
        let short: f64 = MdCssLimits::shortOf(w, h);
        let long: f64 = MdCssLimits::longOf(w, h);
        if kind == "font" {
            if short < 72.0 {
                return 72.0;
            }
            return short;
        }
        if kind == "inset" {
            return short * 0.4;
        }
        if kind == "space" {
            return long / 2.0;
        }
        if kind == "pad" {
            return short / 4.0;
        }
        if kind == "stroke" {
            return short / 8.0;
        }
        long
    }
    /// v into lo..hi; NaN and infinities to lo.
    pub fn into(v: f64, lo: f64, hi: f64) -> f64 {
        if v != v {
            return lo;
        }
        if v < lo {
            return lo;
        }
        if v > hi {
            return hi;
        }
        v
    }
    /// One value of a kind, against the page.
    pub fn clampKind(kind: &str, v: f64, page_w: f64, page_h: f64) -> f64 {
        let _tmp_1869 = MdCssLimits::lo(kind, page_w, page_h);
        MdCssLimits::into(v, _tmp_1869, MdCssLimits::hi(kind, page_w, page_h))
    }
    /// One CSS property's value, in points, against the page: what a reader
    /// outside `MdStyle` (a chart's defaults, a figure's size) clamps with.
    /// A property this does not know is returned as it is, unless it is not a
    /// number.
    pub fn clamp(prop: &str, sel: &str, v: f64, page_w: f64, page_h: f64) -> f64 {
        let kind: String = MdCssLimits::kindOf(prop, sel);
        if kind.is_empty() {
            if v != v {
                return 0.0;
            }
            return v;
        }
        MdCssLimits::clampKind(&kind, v, page_w, page_h)
    }
    /// Whether the value is already in range: what an editor asks to say
    /// "this is past what is drawn".
    pub fn inRange(prop: &str, sel: &str, v: f64, page_w: f64, page_h: f64) -> bool {
        MdCssLimits::clamp(prop, sel, v, page_w, page_h) == v
    }
    /// A size that may be 0 for "not set" (the header's and footer's own).
    pub fn optional(kind: &str, v: f64, page_w: f64, page_h: f64) -> f64 {
        if v == 0.0 {
            return 0.0;
        }
        MdCssLimits::clampKind(kind, v, page_w, page_h)
    }
    /// Every number in a style, into its range. The page first, because every
    /// other range is measured against it.
    pub fn clampStyle(s: &Rc<RefCell<MdStyle>>) {
        let _tmp_1872 = MdCssLimits::clampKind("page", s.borrow().pageWidth.clone(), 0.0, 0.0);
        s.borrow_mut().pageWidth = _tmp_1872;
        let _tmp_1874 = MdCssLimits::clampKind("page", s.borrow().pageHeight.clone(), 0.0, 0.0);
        s.borrow_mut().pageHeight = _tmp_1874;
        let w: f64 = s.borrow().pageWidth;
        let h: f64 = s.borrow().pageHeight;
        let _tmp_1876 = MdCssLimits::clampKind("inset", s.borrow().margin.clone(), w, h);
        s.borrow_mut().margin = _tmp_1876;
        let _tmp_1878 = MdCssLimits::clampKind("font", s.borrow().bodySize.clone(), w, h);
        s.borrow_mut().bodySize = _tmp_1878;
        let _tmp_1880 = MdCssLimits::clampKind("font", s.borrow().h1Size.clone(), w, h);
        s.borrow_mut().h1Size = _tmp_1880;
        let _tmp_1882 = MdCssLimits::clampKind("font", s.borrow().h2Size.clone(), w, h);
        s.borrow_mut().h2Size = _tmp_1882;
        let _tmp_1884 = MdCssLimits::clampKind("font", s.borrow().h3Size.clone(), w, h);
        s.borrow_mut().h3Size = _tmp_1884;
        let _tmp_1886 = MdCssLimits::clampKind("font", s.borrow().h4Size.clone(), w, h);
        s.borrow_mut().h4Size = _tmp_1886;
        let _tmp_1888 = MdCssLimits::clampKind("font", s.borrow().h5Size.clone(), w, h);
        s.borrow_mut().h5Size = _tmp_1888;
        let _tmp_1890 = MdCssLimits::clampKind("font", s.borrow().h6Size.clone(), w, h);
        s.borrow_mut().h6Size = _tmp_1890;
        let _tmp_1892 = MdCssLimits::clampKind("font", s.borrow().codeSize.clone(), w, h);
        s.borrow_mut().codeSize = _tmp_1892;
        let _tmp_1894 = MdCssLimits::clampKind("font", s.borrow().furnitureSize.clone(), w, h);
        s.borrow_mut().furnitureSize = _tmp_1894;
        let _tmp_1896 = MdCssLimits::optional("font", s.borrow().headerSize.clone(), w, h);
        s.borrow_mut().headerSize = _tmp_1896;
        let _tmp_1898 = MdCssLimits::optional("font", s.borrow().footerSize.clone(), w, h);
        s.borrow_mut().footerSize = _tmp_1898;
        let _tmp_1900 = MdCssLimits::optional("font", s.borrow().tableSize.clone(), w, h);
        s.borrow_mut().tableSize = _tmp_1900;
        let _tmp_1902 = MdCssLimits::clampKind("ratio", s.borrow().lineHeight.clone(), w, h);
        s.borrow_mut().lineHeight = _tmp_1902;
        let _tmp_1904 = MdCssLimits::clampKind("ratio", s.borrow().codeLineHeight.clone(), w, h);
        s.borrow_mut().codeLineHeight = _tmp_1904;
        let _tmp_1906 = MdCssLimits::clampKind("space", s.borrow().paraSpacing.clone(), w, h);
        s.borrow_mut().paraSpacing = _tmp_1906;
        let _tmp_1908 = MdCssLimits::clampKind("space", s.borrow().headingSpaceBefore.clone(), w, h);
        s.borrow_mut().headingSpaceBefore = _tmp_1908;
        let _tmp_1910 = MdCssLimits::clampKind("space", s.borrow().headingSpaceAfter.clone(), w, h);
        s.borrow_mut().headingSpaceAfter = _tmp_1910;
        let _tmp_1912 = MdCssLimits::clampKind("space", s.borrow().listIndent.clone(), w, h);
        s.borrow_mut().listIndent = _tmp_1912;
        let _tmp_1914 = MdCssLimits::clampKind("space", s.borrow().markerGap.clone(), w, h);
        s.borrow_mut().markerGap = _tmp_1914;
        let _tmp_1916 = MdCssLimits::clampKind("space", s.borrow().listTightSpacing.clone(), w, h);
        s.borrow_mut().listTightSpacing = _tmp_1916;
        let _tmp_1918 = MdCssLimits::clampKind("space", s.borrow().quoteIndent.clone(), w, h);
        s.borrow_mut().quoteIndent = _tmp_1918;
        let _tmp_1920 = MdCssLimits::clampKind("space", s.borrow().embedSpacing.clone(), w, h);
        s.borrow_mut().embedSpacing = _tmp_1920;
        let _tmp_1922 = MdCssLimits::clampKind("space", s.borrow().imgMaxHeight.clone(), w, h);
        s.borrow_mut().imgMaxHeight = _tmp_1922;
        let _tmp_1924 = MdCssLimits::clampKind("space", s.borrow().headerImageH.clone(), w, h);
        s.borrow_mut().headerImageH = _tmp_1924;
        let _tmp_1926 = MdCssLimits::clampKind("space", s.borrow().footerImageH.clone(), w, h);
        s.borrow_mut().footerImageH = _tmp_1926;
        let _tmp_1928 = MdCssLimits::clampKind("pad", s.borrow().codePadding.clone(), w, h);
        s.borrow_mut().codePadding = _tmp_1928;
        let _tmp_1930 = MdCssLimits::clampKind("pad", s.borrow().quotePadY.clone(), w, h);
        s.borrow_mut().quotePadY = _tmp_1930;
        let _tmp_1932 = MdCssLimits::clampKind("pad", s.borrow().cellPadX.clone(), w, h);
        s.borrow_mut().cellPadX = _tmp_1932;
        let _tmp_1934 = MdCssLimits::clampKind("pad", s.borrow().cellPadY.clone(), w, h);
        s.borrow_mut().cellPadY = _tmp_1934;
        let _tmp_1936 = MdCssLimits::clampKind("stroke", s.borrow().quoteBarWidth.clone(), w, h);
        s.borrow_mut().quoteBarWidth = _tmp_1936;
    }
    /// A block's own `font-size` is a multiple of the size it would have had
    /// (`{.lead}` at 1.3em). The multiple that takes `base` out of the font
    /// range is cut to the one that lands on its edge.
    pub fn ratio(r: f64, base: f64, page_w: f64, page_h: f64) -> f64 {
        if r != r || base <= 0.0 {
            return 1.0;
        }
        let size: f64 = MdCssLimits::clampKind("font", base * r, page_w, page_h);
        size / base
    }
}
