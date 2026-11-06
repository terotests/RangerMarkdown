// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::rgr::MdText;

/// ============================================================================
/// MdNode.rgr — the markdown AST.
/// ============================================================================
///
/// One class, a `kind` string, and children. Not a class per node type: the
/// parity dump has to walk this generically and compare it with a reference
/// implementation's XML, and a switch on a string does that in one loop where
/// a type hierarchy would need a visitor per target language.
///
/// The kinds are CommonMark's own names, so the XML this dumps and the XML
/// `commonmark` (the JS reference) dumps can be diffed without a translation
/// table in between — the one thing MERMAID_PARITY.md needed and did not have.
///
///   blocks   document block_quote list item code_block html_block paragraph
///            heading thematic_break table table_row table_cell
///   inlines  text softbreak linebreak code html_inline emph strong strike
///            link image
///   …and markdown-it's plugins, which a deck reads and CommonMark does not:
///            sub (`~x~`) sup (`^x^`) ins (`++x++`) mark (`==x==`)
///            abbr (`*[HTML]: …`, the title in `title`)
///
/// …and the blocks of markdown-it's standard plugins, under their names:
///
///   footnote_def   `[^label]: text`, a container (markdown-it-footnote)
///   footnote_ref   `[^label]` / `^[inline text]`, an inline (same)
///   dl dt dd       a definition list, its term and its definitions
///                  (markdown-it-deflist)
///   container      `::: warning` … `:::` (markdown-it-container)
///
/// Every node carries `srcStart` / `srcEnd`: byte offsets into the text it was
/// parsed from. The viewer needs them to put a caret where a reader clicked,
/// and the incremental reparse needs them to know what to throw away.
/// ============================================================================
#[derive(Clone)]
pub struct MdNode {
    pub kind: String,
    /// text / code / html content. Empty for containers.
    pub literal: String,
    pub children: Vec<Rc<RefCell<MdNode>>>,
    /// heading level 1..6; for an item, the marker's content indent.
    pub level: i64,
    /// fenced code: the info string, verbatim, before the first space.
    pub info: String,
    pub fenced: bool,
    /// list / item
    pub listOrdered: bool,
    pub listStart: i64,
    pub listTight: bool,
    /// "." or ")" for ordered, the bullet character for unordered.
    pub listDelim: String,
    /// -1 = not a task item, 0 = unchecked, 1 = checked (GFM).
    pub taskState: i64,
    /// link / image
    pub destination: String,
    pub title: String,
    /// ------------------------------------------------------------------
    /// Footnotes (markdown-it-footnote)
    /// ------------------------------------------------------------------
    ///
    /// A `footnote_ref` carries the number its footnote was given — in the
    /// order footnotes are first REFERENCED, not defined, as markdown-it
    /// numbers them — and which reference to it this is (0 for the first),
    /// which is what an HTML back-link is named by. Its label is `info`.
    ///
    /// A `footnote_def` carries the same number (0: never referenced, so it is
    /// drawn nowhere) and how many references it has. One with no label is an
    /// inline footnote's body (`^[…]`): it is not in the tree where it was
    /// written, only in the document's `notes`.
    ///
    /// `notes`, on the document only: every referenced footnote in number
    /// order, so `notes[n - 1]` is footnote n. A defined footnote is ALSO in
    /// the tree where its definition stands; a reader that walks the tree
    /// draws nothing there, and one that wants the footnotes asks this.
    pub noteNumber: i64,
    pub noteSub: i64,
    pub noteRefs: i64,
    pub notes: Vec<Rc<RefCell<MdNode>>>,
    /// table: the column alignment, "" | "left" | "center" | "right"
    pub align: String,
    pub isHeader: bool,
    /// source map, in characters, into the document the parser was handed.
    pub srcStart: i64,
    pub srcEnd: i64,
    /// ------------------------------------------------------------------
    /// …and the same thing one level finer, for the text INSIDE a block.
    /// ------------------------------------------------------------------
    ///
    /// `srcStart` says where a paragraph began. It does not say where the
    /// fourth character of its third word is, and that is what a caret in a
    /// drawn document needs — the viewer could name the paragraph a reader
    /// clicked and nothing smaller.
    ///
    /// The two are not the same string. `literal` is what the block parser
    /// handed the inline parser, and getting there strips list indents, block
    /// quote markers and their following space, expands tabs into spaces that
    /// were never in the file, drops reference definitions off the front, and
    /// trims both ends. So offset 12 of `literal` is not offset 12 of anything.
    ///
    /// These three arrays are the correspondence, as PIECES: `mapLit[i]`
    /// characters of `literal` starting at `mapSrc[i]` in the document, for
    /// `mapLen[i]` characters. Pieces are in order and do not overlap, and a
    /// literal offset in no piece has no source — which happens, and is
    /// reported rather than guessed at:
    ///
    ///   * the spaces a tab was expanded into. They are one source character
    ///     between them, so the piece names the TAB and is one long against
    ///     several literal characters; only its first maps back.
    ///   * `[x] ` taken off a task item — a marker the reader sees drawn as a
    ///     checkbox rather than as text.
    ///
    /// The inline parser stamps every node it makes through this, so an `emph`
    /// three levels down carries the offsets of the `*` that opened it and the
    /// `*` that closed it.
    pub mapLit: Vec<i64>,
    pub mapSrc: Vec<i64>,
    pub mapLen: Vec<i64>,
    /// Has this node's own span been decided by something that knows better
    /// than the step it was created in? A link began at a `[` several steps
    /// back and an emphasis is decided after the line has been read, so both
    /// set their span themselves — and the blanket stamp at the end of the
    /// step must not then overwrite it. Without this the link took the span of
    /// its own closing bracket, which is valid, ordered, and wrong.
    pub srcKnown: bool,
    /// A text node that STANDS FOR its source rather than spelling it: `©`
    /// for `(c)`, `…` for `...`, 😉 for `:wink:`. Its literal and its span are
    /// different strings of different lengths, and the correspondence is the
    /// whole span to the whole literal — there is no fourth character of `©`
    /// to put a caret after. Every other text node maps character for
    /// character, which is what the layout's offset arithmetic assumes; this
    /// flag is how a run tells it not to.
    pub subst: bool,
    /// On the document node: everything document-wide the inlines were read
    /// with (`MdInlineOptions.signature`). The same block source reads to
    /// different nodes under a different one, so a layout cache keyed on the
    /// source has to be keyed on this too.
    pub readingSig: String,
    /// ------------------------------------------------------------------
    /// What the document said this block IS — `{.lead #totals cols=3}`
    /// ------------------------------------------------------------------
    ///
    /// Goldmark's block attributes, which is the syntax a Hugo site is already
    /// written in. They are carried and not interpreted: `MdAttrs` reads the
    /// characters, `MdCss` decides what a class MEANS, and the layout asks the
    /// resolved style rather than the class — the same split `CssCore` and
    /// `PptxCss` already keep.
    pub classes: Vec<String>,
    pub nodeId: String,
    pub attrKeys: Vec<String>,
    pub attrVals: Vec<String>,
    /// What could not be read in a heading's `{…}` (MdAttrs.headingAttrs),
    /// one line per token, for the writer. Empty when all of it was read.
    pub attrNotes: Vec<String>,
    /// ------------------------------------------------------------------
    /// Parser state. It lives on the node rather than in a table beside it
    /// for the reason cmark's own reference implementation puts it there:
    /// the block algorithm walks a stack of half-built nodes and asks each
    /// one whether this line continues it, and a parallel structure would
    /// have to be kept in step with the tree through every open and close.
    /// None of it survives `finalize` in a way a consumer should read.
    /// ------------------------------------------------------------------
    pub open: bool,
    pub lines: Vec<String>,
    /// How long the raw text accumulated in `lines` is, counting the newline
    /// each one is joined with. The map pieces below are built against THAT
    /// string as the lines arrive, because the transformations between it and
    /// `literal` are all prefix drops and trims — recorded once at the end by
    /// `shiftMap` — while the lines themselves are rewritten in place by
    /// `stripReferences` and would be gone by then.
    pub rawLen: i64,
    pub lastLineBlank: bool,
    pub lastLineChecked: bool,
    /// The lines (1-based) a block began and last took a line on. A definition
    /// list's term is a paragraph of ONE line directly above its first
    /// definition, or one blank line above it, and these are how the parser
    /// can still tell once the paragraph has been closed.
    pub firstLineNo: i64,
    pub lastLineNo: i64,
    /// fenced code
    pub fenceChar: i64,
    pub fenceLength: i64,
    pub fenceOffset: i64,
    /// html block start condition, 1..7
    pub htmlBlockType: i64,
    /// list item: the marker's own offset, and the width of marker plus the
    /// spaces after it — together, the indent a continuation line needs
    pub markerOffset: i64,
    pub padding: i64,
    /// a list's marker character, kept so a different one starts a new list
    pub listBulletChar: i64,
}

