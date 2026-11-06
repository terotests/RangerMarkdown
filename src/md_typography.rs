// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::md_attrs::MdAttrs;
use crate::md_char::MdChar;
use crate::md_entities::MdEntities;
use crate::md_front_matter::MdMeta;
use crate::md_node::MdNode;
use crate::rgr::MdText;
use crate::rgr::{MdEmoji};

/// ============================================================================
/// MdTypography.rgr — what markdown-it does to a line's TEXT after parsing it.
/// ============================================================================
///
///   (c) (tm) (r) +-  ...  ?.....  !!!!!!  ,,  --  ---      © ™ ® ± … ?.. !!! , – —
///   "Smartypants" and 'single quotes', it's                “…” ‘…’ ’
///   :wink: :cry:  :-)  8-)  ;)                             😉 😢 😃 😎 😉
///   HTML, with `*[HTML]: Hyper Text Markup Language`      <abbr title="…">HTML</abbr>
///
/// markdown-it's `typographer` (its core rules `replacements` and
/// `smartquotes`), markdown-it-emoji and markdown-it-abbr. All four are core
/// rules there — they run after the inline pass, over the text tokens it left —
/// and they are a pass after the inline parse here for the same reason: a code
/// span, a link's address, an autolink and a piece of raw HTML have to have
/// taken their characters first, because none of those may be touched. Neither
/// may a bare address in the text (`https://a--b.example`) or an attribute
/// block (`{data-x="y"}`) left in a line; those are fenced off before anything
/// else looks.
///
/// **What a replacement does to the source map.** A text node maps character
/// for character to the file, and the layout's caret arithmetic depends on it.
/// `©` is one character standing for three, so it cannot sit in such a node.
/// Every replacement that changes the length becomes a text node of its own,
/// flagged `subst`, whose span is the WHOLE source it came from — a click on
/// `©` lands on `(c)`, before it or after it, and never in its middle. A curly
/// quote is one character for one, so it stays where it was and the map stays
/// exact.
///
/// **The unit of work** is a GROUP: a run of sibling text nodes that are
/// verbatim (their span is as long as their text) and contiguous in the file —
/// what markdown-it would have as one text token. CommonMark's parser splits
/// text at every `!` and `[`, and `!!!!!!` has to be seen as one run. Inside a
/// group the text is a list of CELLS, one per character to begin with, each
/// carrying its source span; a replacement merges cells, an abbreviation tags
/// them, and at the end each group is written back as nodes — unchanged groups
/// are left exactly as the parser made them.
/// ============================================================================
/// What a document reads beyond CommonMark, and what it defined.
#[derive(Clone)]
pub struct MdInlineOptions {
    /// markdown-it's plugins: sub, sup, ins, mark, emoji, abbr
    pub extensions: bool,
    /// markdown-it's `typographer`: the replacements and the quotes
    pub typographer: bool,
    /// set by `commonMarkOnly`: the front matter may not switch anything on
    pub locked: bool,
    /// the quotes, as code points: double open, double close, single open,
    /// single close — English by default, ” ” ’ ’ for Finnish and Swedish
    pub dq1: i64,
    pub dq2: i64,
    pub sq1: i64,
    pub sq2: i64,
    /// abbreviations, longest label first (which is the order they are
    /// tried in), and the title of each
    pub abbrLabels: Vec<String>,
    pub abbrTitles: HashMap<String,String>,
}

impl MdInlineOptions {
    pub fn new() -> Self {
        Self {
            extensions: true,
            typographer: true,
            locked: false,
            dq1: 8220,
            dq2: 8221,
            sq1: 8216,
            sq2: 8217,
            abbrLabels: Vec::new(),
            abbrTitles: HashMap::new(),
        }
    }
    pub fn commonMarkOnly(&mut self) {
        self.extensions = false;
        self.typographer = false;
        self.locked = true;
    }
    /// `typographer: false` and `lang: fi` in the front matter.
    pub fn readMeta(&mut self, meta: &Rc<RefCell<MdMeta>>) {
        if self.locked {
            return;
        }
        self.typographer = meta.borrow().flag("typographer", true);
        let lang: String = MdChar::lower(&meta.borrow().get("lang", "").trim().to_string());
        let mut two: String = lang.clone();
        if (MdText::str_len(&lang) as i64) > 2 {
            let sep: i64 = MdText::char_at(&lang, 2);
            if sep == 45 || sep == 95 {
                two = MdText::substr(&lang, 0, 2);
            }
        }
        if two == "fi" || two == "sv" {
            self.dq1 = 8221;
            self.dq2 = 8221;
            self.sq1 = 8217;
            self.sq2 = 8217;
        }
    }
    /// Everything above as one string: two documents read with the same one
    /// read the same block source to the same nodes.
    pub fn signature(&self) -> String {
        let mut sig: String = "x".to_string();
        if self.extensions {
            sig = "e".to_string();
        }
        if self.typographer {
            sig = format!("{}t", sig);
        }
        sig = format!("{}:{},{},{},", sig, self.dq1, self.dq2, self.sq1);
        sig = format!("{}{}", sig, self.sq2);
        let mut i: i64 = 0;
        let n: i64 = self.abbrLabels.len() as i64;
        while i < n {
            let l: String = self.abbrLabels[i as usize].clone();
            let t: String = if self.abbrTitles.get(&l)
                .cloned()
                .is_some() { self.abbrTitles.get(&l).cloned().unwrap() } else { String::new() };
            sig = format!("{}|{}:{}={}", sig, (MdText::str_len(&l) as i64), l, t);
            i += 1;
        }
        sig
    }
    pub fn defineAbbreviation(&mut self, label: &str, title: &str) {
        if self.abbrTitles.contains_key(label) {
            return;
        }
        self.abbrTitles.insert(label.to_string(), title.to_string());
        let __len: i64 = MdText::str_len(&label) as i64;
        let mut out: Vec<String> = Vec::new();
        let mut placed: bool = false;
        let mut i: i64 = 0;
        let n: i64 = self.abbrLabels.len() as i64;
        while i < n {
            let l: String = self.abbrLabels[i as usize].clone();
            if false == placed && (MdText::str_len(&l) as i64) < __len {
                out.push(label.to_string());
                placed = true;
            }
            out.push(l.clone());
            i += 1;
        }
        if false == placed {
            out.push(label.to_string());
        }
        self.abbrLabels = out.clone();
    }
    /// One table for the process: it is 1900 names, and a document is parsed
    /// again on every keystroke.
    fn emoji(&self) -> Rc<RefCell<MdEmoji>> {
        MdEmoji::shared()
    }
    pub fn abbrTitle(&self, label: &str) -> String {
        if self.abbrTitles.get(label)
            .cloned()
            .is_some() { self.abbrTitles.get(label).cloned().unwrap() } else { String::new() }.clone()
    }
    pub fn hasAbbreviations(&self) -> bool {
        !self.abbrLabels.is_empty()
    }
}

/// One character of a group, or what a replacement made of several.
#[derive(Clone)]
pub struct MdTypoCell {
    pub text: String,
    /// its source, [a, b)
    pub a: i64,
    pub b: i64,
    /// 0 a character as typed, 1 an emoji, 2 a typographic replacement,
    /// 3 a character no rule may touch (an address, an attribute block)
    pub kind: i64,
    /// the abbreviation it is part of, an index into the group's titles, or -1
    pub abbr: i64,
}

impl MdTypoCell {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            a: 0,
            b: 0,
            kind: 0,
            abbr: -1,
        }
    }
    pub fn make(text: String, a: i64, b: i64, kind: i64, abbr: i64) -> Rc<RefCell<MdTypoCell>> {
        let mut c: Rc<RefCell<MdTypoCell>> = Rc::new(RefCell::new(MdTypoCell::new()));
        c.borrow_mut().text = text.clone();
        c.borrow_mut().a = a;
        c.borrow_mut().b = b;
        c.borrow_mut().kind = kind;
        c.borrow_mut().abbr = abbr;
        c
    }
    pub fn first(&self) -> i64 {
        if self.text.is_empty() {
            return 32;
        }
        MdText::char_at(&self.text, 0)
    }
    pub fn last(&self) -> i64 {
        let n: i64 = MdText::str_len(&self.text) as i64;
        if n == 0 {
            return 32;
        }
        MdText::char_at(&self.text, n - 1)
    }
}

