// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::rgr::MdText;

/// ============================================================================
/// MdStyle.rgr — the values a stylesheet sets, in one place.
/// ============================================================================
///
/// Every number the layout uses lives here and nowhere else, so that the layout
/// never has one written into it. That was true when this was a class at the
/// top of `MdLayout.rgr`; it is a file of its own now for one reason:
/// `MdCss` resolves a stylesheet ONTO it, and the layout asks `MdCss` what a
/// block's `{.classes}` mean. Both directions through one file would be a
/// cycle; the style in the middle is what breaks it.
///
///   MdStyle.rgr   ── the fields
///        ↑
///   MdCss.rgr     ── what a sheet's properties mean, over gallery/css
///        ↑
///   MdLayout.rgr  ── asks the sheet about a block, lays it out
///
/// Nothing moved. This is the same class, in its own file.
/// ============================================================================
#[derive(Clone)]
pub struct MdStyle {
    pub bodyFamily: String,
    /// There is no monospace face in `gallery/pdf_writer/assets/fonts`, so
    /// code is set in the sans face that is there. It is measured and painted
    /// with the same one, which is what keeps the geometry honest; it is not
    /// monospaced, which is a missing asset and not a layout decision.
    pub monoFamily: String,
    pub headingFamily: String,
    pub bodySize: f64,
    pub lineHeight: f64,
    pub paraSpacing: f64,
    pub h1Size: f64,
    pub h2Size: f64,
    pub h3Size: f64,
    pub h4Size: f64,
    pub h5Size: f64,
    pub h6Size: f64,
    /// A heading's colour: per level, then `heading`, then the text's ("" =
    /// not set, fall through).
    pub headingColor: String,
    pub h1Color: String,
    pub h2Color: String,
    pub h3Color: String,
    pub h4Color: String,
    pub h5Color: String,
    pub h6Color: String,
    pub headingSpaceBefore: f64,
    pub headingSpaceAfter: f64,
    pub codeSize: f64,
    pub codeLineHeight: f64,
    pub codePadding: f64,
    pub codeBackground: String,
    /// <mark>: the highlight behind the text, and the text on it
    pub markBackground: String,
    pub markColor: String,
    /// <kbd>: a key cap's face and edge; "" follows `code`
    pub kbdBackground: String,
    pub kbdBorder: String,
    pub codeBorder: String,
    /// The five colours a listing is read with. Not a theme yet — the values a
    /// stylesheet will set, in one place.
    pub codeKeyword: String,
    pub codeString: String,
    pub codeComment: String,
    pub codeNumber: String,
    pub codePlain: String,
    pub highlightCode: bool,
    pub textColor: String,
    /// `p { color }`: a paragraph's text, "" for the document's
    pub paraColor: String,
    /// The page's own ground, and a picture on it.
    ///
    /// `background-color` was already here three times — for code, for a table
    /// head, for a blockquote — and nowhere for the PAGE, which is the one a
    /// reader notices first and the one a company template is mostly made of.
    /// `background-image` names a picture in the byte store; it is drawn behind
    /// everything and is never the reason a page has no text on it.
    pub pageBackground: String,
    pub pageBackgroundImage: String,
    pub mutedColor: String,
    pub linkColor: String,
    pub ruleColor: String,
    pub quoteBarColor: String,
    pub quoteBarWidth: f64,
    pub quoteIndent: f64,
    /// The space inside the bar, above the first line and below the last.
    ///
    /// Without it the bar began exactly at the top of the first line's box and
    /// ended exactly where the next block started — flush against the text at
    /// one end and dangling past it at the other, because what it was measuring
    /// at the bottom was the last paragraph's trailing space rather than the
    /// paragraph. A quote is a block with an inside; this is the inside.
    pub quotePadY: f64,
    /// Quoted text is set in the muted colour, which is what every markdown
    /// style does and what says "this is somebody else's sentence" without a
    /// box around it. A link, a code span or a heading inside the quote keeps
    /// its own colour: this is the colour of ordinary text in there, not a wash
    /// over everything.
    pub quoteColor: String,
    /// `blockquote { background-color }`: a band under the quote, "" for none
    pub quoteBackground: String,
    /// `{container=box}` / `{container=bubble}`: the theme's plate,
    /// `container { background-color; border-radius }`. "" / negative leave it
    /// to `MdContainer.plan` (the page's colour, a radius from the text size).
    pub containerBackground: String,
    pub containerRadius: f64,
    /// `container { box-shadow: 0 6pt 18pt rgba(…) }`: the colour of a soft
    /// shadow under the plate, "" for none
    pub containerShadow: String,
    /// `container { padding; border }`: the room inside the plate and a line
    /// round it, -1 / "" to leave them to `MdContainer.plan`
    pub containerPadX: f64,
    pub containerPadY: f64,
    pub containerBorder: String,
    pub containerBorderWidth: f64,
    pub listIndent: f64,
    pub markerGap: f64,
    /// The colour of a list's bullets and numbers: `list { color }`. Empty
    /// follows the muted colour, which is what a marker was before a sheet
    /// could say anything about it.
    pub markerColor: String,
    /// What separates two items of a TIGHT list — one with no blank line
    /// between its items.
    ///
    /// CommonMark's whole definition of tightness is that such an item's text
    /// is not a paragraph, and a paragraph's spacing is what was being charged:
    /// a five-item shopping list was set with the same air between the items as
    /// between two paragraphs of prose, which is what a LOOSE list is for. The
    /// distinction is in the parser, was on the node, and only the two extra
    /// points a loose list adds were ever read.
    pub listTightSpacing: f64,
    pub tableBorder: String,
    pub tableHeadBackground: String,
    /// `th { color }`: the header row's text, "" for the table's own
    pub tableHeadColor: String,
    pub cellPadX: f64,
    pub cellPadY: f64,
    /// the size a table's text is set at, 0 for the body's
    /// (`table { font-size }`)
    pub tableSize: f64,
    /// `table { cell-tones: "Done=green Late=red" }`: cells coloured by their
    /// text in every table (`MdCellTones`), "" for none
    pub tableCellTones: String,
    pub pageWidth: f64,
    pub pageHeight: f64,
    pub margin: f64,
    /// PRINT (`@media print { page { bleed; safe-area } deck { crop-marks } }`,
    /// set by `MdCss.applyPrint` only when the layout is for paper). `bleed`:
    /// how far a picture that reaches the page's edge runs past it, for the
    /// cut; `safeArea`: how far in from the edge text has to stay (the layout
    /// report checks it); `cropMarks`: the PDF draws the cut lines. On a
    /// screen all three are 0 / off and nothing moves.
    pub forPrint: bool,
    pub bleed: f64,
    pub safeArea: f64,
    pub cropMarks: bool,
    /// Page furniture. All off by default: a document that said nothing about
    /// itself gets a plain page, which is what a README wants.
    pub title: String,
    /// "" | "title" | "h1"
    pub runningHead: String,
    /// "" | "bottom-center" | "bottom-right" | "bottom-outer"
    pub pageNumbers: String,
    pub wantToc: bool,
    pub breakOnH1: bool,
    pub furnitureSize: f64,
    /// What every page says at its top and its bottom (`header { content }`,
    /// `footer { … }`, or `header:` / `footer:` in the front matter), with a
    /// picture beside it (`background-image: url(logo.png)`). {page}, {pages}
    /// and {title} are filled in per page.
    pub headerText: String,
    pub headerImage: String,
    pub headerAlign: String,
    pub headerSize: f64,
    pub headerColor: String,
    pub headerImageH: f64,
    pub footerText: String,
    pub footerImage: String,
    pub footerAlign: String,
    pub footerSize: f64,
    pub footerColor: String,
    pub footerImageH: f64,
    /// Three places on each edge, beside the one above: `header-left`,
    /// `header-center`, `header-right` (and the footer's). A place may hold a
    /// picture written as in the text, `![](media/logo.png) ACME`.
    pub headerLeft: String,
    pub headerCenter: String,
    pub headerRight: String,
    pub footerLeft: String,
    pub footerCenter: String,
    pub footerRight: String,
    /// How the edge is set: bold text, a band of colour behind it reaching the
    /// sheet's edge, a hairline between it and the content.
    pub headerBold: bool,
    pub headerBackground: String,
    pub headerRule: String,
    pub footerBold: bool,
    pub footerBackground: String,
    pub footerRule: String,
    /// The sheets an edge is left off: "first", "last", page numbers, or any
    /// of them together (`header-skip: first last`).
    pub headerSkip: String,
    pub footerSkip: String,
    /// Pictures (`img { }`): where a picture narrower than the column sits
    /// ("" the start, "center", "end"; a slide centres unless told) and how
    /// tall one may be (0: as tall as the room).
    pub imgAlign: String,
    pub imgMaxHeight: f64,
    /// An embedded diagram that is wider than the column is scaled down to it,
    /// never up: a three-node flowchart should not become a poster.
    pub embedSpacing: f64,
    /// `font-weight`, `font-style`, `text-transform` and `letter-spacing` for
    /// the elements a sheet names them for (`h1`, `heading`, `document`, `p`,
    /// `li`, `blockquote`), by element name (`MdTypo`, `typoFor`)
    pub typoTags: Vec<String>,
    pub typoList: Vec<Rc<RefCell<MdTypo>>>,
}