impl MdNode {
    pub fn new() -> Self {
        Self {
            kind: "text".to_string(),
            literal: String::new(),
            children: Vec::new(),
            level: 0,
            info: String::new(),
            fenced: false,
            listOrdered: false,
            listStart: 1,
            listTight: true,
            listDelim: String::new(),
            taskState: -1,
            destination: String::new(),
            title: String::new(),
            noteNumber: 0,
            noteSub: 0,
            noteRefs: 0,
            notes: Vec::new(),
            align: String::new(),
            isHeader: false,
            srcStart: 0,
            srcEnd: 0,
            mapLit: Vec::new(),
            mapSrc: Vec::new(),
            mapLen: Vec::new(),
            srcKnown: false,
            subst: false,
            readingSig: String::new(),
            classes: Vec::new(),
            nodeId: String::new(),
            attrKeys: Vec::new(),
            attrVals: Vec::new(),
            attrNotes: Vec::new(),
            open: true,
            lines: Vec::new(),
            rawLen: 0,
            lastLineBlank: false,
            lastLineChecked: false,
            firstLineNo: 0,
            lastLineNo: 0,
            fenceChar: 0,
            fenceLength: 0,
            fenceOffset: 0,
            htmlBlockType: 0,
            markerOffset: 0,
            padding: 0,
            listBulletChar: 0,
        }
    }
    pub fn create(kind: String) -> Rc<RefCell<MdNode>> {
        let mut n: Rc<RefCell<MdNode>> = Rc::new(RefCell::new(MdNode::new()));
        n.borrow_mut().kind = kind.clone();
        n
    }
    pub fn text(value: String) -> Rc<RefCell<MdNode>> {
        let mut n: Rc<RefCell<MdNode>> = Rc::new(RefCell::new(MdNode::new()));
        n.borrow_mut().kind = "text".to_string();
        n.borrow_mut().literal = value.clone();
        n
    }
    /// <br>, <br/>, <br /> and <BR …>
    pub fn isBrTag(tag: &str) -> bool {
        let t: String = tag.trim().to_string().to_lowercase();
        if (MdText::str_len(&t) as i64) < 4 {
            return false;
        }
        if MdText::substr(&t, 0, 3) != "<br" {
            return false;
        }
        let c: i64 = MdText::char_at(&t, 3);
        c == 62 || (c == 47 || (c == 32 || (c == 9 || c == 10)))
    }
    pub fn spaces(n: i64) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < n {
            out = format!("{}  ", out);
            i += 1;
        }
        out
    }
    pub fn xmlEscape(v: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&v) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if c == 38 {
                out = format!("{}&amp;", out);
            }
            if c == 60 {
                out = format!("{}&lt;", out);
            }
            if c == 62 {
                out = format!("{}&gt;", out);
            }
            if c == 34 {
                out = format!("{}&quot;", out);
            }
            if ((c != 38 && c != 60) && c != 62) && c != 34 {
                out = format!("{}{}", out, MdText::substr(&v, i, i + 1));
            }
            i += 1;
        }
        out
    }
    pub fn addClass(&mut self, c: &str) {
        if c.is_empty() {
            return;
        }
        if self.hasClass(c) {
            return;
        }
        self.classes.push(c.to_string());
    }
    pub fn classCount(&self) -> i64 {
        self.classes.len() as i64
    }
    pub fn classAt(&self, i: i64) -> String {
        self.classes[i as usize].clone()
    }
    pub fn hasClass(&self, c: &str) -> bool {
        let mut i: i64 = 0;
        let n: i64 = self.classes.len() as i64;
        while i < n {
            if self.classes[i as usize].clone() == c {
                return true;
            }
            i += 1;
        }
        false
    }
    /// Last one wins, which is what a cascade of two attribute blocks on the
    /// same node would mean and what every other reader of this syntax does.
    pub fn setAttr(&mut self, k: &str, v: &str) {
        let mut i: i64 = 0;
        let n: i64 = self.attrKeys.len() as i64;
        while i < n {
            if self.attrKeys[i as usize].clone() == k {
                self.attrVals[i as usize] = v.to_string();
                return;
            }
            i += 1;
        }
        self.attrKeys.push(k.to_string());
        self.attrVals.push(v.to_string());
    }
    pub fn hasAttr(&self, k: &str) -> bool {
        let mut i: i64 = 0;
        let n: i64 = self.attrKeys.len() as i64;
        while i < n {
            if self.attrKeys[i as usize].clone() == k {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn attrValue(&self, k: &str) -> String {
        let mut i: i64 = 0;
        let n: i64 = self.attrKeys.len() as i64;
        while i < n {
            if self.attrKeys[i as usize].clone() == k {
                return self.attrVals[i as usize].clone();
            }
            i += 1;
        }
        String::new()
    }
    pub fn attrCount(&self) -> i64 {
        self.attrKeys.len() as i64
    }
    /// Everything an attribute block said, moved from one node to another. The
    /// block parser builds the `{…}` line as a node of its own and then folds
    /// it into the block above; this is that fold.
    pub fn takeAttrsFrom(&mut self, o: &Rc<RefCell<MdNode>>) {
        let mut i: i64 = 0;
        while i < (o.borrow().classes.len() as i64) {
            self.addClass(&o.borrow().classes[i as usize].clone().clone());
            i += 1;
        }
        if !o.borrow().nodeId.is_empty() {
            self.nodeId = o.borrow().nodeId.clone();
        }
        let mut j: i64 = 0;
        while j < (o.borrow().attrKeys.len() as i64) {
            let arg_0 = o.borrow().attrKeys[j as usize].clone().clone();
            self.setAttr(&arg_0, &o.borrow().attrVals[j as usize].clone().clone());
            j += 1;
        }
    }
    pub fn add(&mut self, child: Rc<RefCell<MdNode>>) {
        self.children.push(child.clone());
    }
    pub fn childCount(&self) -> i64 {
        self.children.len() as i64
    }
    pub fn childAt(&self, i: i64) -> Rc<RefCell<MdNode>> {
        self.children[i as usize].clone()
    }
    pub fn lastChild(&self) -> Rc<RefCell<MdNode>> {
        let n: i64 = self.children.len() as i64;
        self.children[(n - 1) as usize].clone()
    }
    pub fn hasChildren(&self) -> bool {
        let n: i64 = self.children.len() as i64;
        n > 0
    }
    /// Whether this node may hold inline children. The block parser leaves the
    /// raw text in `literal` on these and the inline parser replaces it.
    pub fn isLeaf(&self) -> bool {
        if self.kind == "paragraph" {
            return true;
        }
        if self.kind == "heading" {
            return true;
        }
        if self.kind == "table_cell" {
            return true;
        }
        // a definition list's term is one line of inline text
        if self.kind == "dt" {
            return true;
        }
        false
    }
    /// The footnote numbered `n`, or a node of kind "none" when there is none.
    pub fn noteAt(&self, n: i64) -> Rc<RefCell<MdNode>> {
        if n < 1 || n > (self.notes.len() as i64) {
            return MdNode::create("none".to_string());
        }
        self.notes[(n - 1) as usize].clone()
    }
    pub fn noteCount(&self) -> i64 {
        self.notes.len() as i64
    }
    /// ------------------------------------------------------------------
    /// The map, read
    /// ------------------------------------------------------------------
    /// One past the last source character any piece of this block's map covers.
    /// -1 for a block with no map. It is what `srcEnd` should be, and for the
    /// blocks that close on the line they opened on it is the only thing that
    /// knows: `finalizeTop` stamps `srcEnd` from the start of the line it is
    /// ON, which for a one-line block is the line it STARTED on.
    pub fn srcEndOfMap(&self) -> i64 {
        let n: i64 = self.mapLit.len() as i64;
        if n == 0 {
            return -1;
        }
        let mut best: i64 = -1;
        let mut i: i64 = 0;
        while i < n {
            let e: i64 = self.mapSrc[i as usize] + self.mapLen[i as usize];
            if e > best {
                best = e;
            }
            i += 1;
        }
        best
    }
    pub fn hasMap(&self) -> bool {
        !self.mapLit.is_empty()
    }
    /// Turn a map built against the RAW text into one against `literal`:
    /// `drop` characters came off the front, and what is left is `litLen` long.
    /// A piece that falls entirely outside is dropped; one that straddles an
    /// edge is clipped, which is what makes a trailing-newline trim and a
    /// reference definition at the head the same operation.
    pub fn shiftMap(&mut self, drop: i64, lit_len: i64) {
        let mut out_l: Vec<i64> = Vec::new();
        let mut out_s: Vec<i64> = Vec::new();
        let mut out_n: Vec<i64> = Vec::new();
        let n: i64 = self.mapLit.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut a: i64 = self.mapLit[i as usize] - drop;
            let mut s: i64 = self.mapSrc[i as usize];
            let mut __len: i64 = self.mapLen[i as usize];
            if a < 0 {
                // clipped at the front: the piece starts before literal[0]
                let cut: i64 = 0 - a;
                if cut < __len {
                    a = 0;
                    s += cut;
                    __len -= cut;
                } else {
                    __len = 0;
                }
            }
            if a + __len > lit_len {
                __len = lit_len - a;
            }
            if __len > 0 {
                out_l.push(a);
                out_s.push(s);
                out_n.push(__len);
            }
            i += 1;
        }
        self.mapLit = out_l.clone();
        self.mapSrc = out_s.clone();
        self.mapLen = out_n.clone();
    }
    pub fn clearMap(&mut self) {
        self.mapLit.clear();
        self.mapSrc.clear();
        self.mapLen.clear();
        self.rawLen = 0;
    }
    pub fn addMapPiece(&mut self, lit_off: i64, src_off: i64, __len: i64) {
        if __len <= 0 {
            return;
        }
        self.mapLit.push(lit_off);
        self.mapSrc.push(src_off);
        self.mapLen.push(__len);
    }
    /// Where literal[off] is in the document, or -1 when it is one of the
    /// characters the header says has no source.
    ///
    /// An offset one past the end of a piece belongs to that piece: a caret
    /// sits BETWEEN characters, and the position after the last character of a
    /// paragraph is a place a caret goes.
    pub fn srcOfLiteral(&self, off: i64) -> i64 {
        let n: i64 = self.mapLit.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let a: i64 = self.mapLit[i as usize];
            let __len: i64 = self.mapLen[i as usize];
            if off >= a && off < a + __len {
                return self.mapSrc[i as usize] + (off - a);
            }
            i += 1;
        }
        // one past the last piece's end
        if n > 0 {
            let last: i64 = n - 1;
            let la: i64 = self.mapLit[last as usize];
            let ll: i64 = self.mapLen[last as usize];
            if off == la + ll {
                return self.mapSrc[last as usize] + ll;
            }
        }
        -1
    }
    /// The inverse. A source offset inside a stripped marker is in no piece and
    /// answers -1.
    pub fn literalOfSrc(&self, src: i64) -> i64 {
        let n: i64 = self.mapLit.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let s: i64 = self.mapSrc[i as usize];
            let __len: i64 = self.mapLen[i as usize];
            if src >= s && src < s + __len {
                return self.mapLit[i as usize] + (src - s);
            }
            i += 1;
        }
        -1
    }
    /// The concatenated text of an inline subtree — what a heading anchor, an
    /// image's alt text and a footnote's back-reference are made of.
    pub fn plainText(&self) -> String {
        if self.kind == "text" {
            return self.literal.clone();
        }
        if self.kind == "code" {
            return self.literal.clone();
        }
        if self.kind == "math" {
            return self.literal.clone();
        }
        if self.kind == "math_block" {
            return self.literal.clone();
        }
        if self.kind == "softbreak" {
            return " ".to_string();
        }
        if self.kind == "linebreak" {
            return " ".to_string();
        }
        // `<br>` in a title breaks the line where it is drawn; in the
        // title's text it is a space, not two words run together
        // ("<small>3 / 4</small><br>Beta" is "3 / 4 Beta", not "3 / 4Beta")
        if self.kind == "html_inline" {
            if MdNode::isBrTag(&self.literal.clone()) {
                return " ".to_string();
            }
            return String::new();
        }
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = self.children.len() as i64;
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = self.children[i as usize].clone();
            let part: String = c.borrow().plainText();
            out = format!("{}{}", out, part);
            i += 1;
        }
        out
    }
    /// ------------------------------------------------------------------
    /// The XML the reference implementation prints, so the two can be diffed.
    /// ------------------------------------------------------------------
    pub fn toXml(&self) -> String {
        let mut out: String = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string();
        out = format!("{}<!DOCTYPE document SYSTEM \"CommonMark.dtd\">\n", out);
        let body: String = self.xmlNode(0);
        format!("{}{}", out, body)
    }
    pub fn xmlNode(&self, depth: i64) -> String {
        let pad: String = MdNode::spaces(depth);
        let name: String = self.kind.clone();
        let mut attrs: String = String::new();
        if self.kind == "heading" {
            attrs = format!("{} level=\"{}\"", attrs, self.level);
        }
        if self.kind == "list" {
            if self.listOrdered {
                attrs = format!("{} type=\"ordered\" start=\"{}\"", attrs, self.listStart);
                attrs = format!("{} delimiter=\"{}\"", attrs, if (self.listDelim == ".") { "period".to_string() } else { "paren".to_string() });
            } else {
                attrs = format!("{} type=\"bullet\"", attrs);
            }
            attrs = format!("{} tight=\"{}\"", attrs, if self.listTight { "true".to_string() } else { "false".to_string() });
        }
        if self.kind == "code_block" {
            if !self.info.is_empty() {
                attrs = format!("{} info=\"{}\"", attrs, MdNode::xmlEscape(&self.info));
            }
        }
        if self.kind == "link" {
            attrs = format!("{} destination=\"{}\"", attrs, MdNode::xmlEscape(&self.destination));
            attrs = format!("{} title=\"{}\"", attrs, MdNode::xmlEscape(&self.title));
        }
        if self.kind == "image" {
            attrs = format!("{} destination=\"{}\"", attrs, MdNode::xmlEscape(&self.destination));
            attrs = format!("{} title=\"{}\"", attrs, MdNode::xmlEscape(&self.title));
        }
        if self.kind == "footnote_ref" || self.kind == "footnote_def" {
            attrs = format!("{} label=\"{}\" number=\"{}\"", attrs, MdNode::xmlEscape(&self.info), self.noteNumber);
        }
        if self.kind == "container" {
            attrs = format!("{} name=\"{}\"", attrs, MdNode::xmlEscape(&self.info));
            if !self.title.is_empty() {
                attrs = format!("{} title=\"{}\"", attrs, MdNode::xmlEscape(&self.title));
            }
        }
        if self.kind == "dl" {
            attrs = format!("{} tight=\"{}\"", attrs, if self.listTight { "true".to_string() } else { "false".to_string() });
        }
        if self.kind == "abbr" {
            attrs = format!("{} title=\"{}\"", attrs, MdNode::xmlEscape(&self.title));
        }
        let mut has_lit: bool = false;
        if self.kind == "text" {
            has_lit = true;
        }
        if self.kind == "code" {
            has_lit = true;
        }
        if self.kind == "code_block" {
            has_lit = true;
        }
        if self.kind == "html_block" {
            has_lit = true;
        }
        if self.kind == "html_inline" {
            has_lit = true;
        }
        let kids: i64 = self.children.len() as i64;
        if kids == 0 {
            if has_lit {
                let esc: String = MdNode::xmlEscape(&self.literal.clone());
                return format!("{}<{}{}>{}</{}>\n", pad, name, attrs, esc, name);
            }
            return format!("{}<{}{} />\n", pad, name, attrs);
        }
        let mut out: String = format!("{}<{}{}>\n", pad, name, attrs);
        let mut i: i64 = 0;
        while i < kids {
            let mut c: Rc<RefCell<MdNode>> = self.children[i as usize].clone();
            let part: String = c.borrow().xmlNode(depth + 1);
            out = format!("{}{}", out, part);
            i += 1;
        }
        format!("{}{}</{}>\n", out, pad, name)
    }
}