/// A run of sibling text nodes markdown-it would have read as one text token.
#[derive(Clone)]
pub struct MdTypoGroup {
    pub parent: Rc<RefCell<MdNode>>,
    pub first: i64,
    pub count: i64,
    pub text: String,
    pub base: i64,
    pub level: i64,
    pub cells: Vec<Rc<RefCell<MdTypoCell>>>,
    pub hasCells: bool,
    pub changed: bool,
    pub titles: Vec<String>,
}

impl MdTypoGroup {
    pub fn new() -> Self {
        Self {
            parent: Rc::new(RefCell::new(MdNode::new())),
            first: 0,
            count: 0,
            text: String::new(),
            base: 0,
            level: 0,
            cells: Vec::new(),
            hasCells: false,
            changed: false,
            titles: Vec::new(),
        }
    }
    pub fn ensureCells(&mut self) {
        if self.hasCells {
            return;
        }
        self.hasCells = true;
        let n: i64 = MdText::str_len(&self.text) as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut _tmp_89 = MdTypoCell::make(
                MdText::substr(&self.text, i, i + 1),
                self.base + i,
                (self.base + i) + 1,
                0,
                -1
            );
            self.cells.push(_tmp_89);
            i += 1;
        }
    }
    pub fn cellCount(&self) -> i64 {
        self.cells.len() as i64
    }
    pub fn cellAt(&self, i: i64) -> Rc<RefCell<MdTypoCell>> {
        self.cells[i as usize].clone()
    }
}

/// One step of the quote pass's walk: a text a quote may be in, a piece of
/// content that is only context, a line break, or a tag's edge.
#[derive(Clone)]
pub struct MdQuoteItem {
    /// 0 text, 1 other content, 2 break, 3 tag
    pub kind: i64,
    pub level: i64,
    pub text: String,
    pub cells: Vec<Rc<RefCell<MdTypoCell>>>,
    /// a group that no rule but the quotes touched has no cells: its quotes
    /// are put straight into its nodes, at offsets into `text`
    pub direct: bool,
    pub group: Rc<RefCell<MdTypoGroup>>,
}

impl MdQuoteItem {
    pub fn new() -> Self {
        Self {
            kind: 0,
            level: 0,
            text: String::new(),
            cells: Vec::new(),
            direct: false,
            group: Rc::new(RefCell::new(MdTypoGroup::new())),
        }
    }
    pub fn make(kind: i64, level: i64, text: String) -> Rc<RefCell<MdQuoteItem>> {
        let mut q: Rc<RefCell<MdQuoteItem>> = Rc::new(RefCell::new(MdQuoteItem::new()));
        q.borrow_mut().kind = kind;
        q.borrow_mut().level = level;
        q.borrow_mut().text = text.clone();
        q
    }
}

/// A quote waiting for its partner.
#[derive(Clone)]
pub struct MdQuoteOpen {
    pub item: i64,
    pub pos: i64,
    pub single: bool,
    pub level: i64,
}

impl MdQuoteOpen {
    pub fn new() -> Self {
        Self {
            item: 0,
            pos: 0,
            single: false,
            level: 0,
        }
    }
}

#[derive(Clone)]
pub struct MdTypography {
    pub opts: Rc<RefCell<MdInlineOptions>>,
    pub groups: Vec<Rc<RefCell<MdTypoGroup>>>,
    /// the walk in document order: a group (by index), or a quote item
    pub walkGroup: Vec<i64>,
    pub walkItem: Vec<Rc<RefCell<MdQuoteItem>>>,
    /// the walk is for the quotes, and only kept when there are any
    pub walking: bool,
}