impl MdStyle {
    pub fn new() -> Self {
        Self {
            bodyFamily: "Open Sans".to_string(),
            monoFamily: "Noto Sans".to_string(),
            headingFamily: "Open Sans".to_string(),
            bodySize: 11.0,
            lineHeight: 1.45,
            paraSpacing: 9.0,
            h1Size: 24.0,
            h2Size: 19.0,
            h3Size: 15.5,
            h4Size: 13.0,
            h5Size: 11.5,
            h6Size: 11.0,
            headingColor: String::new(),
            h1Color: String::new(),
            h2Color: String::new(),
            h3Color: String::new(),
            h4Color: String::new(),
            h5Color: String::new(),
            h6Color: String::new(),
            headingSpaceBefore: 16.0,
            headingSpaceAfter: 7.0,
            codeSize: 9.5,
            codeLineHeight: 1.42,
            codePadding: 9.0,
            codeBackground: "#f6f8fa".to_string(),
            markBackground: "#ffe766".to_string(),
            markColor: "#1f2328".to_string(),
            kbdBackground: String::new(),
            kbdBorder: String::new(),
            codeBorder: "#e2e6ea".to_string(),
            codeKeyword: "#cf222e".to_string(),
            codeString: "#0a3069".to_string(),
            codeComment: "#57606a".to_string(),
            codeNumber: "#0550ae".to_string(),
            codePlain: "#1f2328".to_string(),
            highlightCode: true,
            textColor: "#1f2328".to_string(),
            paraColor: String::new(),
            pageBackground: String::new(),
            pageBackgroundImage: String::new(),
            mutedColor: "#57606a".to_string(),
            linkColor: "#0969da".to_string(),
            ruleColor: "#d8dee4".to_string(),
            quoteBarColor: "#d0d7de".to_string(),
            quoteBarWidth: 3.0,
            quoteIndent: 16.0,
            quotePadY: 6.0,
            quoteColor: "#57606a".to_string(),
            quoteBackground: String::new(),
            containerBackground: String::new(),
            containerRadius: -1.0,
            containerShadow: String::new(),
            containerPadX: -1.0,
            containerPadY: -1.0,
            containerBorder: String::new(),
            containerBorderWidth: -1.0,
            listIndent: 22.0,
            markerGap: 8.0,
            markerColor: String::new(),
            listTightSpacing: 3.0,
            tableBorder: "#d0d7de".to_string(),
            tableHeadBackground: "#f6f8fa".to_string(),
            tableHeadColor: String::new(),
            cellPadX: 7.0,
            cellPadY: 5.0,
            tableSize: 0.0,
            tableCellTones: String::new(),
            pageWidth: 595.28,
            pageHeight: 841.89,
            margin: 56.0,
            forPrint: false,
            bleed: 0.0,
            safeArea: 0.0,
            cropMarks: false,
            title: String::new(),
            runningHead: String::new(),
            pageNumbers: String::new(),
            wantToc: false,
            breakOnH1: false,
            furnitureSize: 8.5,
            headerText: String::new(),
            headerImage: String::new(),
            headerAlign: "start".to_string(),
            headerSize: 0.0,
            headerColor: String::new(),
            headerImageH: 22.0,
            footerText: String::new(),
            footerImage: String::new(),
            footerAlign: "start".to_string(),
            footerSize: 0.0,
            footerColor: String::new(),
            footerImageH: 22.0,
            headerLeft: String::new(),
            headerCenter: String::new(),
            headerRight: String::new(),
            footerLeft: String::new(),
            footerCenter: String::new(),
            footerRight: String::new(),
            headerBold: false,
            headerBackground: String::new(),
            headerRule: String::new(),
            footerBold: false,
            footerBackground: String::new(),
            footerRule: String::new(),
            headerSkip: String::new(),
            footerSkip: String::new(),
            imgAlign: String::new(),
            imgMaxHeight: 0.0,
            embedSpacing: 12.0,
            typoTags: Vec::new(),
            typoList: Vec::new(),
        }
    }
    /// The typography a sheet gave `tag`, created empty on first ask.
    pub fn typoAt(&mut self, tag: &str) -> Rc<RefCell<MdTypo>> {
        let mut i: i64 = 0;
        while i < (self.typoTags.len() as i64) {
            if self.typoTags[i as usize].clone() == tag {
                return self.typoList[i as usize].clone();
            }
            i += 1;
        }
        let mut t: Rc<RefCell<MdTypo>> = Rc::new(RefCell::new(MdTypo::new()));
        self.typoTags.push(tag.to_string());
        self.typoList.push(t.clone());
        t
    }
    /// …and what holds for a block written as `tag`: the document's, then
    /// `heading`'s for a heading, then the tag's own — each over the last.
    pub fn typoFor(&self, tag: &str) -> Rc<RefCell<MdTypo>> {
        let mut out: Rc<RefCell<MdTypo>> = Rc::new(RefCell::new(MdTypo::new()));
        if self.typoTags.is_empty() {
            return out;
        }
        out.borrow_mut().over(&self.typoOf("document"));
        if (MdText::str_len(&tag) as i64) == 2 && MdText::char_at(&tag, 0) == 104 {
            out.borrow_mut().over(&self.typoOf("heading"));
        }
        out.borrow_mut().over(&self.typoOf(tag));
        out
    }
    pub fn typoOf(&self, tag: &str) -> Rc<RefCell<MdTypo>> {
        let mut i: i64 = 0;
        while i < (self.typoTags.len() as i64) {
            if self.typoTags[i as usize].clone() == tag {
                return self.typoList[i as usize].clone();
            }
            i += 1;
        }
        Rc::new(RefCell::new(MdTypo::new()))
    }
    pub fn typoText(&self) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < (self.typoTags.len() as i64) {
            let mut t: Rc<RefCell<MdTypo>> = self.typoList[i as usize].clone();
            out = format!("{}{}={};", out, self.typoTags[i as usize].clone(), t.borrow().text());
            i += 1;
        }
        out
    }
    /// ----------------------------------------------------------------------
    /// The look, without the sizes
    /// ----------------------------------------------------------------------
    ///
    /// Every colour, face, picture and typography the style holds, as a list,
    /// and the same list put back. A slide's own rules (`#id h2 { color }`,
    /// `.dark page { background-color }`) are laid over the style while that
    /// slide is laid out and taken off again after (`MdLayout.enterScope`);
    /// sizes are not in it, because a slide that is shrunk to fit scales the
    /// style's sizes in place and a second copy of them would undo that.
    ///
    /// **A colour added above and not added here stays the same on every
    /// slide** — that is the cost of the list; `MdCssTest` checks a few.
    pub fn look(&self) -> Vec<String> {
        let mut v: Vec<String> = vec![self.bodyFamily.clone(), self.monoFamily.clone(), self.headingFamily.clone(), self.headingColor.clone(), self.h1Color.clone(), self.h2Color.clone(), self.h3Color.clone(), self.h4Color.clone(), self.h5Color.clone(), self.h6Color.clone()];
        v.push(self.codeBackground.clone());
        v.push(self.markBackground.clone());
        v.push(self.markColor.clone());
        v.push(self.kbdBackground.clone());
        v.push(self.kbdBorder.clone());
        v.push(self.codeBorder.clone());
        v.push(self.codeKeyword.clone());
        v.push(self.codeString.clone());
        v.push(self.codeComment.clone());
        v.push(self.codeNumber.clone());
        v.push(self.codePlain.clone());
        v.push(self.textColor.clone());
        v.push(self.pageBackground.clone());
        v.push(self.pageBackgroundImage.clone());
        v.push(self.mutedColor.clone());
        v.push(self.linkColor.clone());
        v.push(self.ruleColor.clone());
        v.push(self.quoteBarColor.clone());
        v.push(self.quoteColor.clone());
        v.push(self.quoteBackground.clone());
        v.push(self.containerBackground.clone());
        v.push(self.containerShadow.clone());
        v.push(self.markerColor.clone());
        v.push(self.tableBorder.clone());
        v.push(self.tableHeadBackground.clone());
        v.push(self.tableHeadColor.clone());
        v.push(self.tableCellTones.clone());
        v.push(self.headerColor.clone());
        v.push(self.footerColor.clone());
        v.push(self.headerBackground.clone());
        v.push(self.footerBackground.clone());
        v.push(self.headerRule.clone());
        v.push(self.footerRule.clone());
        v.push(self.imgAlign.clone());
        v.push(self.typoText());
        v.push(self.containerBorder.clone());
        v.push(self.paraColor.clone());
        v.to_vec()
    }
    pub fn setLook(&mut self, v: &[String]) {
        if (v.len() as i64) < 47 {
            return;
        }
        self.bodyFamily = v[0].clone();
        self.monoFamily = v[1].clone();
        self.headingFamily = v[2].clone();
        self.headingColor = v[3].clone();
        self.h1Color = v[4].clone();
        self.h2Color = v[5].clone();
        self.h3Color = v[6].clone();
        self.h4Color = v[7].clone();
        self.h5Color = v[8].clone();
        self.h6Color = v[9].clone();
        self.codeBackground = v[10].clone();
        self.markBackground = v[11].clone();
        self.markColor = v[12].clone();
        self.kbdBackground = v[13].clone();
        self.kbdBorder = v[14].clone();
        self.codeBorder = v[15].clone();
        self.codeKeyword = v[16].clone();
        self.codeString = v[17].clone();
        self.codeComment = v[18].clone();
        self.codeNumber = v[19].clone();
        self.codePlain = v[20].clone();
        self.textColor = v[21].clone();
        self.pageBackground = v[22].clone();
        self.pageBackgroundImage = v[23].clone();
        self.mutedColor = v[24].clone();
        self.linkColor = v[25].clone();
        self.ruleColor = v[26].clone();
        self.quoteBarColor = v[27].clone();
        self.quoteColor = v[28].clone();
        self.quoteBackground = v[29].clone();
        self.containerBackground = v[30].clone();
        self.containerShadow = v[31].clone();
        self.markerColor = v[32].clone();
        self.tableBorder = v[33].clone();
        self.tableHeadBackground = v[34].clone();
        self.tableHeadColor = v[35].clone();
        self.tableCellTones = v[36].clone();
        self.headerColor = v[37].clone();
        self.footerColor = v[38].clone();
        self.headerBackground = v[39].clone();
        self.footerBackground = v[40].clone();
        self.headerRule = v[41].clone();
        self.footerRule = v[42].clone();
        self.imgAlign = v[43].clone();
        self.setTypoText(&v[44].clone());
        self.containerBorder = v[45].clone();
        self.paraColor = v[46].clone();
    }
    pub fn setTypoText(&mut self, text: &str) {
        let mut tags: Vec<String> = Vec::new();
        let mut list: Vec<Rc<RefCell<MdTypo>>> = Vec::new();
        let mut parts: Vec<String> = text.split(";").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut i: i64 = 0;
        while i < (parts.len() as i64) {
            let part: String = parts[i as usize].clone();
            let eq: i64 = MdText::index_of(&part, "=");
            if eq > 0 {
                tags.push(MdText::substr(&part, 0, eq));
                list.push(MdTypo::parse(&MdText::substr(&part, eq + 1, (MdText::str_len(&part) as i64))));
            }
            i += 1;
        }
        self.typoTags = tags.clone();
        self.typoList = list.clone();
    }
    /// Every field of this style, as one string.
    ///
    /// Two uses, and they are the same question asked twice. The LAYOUT CACHE
    /// keys a block's boxes under the source that made them — and under this,
    /// because the same source set 4pt larger is different boxes. Without it,
    /// switching the company template redrew a paragraph at the new size on
    /// the positions measured at the old one, and the second half of a wrapped
    /// line sat on top of the first. `MdCssTest` reads it too, to ask whether
    /// a sheet changed anything it should not have.
    ///
    /// Colours are in it as well as sizes. They do not move a box, but they
    /// are ON one — a template that changes only the link colour must not
    /// replay the last one's.
    ///
    /// **A field added above and not added here is a field the cache cannot
    /// see.** That is the cost of the list, and it is why the test compares
    /// whole fingerprints rather than the three fields somebody remembered.
    pub fn fingerprint(&self) -> String {
        let mut out: String = String::new();
        out = format!("{}body {}/{}", out, self.bodyFamily, self.bodySize);
        out = format!("{}/{}/{}", out, self.lineHeight, self.paraSpacing);
        out = format!("{} head {}/{}", out, self.headingFamily, self.h1Size);
        out = format!("{}/{}/{}", out, self.h2Size, self.h3Size);
        out = format!("{}/{}/{}", out, self.h4Size, self.h5Size);
        out = format!("{}/{}/{}", out, self.h6Size, self.headingSpaceBefore);
        out = format!("{}/{}", out, self.headingSpaceAfter);
        out = format!("{} hc {}/{}/{}/{}/{}/{}/{}", out, self.headingColor, self.h1Color, self.h2Color, self.h3Color, self.h4Color, self.h5Color, self.h6Color);
        out = format!("{} code {}/{}", out, self.monoFamily, self.codeSize);
        out = format!("{}/{}/{}", out, self.codeLineHeight, self.codePadding);
        out = format!("{}/{}/{}", out, self.codeBackground, self.codeBorder);
        out = format!("{}/{}/{}", out, self.codeKeyword, self.codeString);
        out = format!("{}/{}/{}", out, self.codeComment, self.codeNumber);
        out = format!("{}/{}", out, self.codePlain);
        out = format!("{} mark {}/{} kbd {}/{}", out, self.markBackground, self.markColor, self.kbdBackground, self.kbdBorder);
        if self.highlightCode {
            out = format!("{}/hi", out);
        }
        out = format!("{} ink {}/{}", out, self.textColor, self.mutedColor);
        out = format!("{}/{}/{}", out, self.linkColor, self.ruleColor);
        out = format!("{} quote {}/{}", out, self.quoteColor, self.quoteBarColor);
        out = format!("{}/{}/{}", out, self.quoteBarWidth, self.quoteIndent);
        out = format!("{}/{}", out, self.quotePadY);
        out = format!("{} container {}/{}", out, self.containerBackground, self.containerRadius);
        out = format!("{}/{}", out, self.containerShadow);
        out = format!("{} list {}/{}", out, self.listIndent, self.markerGap);
        out = format!("{}/{}", out, self.listTightSpacing);
        out = format!("{}/{}", out, self.markerColor);
        out = format!("{} table {}/{}", out, self.tableBorder, self.tableHeadBackground);
        out = format!("{}/{}/{}", out, self.cellPadX, self.cellPadY);
        out = format!("{}/{}", out, self.tableSize);
        out = format!("{}/{}", out, self.tableCellTones);
        out = format!("{} page {}/{}", out, self.pageWidth, self.pageHeight);
        out = format!("{}/{}", out, self.margin);
        if self.forPrint {
            out = format!("{} print {}/{}", out, self.bleed, self.safeArea);
        }
        out = format!("{}/{}/{}", out, self.pageBackground, self.pageBackgroundImage);
        out = format!("{} furniture {}/{}", out, self.title, self.runningHead);
        out = format!("{}/{}", out, self.pageNumbers);
        if self.wantToc {
            out = format!("{}/toc", out);
        }
        if self.breakOnH1 {
            out = format!("{}/h1", out);
        }
        out = format!("{}/{}", out, self.furnitureSize);
        out = format!("{} head {}/{}/{}/{}/{}/{}", out, self.headerText, self.headerImage, self.headerAlign, self.headerSize, self.headerColor, self.headerImageH);
        out = format!("{} foot {}/{}/{}/{}/{}/{}", out, self.footerText, self.footerImage, self.footerAlign, self.footerSize, self.footerColor, self.footerImageH);
        out = format!("{} slots {}/{}/{}/{}/{}/{}", out, self.headerLeft, self.headerCenter, self.headerRight, self.footerLeft, self.footerCenter, self.footerRight);
        out = format!("{} edge {}/{}/{}/{}/{}/{}", out, self.headerBackground, self.headerRule, self.footerBackground, self.footerRule, self.headerSkip, self.footerSkip);
        if self.headerBold {
            out = format!("{}/hb", out);
        }
        if self.footerBold {
            out = format!("{}/fb", out);
        }
        out = format!("{} img {}/{}", out, self.imgAlign, self.imgMaxHeight);
        out = format!("{} quote+ {} th {} p {}", out, self.quoteBackground, self.tableHeadColor, self.paraColor);
        out = format!("{} plate {}/{}/{}/{}", out, self.containerPadX, self.containerPadY, self.containerBorder, self.containerBorderWidth);
        out = format!("{} typo {}", out, self.typoText());
        out = format!("{} embed {}", out, self.embedSpacing);
        out
    }
    /// The colour a token of that kind is set in.
    pub fn codeColor(&self, kind: i64) -> String {
        if kind == 1 {
            return self.codeKeyword.clone();
        }
        if kind == 2 {
            return self.codeString.clone();
        }
        if kind == 3 {
            return self.codeComment.clone();
        }
        if kind == 4 {
            return self.codeNumber.clone();
        }
        self.codePlain.clone()
    }
    pub fn headingInk(&self, level: i64) -> String {
        let mut c: String = self.h6Color.clone();
        if level == 1 {
            c = self.h1Color.clone();
        }
        if level == 2 {
            c = self.h2Color.clone();
        }
        if level == 3 {
            c = self.h3Color.clone();
        }
        if level == 4 {
            c = self.h4Color.clone();
        }
        if level == 5 {
            c = self.h5Color.clone();
        }
        if !c.is_empty() {
            return c;
        }
        if !self.headingColor.is_empty() {
            return self.headingColor.clone();
        }
        self.textColor.clone()
    }
    pub fn headingSize(&self, level: i64) -> f64 {
        if level == 1 {
            return self.h1Size;
        }
        if level == 2 {
            return self.h2Size;
        }
        if level == 3 {
            return self.h3Size;
        }
        if level == 4 {
            return self.h4Size;
        }
        if level == 5 {
            return self.h5Size;
        }
        self.h6Size
    }
    /// The face, by the convention the font manager and the measurer share:
    /// the weight is a `-Bold` suffix and the slant is part of the family name.
    pub fn faceFor(&self, family: &str, bold: bool, italic: bool) -> String {
        if bold && italic {
            return format!("{}-BoldItalic", family);
        }
        if italic {
            return format!("{}-Italic", family);
        }
        if bold {
            return format!("{}-Bold", family);
        }
        family.to_string()
    }
}