/// The footnotes of one document, while it is being parsed.
///
/// The definitions are collected by the block parser (a reference on line 3
/// may name a definition on line 900, exactly as a link may), and the numbers
/// are handed out by the inline parser as it meets the references, in document
/// order — the order markdown-it numbers them in, so `[^second]` used before
/// `[^first]` is footnote 1. A label is matched as written: markdown-it does
/// not case-fold footnote labels the way CommonMark folds link labels.
#[derive(Clone)]
pub struct MdNotes {
    pub labelAt: HashMap<String,i64>,
    pub defs: Vec<Rc<RefCell<MdNode>>>,
    /// the numbered ones, footnote n at n - 1
    pub list: Vec<Rc<RefCell<MdNode>>>,
}

impl MdNotes {
    pub fn new() -> Self {
        Self {
            labelAt: HashMap::new(),
            defs: Vec::new(),
            list: Vec::new(),
        }
    }
    /// The first definition of a label is the one that counts, as with links.
    pub fn define(&mut self, label: &str, node: Rc<RefCell<MdNode>>) {
        if self.labelAt.contains_key(label) {
            return;
        }
        self.labelAt.insert(label.to_string(), self.defs.len() as i64);
        self.defs.push(node.clone());
    }
    pub fn hasLabel(&self, label: &str) -> bool {
        self.labelAt.contains_key(label)
    }
    pub fn defOf(&self, label: &str) -> Rc<RefCell<MdNode>> {
        if false == self.labelAt.contains_key(label) {
            return MdNode::create("none".to_string());
        }
        let i: i64 = self.labelAt.get(label).cloned().unwrap();
        self.defs[i as usize].clone()
    }
    /// A footnote gets its number when it is first referenced.
    pub fn number(&mut self, fd: Rc<RefCell<MdNode>>) -> i64 {
        if fd.borrow().noteNumber == 0 {
            self.list.push(fd.clone());
            fd.borrow_mut().noteNumber = self.list.len() as i64;
        }
        return fd.borrow().noteNumber;
    }
    /// `ref` refers to `fd`: its number, and which reference it is.
    pub fn refer(&mut self, r#ref: &Rc<RefCell<MdNode>>, fd: Rc<RefCell<MdNode>>) {
        r#ref.borrow_mut().noteNumber = self.number(fd.clone());
        r#ref.borrow_mut().noteSub = fd.borrow().noteRefs;
        let noteRefs_v = fd.borrow().noteRefs + 1;
        fd.borrow_mut().noteRefs = noteRefs_v;
    }
}

/// A link reference definition, collected while the blocks are being parsed and
/// consulted while the inlines are — because a link on line 3 may resolve
/// against a definition on line 900.
#[derive(Clone)]
pub struct MdLinkRef {
    pub label: String,
    pub destination: String,
    pub title: String,
}

impl MdLinkRef {
    pub fn new() -> Self {
        Self {
            label: String::new(),
            destination: String::new(),
            title: String::new(),
        }
    }
    pub fn create(label: String, dest: String, title: String) -> Rc<RefCell<MdLinkRef>> {
        let mut r: Rc<RefCell<MdLinkRef>> = Rc::new(RefCell::new(MdLinkRef::new()));
        r.borrow_mut().label = label.clone();
        r.borrow_mut().destination = dest.clone();
        r.borrow_mut().title = title.clone();
        r
    }
}