impl MdTypography {
    pub fn new() -> Self {
        Self {
            opts: Rc::new(RefCell::new(MdInlineOptions::new())),
            groups: Vec::new(),
            walkGroup: Vec::new(),
            walkItem: Vec::new(),
            walking: true,
        }
    }
    /// Everything, on one block's inlines.
    /// `raw` is the text the block's inlines were parsed from: a block with
    /// nothing in it any rule could take is not walked at all, which is most
    /// of them.
    pub fn apply(block: Rc<RefCell<MdNode>>, raw: &str, o: Rc<RefCell<MdInlineOptions>>) {
        if false == o.borrow().extensions && false == o.borrow().typographer {
            return;
        }
        if o.borrow().typographer && MdTypography::hasQuote(raw) {
        } else {
            if false == MdTypography::wantsText(raw, &o) {
                return;
            }
        }
        let mut t: Rc<RefCell<MdTypography>> = Rc::new(RefCell::new(MdTypography::new()));
        t.borrow_mut().opts = o.clone();
        t.borrow_mut().walking = o.borrow().typographer && MdTypography::hasQuote(raw);
        t.borrow_mut().run(block.clone());
    }
    /// ----------------------------------------------------------------------
    /// The walk
    /// ----------------------------------------------------------------------
    /// A text node whose span is its text, character for character.
    pub fn verbatim(n: &Rc<RefCell<MdNode>>) -> bool {
        if n.borrow().kind != "text" {
            return false;
        }
        if n.borrow().subst {
            return false;
        }
        if n.borrow().srcStart < 0 {
            return false;
        }
        return n.borrow().srcEnd - n.borrow().srcStart == (MdText::str_len(&n.borrow().literal) as i64);
    }
    pub fn hasQuote(text: &str) -> bool {
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&text, i);
            if c == 34 || c == 39 {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn wantsText(text: &str, opts: &Rc<RefCell<MdInlineOptions>>) -> bool {
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&text, i);
            // letters and spaces are most of any text and start nothing,
            // but for the B, x and X of an emoticon
            let mut look: bool = c < 65 && c != 32;
            if ((c == 66 || c == 120) || c == 88) || c == 8230 {
                look = true;
            }
            if (c >= 91 && c <= 96) || (c >= 123 && c <= 127) {
                look = true;
            }
            if look {
                let mut d: i64 = -1;
                if i + 1 < n {
                    d = MdText::char_at(&text, i + 1);
                }
                if opts.borrow().typographer {
                    if c == 40 {
                        let lc: i64 = MdChar::foldCase(d);
                        if (lc == 99 || lc == 114) || lc == 116 {
                            return true;
                        }
                    }
                    if c == 43 && d == 45 {
                        return true;
                    }
                    if c == 46 && d == 46 {
                        return true;
                    }
                    if c == 44 && d == 44 {
                        return true;
                    }
                    if c == 45 && d == 45 {
                        return true;
                    }
                    if (c == 63 || c == 33) && (d == 63 || d == 33) {
                        return true;
                    }
                    if c == 8230 {
                        return true;
                    }
                }
                if opts.borrow().extensions {
                    if c == 58 && d >= 0 {
                        // a shortcode, or an emoticon's eyes
                        if MdTypography::isNameChar(d) {
                            return true;
                        }
                        if MdTypography::emoticonMouth(d) {
                            return true;
                        }
                    }
                    if c == 59 && ((d == 41 || d == 40) || d == 45) {
                        return true;
                    }
                    if c == 60 && ((d == 51 || d == 47) || d == 92) {
                        return true;
                    }
                    if d == 45 && (((c == 56 || c == 66) || c == 120) || c == 88) {
                        return true;
                    }
                }
            }
            i += 1;
        }
        if opts.borrow().extensions && opts.borrow().hasAbbreviations() {
            let mut k: i64 = 0;
            let m: i64 = opts.borrow().abbrLabels.len() as i64;
            while k < m {
                if MdTypography::contains(text, &opts.borrow().abbrLabels[k as usize].clone()) {
                    return true;
                }
                k += 1;
            }
        }
        false
    }
    /// What may follow the `:` of an emoticon: `:)` `:-)` `:'(` `:,)` `:"(`
    /// `:|` `:*` `:/` `:@` `:$` and the letters, which isNameChar has.
    pub fn emoticonMouth(d: i64) -> bool {
        if (d == 41 || d == 40) || (d == 45 || d == 39) {
            return true;
        }
        if (d == 44 || d == 34) || (d == 124 || d == 42) {
            return true;
        }
        (d == 47 || d == 64) || d == 36
    }
    /// Could there be an address or an attribute block in this text at all?
    pub fn mayFence(t: &str) -> bool {
        let n: i64 = MdText::str_len(&t) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&t, i);
            if c == 123 {
                return true;
            }
            if c == 58 && i + 2 < n {
                if MdText::char_at(&t, i + 1) == 47 && MdText::char_at(&t, i + 2) == 47 {
                    return true;
                }
            }
            if (c == 46 && i >= 3) && MdChar::foldCase(MdText::char_at(&t, i - 1)) == 119 {
                if MdChar::lower(&MdText::substr(&t, i - 3, i)) == "www" {
                    return true;
                }
            }
            i += 1;
        }
        false
    }
    pub fn contains(v: &str, needle: &str) -> bool {
        let n: i64 = MdText::str_len(&needle) as i64;
        let m: i64 = MdText::str_len(&v) as i64;
        if n == 0 {
            return false;
        }
        let mut i: i64 = 0;
        while i + n <= m {
            if MdText::substr(&v, i, i + n) == needle {
                return true;
            }
            i += 1;
        }
        false
    }
    /// Where an address that starts at `i` ends, or -1.
    pub fn addressEnd(t: &str, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&t) as i64;
        if i > 0 {
            if MdChar::isAlnum(MdText::char_at(&t, i - 1)) {
                return -1;
            }
        }
        let mut j: i64 = i;
        let mut scheme: bool = false;
        while j < n && MdChar::isLetter(MdText::char_at(&t, j)) {
            j += 1;
        }
        if j > i && j + 2 < n {
            if MdText::substr(&t, j, j + 3) == "://" {
                scheme = true;
            }
        }
        if false == scheme {
            if i + 4 > n {
                return -1;
            }
            if MdChar::lower(&MdText::substr(&t, i, i + 4)) != "www." {
                return -1;
            }
        }
        let mut k: i64 = i;
        while k < n {
            if MdChar::isUnicodeWhitespace(MdText::char_at(&t, k)) {
                return k;
            }
            k += 1;
        }
        n
    }
    pub fn attrEnd(t: &str, i: i64) -> i64 {
        if MdText::char_at(&t, i) != 123 {
            return -1;
        }
        let n: i64 = MdText::str_len(&t) as i64;
        let mut k: i64 = i + 1;
        while k < n {
            let c: i64 = MdText::char_at(&t, k);
            if c == 125 {
                if MdAttrs::isBlock(&MdText::substr(&t, i, k + 1)) {
                    return k + 1;
                }
                return -1;
            }
            if c == 123 {
                return -1;
            }
            k += 1;
        }
        -1
    }
    /// ----------------------------------------------------------------------
    /// Emoji (markdown-it-emoji)
    /// ----------------------------------------------------------------------
    ///
    /// `:name:` for any name in the table; an emoticon only where it is not
    /// glued to a letter on either side — so `http://` keeps its `:/`.
    pub fn isZPCc(c: i64) -> bool {
        if c < 32 {
            return true;
        }
        if c == 127 {
            return true;
        }
        if MdChar::isUnicodeWhitespace(c) {
            return true;
        }
        // Unicode's punctuation, not its symbols: `$+<=>^`|~` are symbols
        if ((c == 36 || c == 43) || (c == 60 || c == 61)) || c == 62 {
            return false;
        }
        if (c == 94 || c == 96) || (c == 124 || c == 126) {
            return false;
        }
        MdChar::isUnicodePunct(c)
    }
    /// The characters a shortcode or an emoticon can begin with.
    pub fn emojiStart(c: i64) -> bool {
        if (c == 58 || c == 59) || (c == 60 || c == 62) {
            return true;
        }
        if (c == 93 || c == 44) || (c == 56 || c == 66) {
            return true;
        }
        if (c == 120 || c == 88) || (c == 111 || c == 79) {
            return true;
        }
        c == 48
    }
    pub fn isNameChar(c: i64) -> bool {
        if MdChar::isAlnum(c) {
            return true;
        }
        if c == 95 || c == 43 {
            return true;
        }
        c == 45
    }
    /// ----------------------------------------------------------------------
    /// Abbreviations (markdown-it-abbr)
    /// ----------------------------------------------------------------------
    ///
    /// A label as a whole word: at the edge of the text or beside a space,
    /// punctuation, or one of `$+<=>^`|~`. `xxxHTMLyyy` is not an HTML.
    pub fn isAbbrEdge(c: i64) -> bool {
        if c < 0 {
            return true;
        }
        if MdChar::isUnicodeWhitespace(c) {
            return true;
        }
        MdChar::isUnicodePunct(c)
    }
    /// `?` or `!` twice running: where a run of four could be
    pub fn marksIn(t: &str) -> bool {
        let n: i64 = MdText::str_len(&t) as i64;
        let mut i: i64 = 1;
        while i < n {
            let a: i64 = MdText::char_at(&t, i - 1);
            let b: i64 = MdText::char_at(&t, i);
            if (a == 63 || a == 33) && (b == 63 || b == 33) {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn textOf(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = s.len() as i64;
        while i < n {
            let mut c: Rc<RefCell<MdTypoCell>> = s[i as usize].clone();
            out = format!("{}{}", out, c.borrow().text);
            i += 1;
        }
        out
    }
    pub fn is(s: &Vec<Rc<RefCell<MdTypoCell>>>, i: i64, ch: &str) -> bool {
        if i < 0 {
            return false;
        }
        if i >= (s.len() as i64) {
            return false;
        }
        let mut c: Rc<RefCell<MdTypoCell>> = s[i as usize].clone();
        if c.borrow().kind != 0 {
            return false;
        }
        return c.borrow().text == ch;
    }
    pub fn isFold(s: &Vec<Rc<RefCell<MdTypoCell>>>, i: i64, lower: &str) -> bool {
        if i < 0 {
            return false;
        }
        if i >= (s.len() as i64) {
            return false;
        }
        let mut c: Rc<RefCell<MdTypoCell>> = s[i as usize].clone();
        if c.borrow().kind != 0 {
            return false;
        }
        return MdChar::lower(&c.borrow().text.clone()) == lower;
    }
    pub fn merged(s: &Vec<Rc<RefCell<MdTypoCell>>>, i: i64, __len: i64, text: &str) -> Rc<RefCell<MdTypoCell>> {
        let mut a: Rc<RefCell<MdTypoCell>> = s[i as usize].clone();
        let mut z: Rc<RefCell<MdTypoCell>> = s[((i + __len) - 1) as usize].clone();
        let mut _tmp_92 = a.borrow().a.clone();
        let mut _tmp_93 = z.borrow().b.clone();
        return MdTypoCell::make(text.to_string(), _tmp_92, _tmp_93, 2, a.borrow().abbr.clone());
    }
    /// (c) (C) → ©, (r) → ®, (tm) → ™
    pub fn scoped(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut took: i64 = 0;
            if MdTypography::is(s, i, "(") {
                if MdTypography::isFold(s, i + 1, "c") && MdTypography::is(s, i + 2, ")") {
                    out.push(MdTypography::merged(
                        s,
                        i,
                        3,
                        &char::from_u32(169 as u32).unwrap_or('\0').to_string()
                    ));
                    took = 3;
                }
                if MdTypography::isFold(s, i + 1, "r") && MdTypography::is(s, i + 2, ")") {
                    out.push(MdTypography::merged(
                        s,
                        i,
                        3,
                        &char::from_u32(174 as u32).unwrap_or('\0').to_string()
                    ));
                    took = 3;
                }
                if MdTypography::isFold(s, i + 1, "t") && MdTypography::isFold(s, i + 2, "m") {
                    if MdTypography::is(s, i + 3, ")") {
                        out.push(MdTypography::merged(
                            s,
                            i,
                            4,
                            &char::from_u32(8482 as u32).unwrap_or('\0').to_string()
                        ));
                        took = 4;
                    }
                }
            }
            if took == 0 {
                out.push(s[i as usize].clone());
                i += 1;
            } else {
                i += took;
            }
        }
        out
    }
    pub fn pair(s: &Vec<Rc<RefCell<MdTypoCell>>>, x: &str, y: &str, to: &str) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            if MdTypography::is(s, i, x) && MdTypography::is(s, i + 1, y) {
                out.push(MdTypography::merged(s, i, 2, to));
                i += 2;
            } else {
                out.push(s[i as usize].clone());
                i += 1;
            }
        }
        out
    }
    /// How many cells from `i` on are `ch`, as typed.
    pub fn runOf(s: &Vec<Rc<RefCell<MdTypoCell>>>, i: i64, ch: &str) -> i64 {
        let n: i64 = s.len() as i64;
        let mut j: i64 = i;
        while j < n && MdTypography::is(s, j, ch) {
            j += 1;
        }
        j - i
    }
    /// `..` and longer → …
    pub fn dots(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let r: i64 = MdTypography::runOf(s, i, ".");
            if r >= 2 {
                out.push(MdTypography::merged(
                    s,
                    i,
                    r,
                    &char::from_u32(8230 as u32).unwrap_or('\0').to_string()
                ));
                i += r;
            } else {
                out.push(s[i as usize].clone());
                i += 1;
            }
        }
        out
    }
    /// …but `?…` and `!…` are `?..` and `!..`
    pub fn dotsAfterMark(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let ell: String = char::from_u32(8230 as u32).unwrap_or('\0').to_string();
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdTypoCell>> = s[i as usize].clone();
            let mut done: bool = false;
            if i > 0 && c.borrow().text == ell {
                if MdTypography::is(s, i - 1, "?") || MdTypography::is(s, i - 1, "!") {
                    let mut _tmp_95 = c.borrow().a.clone();
                    let mut _tmp_96 = c.borrow().b.clone();
                    out.push(MdTypoCell::make("..".to_string(), _tmp_95, _tmp_96, 2, c.borrow().abbr.clone()));
                    done = true;
                }
            }
            if false == done {
                out.push(c.clone());
            }
            i += 1;
        }
        out
    }
    /// four or more of `?` and `!` → three of the last
    pub fn marks(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut j: i64 = i;
            while j < n && (MdTypography::is(s, j, "?") || MdTypography::is(s, j, "!")) {
                j += 1;
            }
            if j - i >= 4 {
                let mut lastc: Rc<RefCell<MdTypoCell>> = s[(j - 1) as usize].clone();
                let three: String = format!("{}{}{}", lastc.borrow().text, lastc.borrow().text, lastc.borrow().text);
                out.push(MdTypography::merged(s, i, j - i, &three));
                i = j;
            } else {
                if j == i {
                    j = i + 1;
                }
                while i < j {
                    out.push(s[i as usize].clone());
                    i += 1;
                }
            }
        }
        out
    }
    pub fn commas(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let r: i64 = MdTypography::runOf(s, i, ",");
            if r >= 2 {
                out.push(MdTypography::merged(s, i, r, ","));
                i += r;
            } else {
                out.push(s[i as usize].clone());
                i += 1;
            }
        }
        out
    }
    pub fn lastBefore(s: &Vec<Rc<RefCell<MdTypoCell>>>, i: i64) -> i64 {
        if i <= 0 {
            return -1;
        }
        let mut c: Rc<RefCell<MdTypoCell>> = s[(i - 1) as usize].clone();
        return c.borrow().last();
    }
    pub fn firstAt(s: &Vec<Rc<RefCell<MdTypoCell>>>, i: i64) -> i64 {
        if i >= (s.len() as i64) {
            return -1;
        }
        let mut c: Rc<RefCell<MdTypoCell>> = s[i as usize].clone();
        return c.borrow().first();
    }
    /// `---` with no dash either side → —
    pub fn emDash(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut ok: bool = false;
            if MdTypography::runOf(s, i, "-") >= 3 {
                let before: i64 = MdTypography::lastBefore(s, i);
                let after: i64 = MdTypography::firstAt(s, i + 3);
                ok = before != 45 && after != 45;
            }
            if ok {
                out.push(MdTypography::merged(
                    s,
                    i,
                    3,
                    &char::from_u32(8212 as u32).unwrap_or('\0').to_string()
                ));
                i += 3;
            } else {
                out.push(s[i as usize].clone());
                i += 1;
            }
        }
        out
    }
    pub fn spaceOrEdge(c: i64) -> bool {
        if c < 0 {
            return true;
        }
        MdChar::isUnicodeWhitespace(c)
    }
    /// ` -- ` → ` – `
    pub fn enDashSpaced(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut ok: bool = false;
            if MdTypography::is(s, i, "-") && MdTypography::is(s, i + 1, "-") {
                let before: i64 = MdTypography::lastBefore(s, i);
                let after: i64 = MdTypography::firstAt(s, i + 2);
                ok = MdTypography::spaceOrEdge(before) && MdTypography::spaceOrEdge(after);
            }
            if ok {
                out.push(MdTypography::merged(
                    s,
                    i,
                    2,
                    &char::from_u32(8211 as u32).unwrap_or('\0').to_string()
                ));
                i += 2;
            } else {
                out.push(s[i as usize].clone());
                i += 1;
            }
        }
        out
    }
    /// `a--b` → `a–b`: neither side a dash or a space
    pub fn enDashJoined(s: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut ok: bool = false;
            if MdTypography::is(s, i, "-") && MdTypography::is(s, i + 1, "-") {
                let before: i64 = MdTypography::lastBefore(s, i);
                let after: i64 = MdTypography::firstAt(s, i + 2);
                let ok_b: bool = before < 0 || before != 45 && false == MdChar::isUnicodeWhitespace(before);
                let ok_a: bool = after < 0 || after != 45 && false == MdChar::isUnicodeWhitespace(after);
                ok = ok_b && ok_a;
            }
            if ok {
                out.push(MdTypography::merged(
                    s,
                    i,
                    2,
                    &char::from_u32(8211 as u32).unwrap_or('\0').to_string()
                ));
                i += 2;
            } else {
                out.push(s[i as usize].clone());
                i += 1;
            }
        }
        out
    }
    pub fn isPunct(c: i64) -> bool {
        MdChar::isUnicodePunct(c)
    }
    pub fn isSpace(c: i64) -> bool {
        MdChar::isUnicodeWhitespace(c)
    }
    /// Is offset `at` inside a bare address or an attribute block?
    pub fn isFenced(t: &str, at: i64) -> bool {
        if false == MdTypography::mayFence(t) {
            return false;
        }
        let mut i: i64 = 0;
        while i <= at {
            let mut stop: i64 = MdTypography::addressEnd(t, i);
            if stop < 0 {
                stop = MdTypography::attrEnd(t, i);
            }
            if stop > i {
                if at < stop {
                    return true;
                }
                i = stop;
            } else {
                i += 1;
            }
        }
        false
    }
    pub fn abbrOf(g: &Rc<RefCell<MdTypoGroup>>, i: i64) -> i64 {
        let mut c: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i);
        return c.borrow().abbr;
    }
    pub fn run(&mut self, block: Rc<RefCell<MdNode>>) {
        self.collect(block.clone(), 0);
        let n: i64 = self.groups.len() as i64;
        if n == 0 {
            return;
        }
        let mut quotes: bool = false;
        if false == self.walking {
            quotes = true;
        }
        let mut i: i64 = 0;
        while i < n {
            let mut g: Rc<RefCell<MdTypoGroup>> = self.groups[i as usize].clone();
            if self.opts.borrow().typographer && false == quotes {
                quotes = MdTypography::hasQuote(&g.borrow().text);
            }
            if self.wants(&g.borrow().text.clone()) {
                g.borrow_mut().ensureCells();
                self.protect(&g);
                if self.opts.borrow().extensions {
                    self.emojiPass(&g);
                    self.abbrPass(&g);
                }
                if self.opts.borrow().typographer {
                    self.replacePass(&g);
                }
            }
            i += 1;
        }
        if self.walking && quotes {
            self.quotePass();
        }
        // written back last to first, so a group's child indices are still
        // the ones it was found at
        let mut j: i64 = n - 1;
        while j >= 0 {
            let mut g2: Rc<RefCell<MdTypoGroup>> = self.groups[j as usize].clone();
            if g2.borrow().changed {
                self.rebuild(&g2);
            }
            j -= 1;
        }
    }
    pub fn pushItem(&mut self, q: Rc<RefCell<MdQuoteItem>>) {
        if false == self.walking {
            return;
        }
        self.walkGroup.push(-1);
        self.walkItem.push(q.clone());
    }
    pub fn collect(&mut self, parent: Rc<RefCell<MdNode>>, level: i64) {
        let mut i: i64 = 0;
        let n: i64 = parent.borrow().childCount();
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = parent.borrow().childAt(i);
            if MdTypography::verbatim(&c) {
                let mut g: Rc<RefCell<MdTypoGroup>> = Rc::new(RefCell::new(MdTypoGroup::new()));
                g.borrow_mut().parent = parent.clone();
                g.borrow_mut().first = i;
                g.borrow_mut().level = level;
                g.borrow_mut().base = c.borrow().srcStart;
                let mut text: String = c.borrow().literal.clone();
                let mut end: i64 = c.borrow().srcEnd;
                let mut j: i64 = i + 1;
                let mut going: bool = true;
                while going && j < n {
                    let mut d: Rc<RefCell<MdNode>> = parent.borrow().childAt(j);
                    if MdTypography::verbatim(&d) && d.borrow().srcStart == end {
                        text = format!("{}{}", text, d.borrow().literal);
                        end = d.borrow().srcEnd;
                        j += 1;
                    } else {
                        going = false;
                    }
                }
                g.borrow_mut().count = j - i;
                g.borrow_mut().text = text.clone();
                self.groups.push(g.clone());
                if self.walking {
                    self.walkGroup.push((self.groups.len() as i64) - 1);
                    self.walkItem.push(MdQuoteItem::make(0, level, String::new()));
                }
                i = j;
            } else {
                self.collectOne(c.clone(), level);
                i += 1;
            }
        }
    }
    pub fn collectOne(&mut self, c: Rc<RefCell<MdNode>>, level: i64) {
        let k: String = c.borrow().kind.clone();
        if k == "softbreak" || k == "linebreak" {
            self.pushItem(MdQuoteItem::make(2, level, String::new()).clone());
            return;
        }
        let mut container: bool = false;
        if k == "emph" || k == "strong" {
            container = true;
        }
        if k == "strike" || k == "ins" {
            container = true;
        }
        if k == "mark" || k == "sub" {
            container = true;
        }
        if k == "sup" || k == "abbr" {
            container = true;
        }
        // a link's text, but not an autolink's, which IS its address
        if k == "link" && c.borrow().info != "auto" {
            container = true;
        }
        if container {
            self.pushItem(MdQuoteItem::make(3, level, String::new()).clone());
            self.collect(c.clone(), level + 1);
            self.pushItem(MdQuoteItem::make(3, level, String::new()).clone());
            return;
        }
        // code, math, raw HTML, an entity, an escape, a picture's alt text:
        // context for a quote beside it, and nothing to change
        if false == self.walking {
            return;
        }
        let mut content: String = c.borrow().literal.clone();
        if ((k != "text" && k != "code") && k != "math") && k != "html_inline" {
            content = c.borrow().plainText();
        }
        self.pushItem(MdQuoteItem::make(1, level, content.clone()).clone());
    }
    /// Is there anything in this text a rule other than the quotes could
    /// take? Asked of every run of text in the document, so it looks at a
    /// character and the one after it and builds nothing: most prose has
    /// none of these and is left exactly as the parser made it.
    pub fn wants(&self, text: &str) -> bool {
        MdTypography::wantsText(text, &self.opts.clone())
    }
    /// ----------------------------------------------------------------------
    /// What no rule may touch
    /// ----------------------------------------------------------------------
    ///
    /// A bare address (`https://…`, `www.…`) up to the next space, and a
    /// `{…}` that reads as an attribute block. markdown-it's linkify would
    /// have made the first a link; the second is an instruction, not prose.
    pub fn protect(&self, g: &Rc<RefCell<MdTypoGroup>>) {
        let t: String = g.borrow().text.clone();
        if false == MdTypography::mayFence(&t) {
            return;
        }
        let n: i64 = MdText::str_len(&t) as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut stop: i64 = MdTypography::addressEnd(&t, i);
            if stop < 0 {
                stop = MdTypography::attrEnd(&t, i);
            }
            if stop > i {
                let mut k: i64 = i;
                while k < stop {
                    let mut cell: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(k);
                    cell.borrow_mut().kind = 3;
                    k += 1;
                }
                i = stop;
            } else {
                i += 1;
            }
        }
    }
    /// The text of cells [i, i+len) when they are all characters as typed,
    /// else "".
    pub fn plainText(&self, g: &Rc<RefCell<MdTypoGroup>>, i: i64, __len: i64) -> String {
        let n: i64 = g.borrow().cellCount();
        if i + __len > n {
            return String::new();
        }
        let mut out: String = String::new();
        let mut k: i64 = i;
        while k < i + __len {
            let mut c: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(k);
            if c.borrow().kind != 0 || c.borrow().abbr >= 0 {
                return String::new();
            }
            out = format!("{}{}", out, c.borrow().text);
            k += 1;
        }
        out
    }
    pub fn charOf(&self, g: &Rc<RefCell<MdTypoGroup>>, i: i64) -> i64 {
        if i < 0 {
            return -1;
        }
        if i >= g.borrow().cellCount() {
            return -1;
        }
        let mut c: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i);
        return c.borrow().first();
    }
    pub fn emojiPass(&self, g: &Rc<RefCell<MdTypoGroup>>) {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let n: i64 = g.borrow().cellCount();
        let sc_count: i64 = MdEmoji::shortcut_count(&self.opts.borrow().emoji());
        let mut i: i64 = 0;
        while i < n {
            let mut cell: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i);
            let mut took: i64 = 0;
            if cell.borrow().kind == 0 && MdTypography::emojiStart(cell.borrow().first()) {
                if cell.borrow().text == ":" {
                    let mut j: i64 = i + 1;
                    while j < n && MdTypography::isNameChar(self.charOf(g, j)) {
                        j += 1;
                    }
                    if j > i + 1 && self.charOf(g, j) == 58 {
                        let name: String = self.plainText(g, i + 1, (j - i) - 1);
                        let v: String = MdEntities::decodeCodes(&MdEmoji::lookup_codes(&self.opts.borrow().emoji(), &name));
                        if !v.is_empty() {
                            let mut last: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(j);
                            let mut _tmp_107 = cell.borrow().a.clone();
                            out.push(MdTypoCell::make(v.clone(), _tmp_107, last.borrow().b.clone(), 1, -1));
                            took = (j - i) + 1;
                        }
                    }
                }
                let mut k: i64 = 0;
                while took == 0 && k < sc_count {
                    let sc: String = MdEmoji::shortcut_at(&self.opts.borrow().emoji(), k);
                    let __len: i64 = MdText::str_len(&sc) as i64;
                    if self.plainText(g, i, __len) == sc {
                        // A match glued to a letter is not an emoticon, and the
                        // scan goes on after it as markdown-it's does.
                        let before: i64 = self.charOf(g, i - 1);
                        let after: i64 = self.charOf(g, i + __len);
                        let ok_before: bool = i == 0 || MdTypography::isZPCc(before);
                        let ok_after: bool = i + __len >= n || MdTypography::isZPCc(after);
                        if ok_before && ok_after {
                            let v2: String = MdEntities::decodeCodes(&MdEmoji::lookup_codes(&self.opts.borrow().emoji(), &MdEmoji::shortcut_name(&self.opts.borrow().emoji(), k)));
                            let mut lastc: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt((i + __len) - 1);
                            let mut _tmp_115 = cell.borrow().a.clone();
                            out.push(MdTypoCell::make(v2.clone(), _tmp_115, lastc.borrow().b.clone(), 1, -1));
                        } else {
                            let mut q: i64 = i;
                            while q < i + __len {
                                out.push(g.borrow().cellAt(q));
                                q += 1;
                            }
                        }
                        took = __len;
                    }
                    k += 1;
                }
            }
            if took == 0 {
                out.push(cell.clone());
                i += 1;
            } else {
                i += took;
            }
        }
        if (out.len() as i64) != n {
            g.borrow_mut().changed = true;
        }
        g.borrow_mut().cells = out.clone();
    }
    pub fn abbrPass(&self, g: &Rc<RefCell<MdTypoGroup>>) {
        if false == self.opts.borrow().hasAbbreviations() {
            return;
        }
        let n: i64 = g.borrow().cellCount();
        let m: i64 = self.opts.borrow().abbrLabels.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut took: i64 = 0;
            let mut prev_ok: bool = true;
            if i > 0 {
                let mut pc: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i - 1);
                if pc.borrow().kind == 0 {
                    prev_ok = MdTypography::isAbbrEdge(pc.borrow().last());
                }
            }
            if prev_ok {
                let mut k: i64 = 0;
                while took == 0 && k < m {
                    let label: String = self.opts.borrow().abbrLabels[k as usize].clone();
                    let __len: i64 = MdText::str_len(&label) as i64;
                    if self.plainText(g, i, __len) == label {
                        let mut next_ok: bool = true;
                        if i + __len < n {
                            let mut nc: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i + __len);
                            if nc.borrow().kind == 0 {
                                next_ok = MdTypography::isAbbrEdge(nc.borrow().first());
                            }
                        }
                        if next_ok {
                            g.borrow_mut().titles.push(self.opts.borrow().abbrTitle(&label));
                            let id: i64 = (g.borrow().titles.len() as i64) - 1;
                            let mut q: i64 = i;
                            while q < i + __len {
                                let mut cc: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(q);
                                cc.borrow_mut().abbr = id;
                                q += 1;
                            }
                            g.borrow_mut().changed = true;
                            took = __len;
                        }
                    }
                    k += 1;
                }
            }
            if took == 0 {
                i += 1;
            } else {
                i += took;
            }
        }
    }
    /// ----------------------------------------------------------------------
    /// Replacements (markdown-it's `replacements` rule, in its order)
    /// ----------------------------------------------------------------------
    ///
    /// Over each stretch of the group markdown-it would have as one token: the
    /// cells between emoji, between the edges of an abbreviation, and outside
    /// the fenced-off addresses.
    pub fn replacePass(&self, g: &Rc<RefCell<MdTypoGroup>>) {
        let mut out: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let mut seg: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        let mut seg_abbr: i64 = -2;
        let n: i64 = g.borrow().cellCount();
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i);
            let in_stretch: bool = c.borrow().kind == 0 || c.borrow().kind == 2;
            if in_stretch && (c.borrow().abbr == seg_abbr || seg.is_empty()) {
                seg_abbr = c.borrow().abbr;
                seg.push(c.clone());
            } else {
                out = self.flushStretch(g, &mut out, &seg);
                seg = self.emptyCells();
                seg_abbr = -2;
                if in_stretch {
                    seg_abbr = c.borrow().abbr;
                    seg.push(c.clone());
                } else {
                    out.push(c.clone());
                }
            }
            i += 1;
        }
        out = self.flushStretch(g, &mut out, &seg);
        g.borrow_mut().cells = out.clone();
    }
    pub fn emptyCells(&self) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let mut e: Vec<Rc<RefCell<MdTypoCell>>> = Vec::new();
        e
    }
    pub fn flushStretch(&self, g: &Rc<RefCell<MdTypoGroup>>, mut out: &mut Vec<Rc<RefCell<MdTypoCell>>>, seg: &Vec<Rc<RefCell<MdTypoCell>>>) -> Vec<Rc<RefCell<MdTypoCell>>> {
        let n0: i64 = seg.len() as i64;
        if n0 == 0 {
            return out.clone();
        }
        let mut s: Vec<Rc<RefCell<MdTypoCell>>> = seg.clone();
        let t: String = MdTypography::textOf(&s);
        if MdTypography::contains(&t, "(") {
            s = MdTypography::scoped(&s);
        }
        if MdTypography::contains(&t, "+-") {
            s = MdTypography::pair(&s, "+", "-", &char::from_u32(177 as u32).unwrap_or('\0').to_string());
        }
        if MdTypography::contains(&t, "..") {
            s = MdTypography::dots(&s);
        }
        if MdTypography::contains(&t, "..") || MdTypography::contains(&t, &char::from_u32(8230 as u32).unwrap_or('\0').to_string()) {
            s = MdTypography::dotsAfterMark(&s);
        }
        if MdTypography::marksIn(&t) {
            s = MdTypography::marks(&s);
        }
        if MdTypography::contains(&t, ",,") {
            s = MdTypography::commas(&s);
        }
        if MdTypography::contains(&t, "--") {
            s = MdTypography::emDash(&s);
            s = MdTypography::enDashSpaced(&s);
            s = MdTypography::enDashJoined(&s);
        }
        let n1: i64 = s.len() as i64;
        let mut i: i64 = 0;
        while i < n1 {
            let mut c: Rc<RefCell<MdTypoCell>> = s[i as usize].clone();
            if c.borrow().kind == 2 {
                g.borrow_mut().changed = true;
            }
            out.push(c.clone());
            i += 1;
        }
        out.clone()
    }
    /// ----------------------------------------------------------------------
    /// Quotes (markdown-it's `smartquotes` rule)
    /// ----------------------------------------------------------------------
    ///
    /// Over the whole block at once, because a pair of quotes may hold an
    /// emphasis or a link: `"*a*"`. A quote pairs with one at the same depth.
    /// Whether a quote opens or closes is decided by its neighbours — which
    /// may be in the next node, across a tag, never across a line break.
    pub fn quoteItems(&self) -> Vec<Rc<RefCell<MdQuoteItem>>> {
        let mut items: Vec<Rc<RefCell<MdQuoteItem>>> = Vec::new();
        let n: i64 = self.walkGroup.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let gi: i64 = self.walkGroup[i as usize];
            if gi < 0 {
                items.push(self.walkItem[i as usize].clone());
            } else {
                let mut g: Rc<RefCell<MdTypoGroup>> = self.groups[gi as usize].clone();
                if false == g.borrow().hasCells {
                    let mut _tmp_119 = g.borrow().level.clone();
                    let mut di: Rc<RefCell<MdQuoteItem>> = MdQuoteItem::make(0, _tmp_119, g.borrow().text.clone());
                    di.borrow_mut().direct = true;
                    di.borrow_mut().group = g.clone();
                    items.push(di.clone());
                } else {
                    self.expandGroup(&g, &mut items);
                }
            }
            i += 1;
        }
        items
    }
    /// A group's cells as markdown-it's tokens would be: text, an emoji, the
    /// inside of an abbreviation one level down.
    pub fn expandGroup(&self, g: &Rc<RefCell<MdTypoGroup>>, mut items: &mut Vec<Rc<RefCell<MdQuoteItem>>>) {
        let n: i64 = g.borrow().cellCount();
        let mut cur: Rc<RefCell<MdQuoteItem>> = MdQuoteItem::make(0, g.borrow().level.clone(), String::new());
        let mut cur_abbr: i64 = -1;
        let mut open: bool = false;
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i);
            if c.borrow().abbr != cur_abbr {
                if open {
                    items.push(cur.clone());
                    open = false;
                }
                if cur_abbr >= 0 {
                    items.push(MdQuoteItem::make(3, g.borrow().level.clone(), String::new()));
                }
                if c.borrow().abbr >= 0 {
                    items.push(MdQuoteItem::make(3, g.borrow().level.clone(), String::new()));
                }
                cur_abbr = c.borrow().abbr;
            }
            let mut lv: i64 = g.borrow().level;
            if cur_abbr >= 0 {
                lv = g.borrow().level + 1;
            }
            if c.borrow().kind == 0 {
                if false == open {
                    cur = MdQuoteItem::make(0, lv, String::new());
                    open = true;
                }
                let text_v = format!("{}{}", cur.borrow().text, c.borrow().text);
                cur.borrow_mut().text = text_v;
                cur.borrow_mut().cells.push(c.clone());
            } else {
                if open {
                    items.push(cur.clone());
                    open = false;
                }
                items.push(MdQuoteItem::make(1, lv, c.borrow().text.clone()));
            }
            i += 1;
        }
        if open {
            items.push(cur.clone());
        }
        if cur_abbr >= 0 {
            items.push(MdQuoteItem::make(3, g.borrow().level.clone(), String::new()));
        }
    }
    pub fn quotePass(&self) {
        let mut items: Vec<Rc<RefCell<MdQuoteItem>>> = self.quoteItems();
        let mut stack: Vec<Rc<RefCell<MdQuoteOpen>>> = Vec::new();
        let n: i64 = items.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut it: Rc<RefCell<MdQuoteItem>> = items[i as usize].clone();
            let this_level: i64 = it.borrow().level;
            // the stack keeps only what is at this depth or shallower
            let sn: i64 = stack.len() as i64;
            if sn > 0 {
                let mut top: Rc<RefCell<MdQuoteOpen>> = stack[(sn - 1) as usize].clone();
                if top.borrow().level > this_level {
                    let mut keep: Vec<Rc<RefCell<MdQuoteOpen>>> = Vec::new();
                    let mut si: i64 = 0;
                    let mut cut: bool = false;
                    while si < sn {
                        let mut so: Rc<RefCell<MdQuoteOpen>> = stack[si as usize].clone();
                        if so.borrow().level > this_level {
                            cut = true;
                        }
                        if false == cut {
                            keep.push(so.clone());
                        }
                        si += 1;
                    }
                    stack = keep.clone();
                }
            }
            if it.borrow().kind == 0 && (it.borrow().direct || !it.borrow().cells.is_empty()) {
                stack = self.quotesIn(&items, i, &stack);
            }
            i += 1;
        }
    }
    pub fn quotesIn(&self, items: &Vec<Rc<RefCell<MdQuoteItem>>>, idx: i64, stack_in: &Vec<Rc<RefCell<MdQuoteOpen>>>) -> Vec<Rc<RefCell<MdQuoteOpen>>> {
        let mut stack: Vec<Rc<RefCell<MdQuoteOpen>>> = stack_in.clone();
        let mut it: Rc<RefCell<MdQuoteItem>> = items[idx as usize].clone();
        let text: String = it.borrow().text.clone();
        let max: i64 = MdText::str_len(&text) as i64;
        let this_level: i64 = it.borrow().level;
        let mut pos: i64 = 0;
        while pos < max {
            let q: i64 = MdText::char_at(&text, pos);
            if q == 34 || q == 39 {
                let at: i64 = pos;
                let is_single: bool = q == 39;
                let mut last_char: i64 = 32;
                if at > 0 {
                    last_char = MdText::char_at(&text, at - 1);
                } else {
                    last_char = self.neighbour(items, idx, -1);
                }
                let mut next_char: i64 = 32;
                if at + 1 < max {
                    next_char = MdText::char_at(&text, at + 1);
                } else {
                    next_char = self.neighbour(items, idx, 1);
                }
                let last_punct: bool = MdTypography::isPunct(last_char);
                let next_punct: bool = MdTypography::isPunct(next_char);
                let last_ws: bool = MdTypography::isSpace(last_char);
                let next_ws: bool = MdTypography::isSpace(next_char);
                let mut can_open: bool = true;
                let mut can_close: bool = true;
                if next_ws {
                    can_open = false;
                } else {
                    if next_punct {
                        if false == (last_ws || last_punct) {
                            can_open = false;
                        }
                    }
                }
                if last_ws {
                    can_close = false;
                } else {
                    if last_punct {
                        if false == (next_ws || next_punct) {
                            can_close = false;
                        }
                    }
                }
                if next_char == 34 && q == 34 {
                    // `1""`: the first one is an inch
                    if (last_char >= 48 && last_char <= 57) {
                        can_close = false;
                        can_open = false;
                    }
                }
                if can_open && can_close {
                    // in a run of punctuation it opens or closes; in the
                    // middle of words, neither
                    can_open = last_punct;
                    can_close = next_punct;
                }
                let mut done: bool = false;
                if false == can_open && false == can_close {
                    if is_single {
                        self.setQuote(&it, at, self.opts.borrow().sq2.clone());
                    }
                    done = true;
                }
                if false == done && can_close {
                    let mut j: i64 = (stack.len() as i64) - 1;
                    while j >= 0 && false == done {
                        let mut so: Rc<RefCell<MdQuoteOpen>> = stack[j as usize].clone();
                        if so.borrow().level < this_level {
                            j = -1;
                        } else {
                            if so.borrow().single == is_single && so.borrow().level == this_level {
                                let mut opener: Rc<RefCell<MdQuoteItem>> = items[(so.borrow().item) as usize].clone();
                                if is_single {
                                    self.setQuote(&it, at, self.opts.borrow().sq2.clone());
                                    let mut _tmp_130 = so.borrow().pos.clone();
                                    self.setQuote(&opener, _tmp_130, self.opts.borrow().sq1.clone());
                                } else {
                                    self.setQuote(&it, at, self.opts.borrow().dq2.clone());
                                    let mut _tmp_133 = so.borrow().pos.clone();
                                    self.setQuote(&opener, _tmp_133, self.opts.borrow().dq1.clone());
                                }
                                let mut kept: Vec<Rc<RefCell<MdQuoteOpen>>> = Vec::new();
                                let mut k: i64 = 0;
                                while k < j {
                                    kept.push(stack[k as usize].clone());
                                    k += 1;
                                }
                                stack = kept.clone();
                                done = true;
                            }
                            j -= 1;
                        }
                    }
                }
                if false == done {
                    if can_open {
                        let mut o: Rc<RefCell<MdQuoteOpen>> = Rc::new(RefCell::new(MdQuoteOpen::new()));
                        o.borrow_mut().item = idx;
                        o.borrow_mut().pos = at;
                        o.borrow_mut().single = is_single;
                        o.borrow_mut().level = this_level;
                        stack.push(o.clone());
                    } else {
                        if can_close && is_single {
                            self.setQuote(&it, at, self.opts.borrow().sq2.clone());
                        }
                    }
                }
            }
            pos += 1;
        }
        stack
    }
    /// The character beside item `idx` in direction `dir`: a space at a line
    /// break or at the end, and tags have no characters.
    pub fn neighbour(&self, items: &Vec<Rc<RefCell<MdQuoteItem>>>, idx: i64, dir: i64) -> i64 {
        let n: i64 = items.len() as i64;
        let mut j: i64 = idx + dir;
        while j >= 0 && j < n {
            let mut it: Rc<RefCell<MdQuoteItem>> = items[j as usize].clone();
            if it.borrow().kind == 2 {
                return 32;
            }
            let __len: i64 = MdText::str_len(&it.borrow().text) as i64;
            if __len > 0 {
                if dir < 0 {
                    return MdText::char_at(&it.borrow().text, __len - 1);
                }
                return MdText::char_at(&it.borrow().text, 0);
            }
            j += dir;
        }
        32
    }
    pub fn setQuote(&self, it: &Rc<RefCell<MdQuoteItem>>, at: i64, code: i64) {
        if it.borrow().direct {
            self.setDirect(&it.borrow().group.clone(), at, code);
            return;
        }
        let mut c: Rc<RefCell<MdTypoCell>> = it.borrow().cells[at as usize].clone();
        c.borrow_mut().text = MdChar::fromCode(code);
        // the source is one character and so is this: the node it goes back
        // into still maps character for character
        self.markChanged(&c);
    }
    /// A quote into the node that holds offset `at` of the group's text, in
    /// place: one character for one, so every span stays as it was. Not in a
    /// bare address or an attribute block.
    ///
    /// `at` counts in the text as parsed. A target whose strings count bytes
    /// (Go, C++) holds a curly quote in three, so a quote put in before this
    /// one in the same node has moved what follows it: the place in the node
    /// is found by walking the text as parsed beside the node as it is now.
    pub fn setDirect(&self, g: &Rc<RefCell<MdTypoGroup>>, at: i64, code: i64) {
        if MdTypography::isFenced(&g.borrow().text.clone(), at) {
            return;
        }
        let mut k: i64 = g.borrow().first;
        let mut off: i64 = 0;
        while k < g.borrow().first + g.borrow().count {
            let mut nd: Rc<RefCell<MdNode>> = (g.borrow().parent).borrow().childAt(k);
            // the node's length as parsed: a verbatim node's span
            let __len: i64 = nd.borrow().srcEnd - nd.borrow().srcStart;
            if at >= off && at < off + __len {
                let was: String = MdText::substr(&g.borrow().text, off, off + __len);
                let p: i64 = self.livePos(&was, &nd.borrow().literal.clone(), at - off);
                let literal_v = format!("{}{}{}", MdText::substr(&nd.borrow().literal, 0, p), MdChar::fromCode(code), MdText::substr(&nd.borrow().literal, p + 1, (MdText::str_len(&nd.borrow().literal) as i64)));
                nd.borrow_mut().literal = literal_v;
                return;
            }
            off += __len;
            k += 1;
        }
    }
    /// Where offset `p` of `was` (a node's text as parsed) is in `now` (the
    /// same text with some quotes already curly).
    pub fn livePos(&self, was: &str, now: &str, p: i64) -> i64 {
        let mut i: i64 = 0;
        let mut j: i64 = 0;
        while i < p && j < (MdText::str_len(&now) as i64) {
            if MdText::substr(&now, j, j + 1) == MdText::substr(&was, i, i + 1) {
                i += 1;
                j += 1;
            } else {
                j += self.quoteWidth(now, j);
                i += 1;
            }
        }
        j
    }
    /// how long the curly quote at `j` of `s` is in this target's strings
    pub fn quoteWidth(&self, s: &str, j: i64) -> i64 {
        let mut codes: Vec<i64> = vec![self.opts.borrow().dq1, self.opts.borrow().dq2, self.opts.borrow().sq1, self.opts.borrow().sq2];
        for code in codes.iter().copied() {
            let q: String = MdChar::fromCode(code);
            let w: i64 = MdText::str_len(&q) as i64;
            if j + w <= (MdText::str_len(&s) as i64) && MdText::substr(&s, j, j + w) == q {
                return w;
            }
        }
        1
    }
    pub fn markChanged(&self, c: &Rc<RefCell<MdTypoCell>>) {
        let n: i64 = self.groups.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut g: Rc<RefCell<MdTypoGroup>> = self.groups[i as usize].clone();
            if g.borrow().hasCells && (c.borrow().a >= g.borrow().base && c.borrow().a < g.borrow().base + (MdText::str_len(&g.borrow().text) as i64)) {
                g.borrow_mut().changed = true;
                return;
            }
            i += 1;
        }
    }
    /// ----------------------------------------------------------------------
    /// Back into nodes
    /// ----------------------------------------------------------------------
    pub fn rebuild(&self, g: &Rc<RefCell<MdTypoGroup>>) {
        let mut made: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let n: i64 = g.borrow().cellCount();
        let mut i: i64 = 0;
        while i < n {
            let mut c: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i);
            if c.borrow().abbr >= 0 {
                let mut j: i64 = i;
                while j < n && MdTypography::abbrOf(g, j) == c.borrow().abbr {
                    j += 1;
                }
                let mut ab: Rc<RefCell<MdNode>> = MdNode::create("abbr".to_string());
                ab.borrow_mut().title = g.borrow().titles[(c.borrow().abbr) as usize].clone();
                let mut end_cell: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(j - 1);
                ab.borrow_mut().srcStart = c.borrow().a;
                ab.borrow_mut().srcEnd = end_cell.borrow().b;
                ab.borrow_mut().srcKnown = true;
                let mut inner: Vec<Rc<RefCell<MdNode>>> = self.nodesOf(g, i, j);
                let mut k: i64 = 0;
                while k < (inner.len() as i64) {
                    ab.borrow_mut().add(inner[k as usize].clone());
                    k += 1;
                }
                made.push(ab.clone());
                i = j;
            } else {
                let mut j2: i64 = i;
                while j2 < n && MdTypography::abbrOf(g, j2) < 0 {
                    j2 += 1;
                }
                let mut plain: Vec<Rc<RefCell<MdNode>>> = self.nodesOf(g, i, j2);
                let mut k2: i64 = 0;
                while k2 < (plain.len() as i64) {
                    made.push(plain[k2 as usize].clone());
                    k2 += 1;
                }
                i = j2;
            }
        }
        // the group's nodes out, these in
        let mut parent: Rc<RefCell<MdNode>> = g.borrow().parent.clone().clone();
        let mut kids: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let total: i64 = parent.borrow().childCount();
        let mut p: i64 = 0;
        while p < total {
            if p == g.borrow().first {
                let mut m: i64 = 0;
                while m < (made.len() as i64) {
                    kids.push(made[m as usize].clone());
                    m += 1;
                }
            }
            if p < g.borrow().first || p >= g.borrow().first + g.borrow().count {
                kids.push(parent.borrow().childAt(p));
            }
            p += 1;
        }
        parent.borrow_mut().children = kids.clone();
    }
    /// Cells [from, to) as text nodes: characters as typed run together, each
    /// replacement on its own.
    pub fn nodesOf(&self, g: &Rc<RefCell<MdTypoGroup>>, from: i64, to: i64) -> Vec<Rc<RefCell<MdNode>>> {
        let mut out: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let mut i: i64 = from;
        while i < to {
            let mut c: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(i);
            if c.borrow().kind == 1 || c.borrow().kind == 2 {
                let mut t: Rc<RefCell<MdNode>> = MdNode::text(c.borrow().text.clone());
                t.borrow_mut().srcStart = c.borrow().a;
                t.borrow_mut().srcEnd = c.borrow().b;
                t.borrow_mut().srcKnown = true;
                t.borrow_mut().subst = true;
                out.push(t.clone());
                i += 1;
            } else {
                let mut j: i64 = i;
                let mut lit: String = String::new();
                let mut going: bool = true;
                while going && j < to {
                    let mut d: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(j);
                    if d.borrow().kind == 0 || d.borrow().kind == 3 {
                        lit = format!("{}{}", lit, d.borrow().text);
                        j += 1;
                    } else {
                        going = false;
                    }
                }
                let mut last: Rc<RefCell<MdTypoCell>> = g.borrow().cellAt(j - 1);
                let mut t2: Rc<RefCell<MdNode>> = MdNode::text(lit.clone());
                t2.borrow_mut().srcStart = c.borrow().a;
                t2.borrow_mut().srcEnd = last.borrow().b;
                t2.borrow_mut().srcKnown = true;
                out.push(t2.clone());
                i = j;
            }
        }
        out
    }
}