/// One piece of inline text with one look.
/// `font-weight`, `font-style`, `text-transform` and `letter-spacing` as a
/// block's text takes them. -1 / "" / unset is "not said", so one rule can be
/// laid over another (`over`) the way the cascade inherits them.
#[derive(Clone)]
pub struct MdTypo {
    /// 1 bold, 0 normal, -1 not said
    pub weight: i64,
    /// 1 italic, 0 normal, -1 not said
    pub italic: i64,
    /// "uppercase" / "lowercase" / "capitalize" / "none", "" for not said
    pub transform: String,
    /// in points; `spacingSet` false for not said
    pub spacing: f64,
    pub spacingSet: bool,
}

impl MdTypo {
    pub fn new() -> Self {
        Self {
            weight: -1,
            italic: -1,
            transform: String::new(),
            spacing: 0.0,
            spacingSet: false,
        }
    }
    pub fn parse(t: &str) -> Rc<RefCell<MdTypo>> {
        let mut out: Rc<RefCell<MdTypo>> = Rc::new(RefCell::new(MdTypo::new()));
        let mut parts: Vec<String> = t.split(",").map(|s| s.to_string()).collect::<Vec<String>>();
        if (parts.len() as i64) < 4 {
            return out;
        }
        let w: Option<i64> = parts[0].clone().parse::<i64>().ok();
        out.borrow_mut().weight = if w.is_some() { w.unwrap() } else { -1 };
        let it: Option<i64> = parts[1].clone().parse::<i64>().ok();
        out.borrow_mut().italic = if it.is_some() { it.unwrap() } else { -1 };
        out.borrow_mut().transform = parts[2].clone();
        let sp: String = parts[3].clone();
        if !sp.is_empty() {
            let d: Option<f64> = sp.parse::<f64>().ok();
            out.borrow_mut().spacing = if d.is_some() { d.unwrap() } else { 0.0 };
            out.borrow_mut().spacingSet = true;
        }
        out
    }
    /// `text` as `text-transform` says to set it. Letters only change case;
    /// `capitalize` raises the first letter of each word.
    pub fn apply(mode: &str, text: &str) -> String {
        if mode == "uppercase" {
            return text.to_uppercase();
        }
        if mode == "lowercase" {
            return text.to_lowercase();
        }
        if mode == "capitalize" {
            // by code point: on Go a `charAt` is a byte, and the first
            // letter of "äänet" two of them
            let mut out: String = String::new();
            let mut start: bool = true;
            let mut cs: Vec<i64> = text.chars().map(|c| c as i64).collect::<Vec<i64>>();
            let n: i64 = cs.len() as i64;
            let mut i: i64 = 0;
            while i < n {
                let c: i64 = cs[i as usize];
                let ch: String = MdChar::fromCode(c);
                if c == 32 || (c == 9 || (c == 10 || c == 45)) {
                    start = true;
                    out = format!("{}{}", out, ch);
                } else {
                    if start {
                        out = format!("{}{}", out, ch.to_uppercase());
                    } else {
                        out = format!("{}{}", out, ch);
                    }
                    start = false;
                }
                i += 1;
            }
            return out;
        }
        text.to_string()
    }
    pub fn copy(&self) -> Rc<RefCell<MdTypo>> {
        let mut t: Rc<RefCell<MdTypo>> = Rc::new(RefCell::new(MdTypo::new()));
        t.borrow_mut().over(&Rc::new(RefCell::new(self.clone())));
        t
    }
    pub fn isEmpty(&self) -> bool {
        ((self.weight < 0 && self.italic < 0) && self.transform.is_empty()) && !self.spacingSet
    }
    /// what `top` says, over what this says
    pub fn over(&mut self, top: &Rc<RefCell<MdTypo>>) {
        if top.borrow().weight >= 0 {
            self.weight = top.borrow().weight;
        }
        if top.borrow().italic >= 0 {
            self.italic = top.borrow().italic;
        }
        if !top.borrow().transform.is_empty() {
            self.transform = top.borrow().transform.clone();
        }
        if top.borrow().spacingSet {
            self.spacing = top.borrow().spacing;
            self.spacingSet = true;
        }
    }
    pub fn text(&self) -> String {
        let mut sp: String = String::new();
        if self.spacingSet {
            sp = self.spacing.to_string();
        }
        format!("{},{},{},{}", self.weight.to_string(), self.italic, self.transform, sp)
    }
}
