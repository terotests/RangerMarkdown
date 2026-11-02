// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_entities::MdEntities;
use crate::md_node::MdLinkRef;
use crate::md_node::MdNode;
use crate::md_node::MdNotes;
use crate::md_typography::MdInlineOptions;
use crate::md_typography::MdTypography;
use crate::rgr::MdText;

/// ============================================================================
/// MdInline.rgr — phase 2 of the CommonMark algorithm: what is inside a line.
/// ============================================================================
///
/// The block parser hands each leaf block its raw text; this turns that text
/// into emphasis, code spans, links, images, autolinks and the rest. It is the
/// half of markdown where the difficulty lives, and the reason is one rule:
///
///     `*` opens emphasis or closes it depending on what is on BOTH sides of
///     it, and which `*` pairs with which is decided after the whole line has
///     been read — not while it is being read.
///
/// So the parse is two passes over one line. The first walks it left to right
/// and pushes every run of `*`, `_` or `~` onto a delimiter stack as a plain
/// text node. The second — `processEmphasis` — walks the stack backwards from
/// each closer looking for the nearest opener that may match it, and rewrites
/// the nodes between them into an `emph` or `strong`. `***a***`, `*a **b** c*`
/// and `*a *b* c*` all come out of that one loop; a parser that decides at the
/// first `*` gets all three wrong.
///
/// **Why an index-linked list.** The rewrite moves a run of siblings into a new
/// node and deletes the delimiters' own text nodes, from the middle, while a
/// stack still points at them. The reference implementation uses a doubly
/// linked list for exactly that. Ranger has arrays, so the list is
/// `nodes` + `nx` + `pv`: three arrays where an index, once handed out, stays
/// valid however much of the list is unlinked around it. Building the same
/// thing out of array splices would need every delimiter's index rewritten on
/// every edit, which is the bug this shape does not have.
/// ============================================================================
/// One run of `*`, `_` or `~`, waiting to find out what it is.
#[derive(Clone)]
pub struct MdDelim {
    pub cc: i64,
    pub numdelims: i64,
    pub origdelims: i64,
    /// index into the inline parser's node list
    pub node: i64,
    pub canOpen: bool,
    pub canClose: bool,
    /// the stack is a linked list too, for the same reason the nodes are
    pub prev: i64,
    pub next: i64,
    pub removed: bool,
}

impl MdDelim {
    pub fn new() -> Self {
        Self {
            cc: 0,
            numdelims: 0,
            origdelims: 0,
            node: -1,
            canOpen: false,
            canClose: false,
            prev: -1,
            next: -1,
            removed: false,
        }
    }
}

/// One `[` or `![`, waiting for its `]`.
#[derive(Clone)]
pub struct MdBracket {
    pub node: i64,
    /// subject position just after the bracket
    pub index: i64,
    pub image: bool,
    pub active: bool,
    pub bracketAfter: bool,
    /// the delimiter stack top when this bracket was pushed, so a link's
    /// contents are emphasised without touching anything outside it
    pub prevDelim: i64,
}

impl MdBracket {
    pub fn new() -> Self {
        Self {
            node: -1,
            index: 0,
            image: false,
            active: true,
            bracketAfter: false,
            prevDelim: -1,
        }
    }
}

#[derive(Clone)]
pub struct MdInline {
    pub subject: String,
    pub pos: i64,
    /// --- where a node came from ----------------------------------------------
    ///
    /// `subject` is the block's `literal` with its ends trimmed, and `literal`
    /// is already several strips away from the file — `MdNode`'s header has the
    /// list. So a position in `subject` reaches the document in two steps:
    /// `+ subjBase` gets to the block's literal, and `owner.srcOfLiteral` gets
    /// from there to the character a reader typed.
    ///
    /// Stamping is done in `parseInline`, once, around the dispatch: whatever
    /// nodes a step appended, it appended for the characters that step consumed.
    /// The two constructs that span more than their own step — a link, which
    /// began at a `[` several steps ago, and an emphasis, which is decided
    /// walking the delimiter stack backwards after the line has been read —
    /// overwrite it where they are built.
    pub owner: Rc<RefCell<MdNode>>,
    pub subjBase: i64,
    pub refs: HashMap<String,Rc<RefCell<MdLinkRef>>>,
    /// the document's footnotes: which labels are defined, and the numbers
    /// handed out as references are met (see `MdNotes`)
    pub notes: Rc<RefCell<MdNotes>>,
    pub entities: Rc<RefCell<MdEntities>>,
    /// What the document reads beyond CommonMark — markdown-it's plugins and
    /// its typographer — and the abbreviations it defined. Shared with the
    /// parser a `^…^` or `~…~` starts for its own contents, and with the block
    /// parser that collects the definitions.
    pub opts: Rc<RefCell<MdInlineOptions>>,
    /// the inline node list: nodes, and the links between them
    pub nodes: Vec<Rc<RefCell<MdNode>>>,
    pub nx: Vec<i64>,
    pub pv: Vec<i64>,
    pub head: i64,
    pub tail: i64,
    /// the delimiter stack
    pub delims: Vec<Rc<RefCell<MdDelim>>>,
    pub delimTop: i64,
    /// the bracket stack, innermost last
    pub brackets: Vec<Rc<RefCell<MdBracket>>>,
}

impl MdInline {
    pub fn new() -> Self {
        Self {
            subject: String::new(),
            pos: 0,
            owner: Rc::new(RefCell::new(MdNode::new())),
            subjBase: 0,
            refs: HashMap::new(),
            notes: Rc::new(RefCell::new(MdNotes::new())),
            entities: Rc::new(RefCell::new(MdEntities::new())),
            opts: Rc::new(RefCell::new(MdInlineOptions::new())),
            nodes: Vec::new(),
            nx: Vec::new(),
            pv: Vec::new(),
            head: -1,
            tail: -1,
            delims: Vec::new(),
            delimTop: -1,
            brackets: Vec::new(),
        }
    }
    /// The same count `MdBlock.leadTrim` makes, spelled here so the inline
    /// parser does not import the block parser to ask.
    pub fn leadTrim(s: &str) -> i64 {
        let n: i64 = MdText::str_len(&s) as i64;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            if ((c != 32 && c != 9) && c != 10) && c != 13 {
                return i;
            }
            i += 1;
        }
        n
    }
    /// Everything that is not the first character of some construct. Kept as a
    /// test rather than as the specification's character class regex so the run
    /// scanner and this cannot disagree about where a run ends.
    pub fn isOrdinary(c: i64) -> bool {
        if c == 10 {
            return false;
        }
        if c == 92 {
            return false;
        }
        if c == 96 {
            return false;
        }
        if c == 42 {
            return false;
        }
        if c == 95 {
            return false;
        }
        if c == 126 {
            return false;
        }
        if c == 91 {
            return false;
        }
        if c == 93 {
            return false;
        }
        if c == 33 {
            return false;
        }
        if c == 60 {
            return false;
        }
        if c == 38 {
            return false;
        }
        if c == 36 {
            return false;
        }
        true
    }
    pub fn trailingSpaces(v: &str) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut i: i64 = n;
        while i > 0 {
            let c: i64 = MdText::char_at(&v, i - 1);
            if c != 32 {
                break;
            }
            i -= 1;
        }
        n - i
    }
    pub fn collapseNewlines(v: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&v) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if c == 10 {
                out = format!("{} ", out);
            } else {
                out = format!("{}{}", out, MdText::substr(&v, i, i + 1));
            }
            i += 1;
        }
        out
    }
    /// "If the resulting string both begins and ends with a space character,
    /// but does not consist entirely of space characters, a single space
    /// character is removed from the front and back."
    pub fn stripOneSpace(v: &str) -> String {
        let n: i64 = MdText::str_len(&v) as i64;
        if n < 2 {
            return v.to_string();
        }
        let first: i64 = MdText::char_at(&v, 0);
        let last: i64 = MdText::char_at(&v, n - 1);
        if first != 32 {
            return v.to_string();
        }
        if last != 32 {
            return v.to_string();
        }
        let mut all_space: bool = true;
        let mut i: i64 = 0;
        while i < n {
            if MdText::char_at(&v, i) != 32 {
                all_space = false;
                break;
            }
            i += 1;
        }
        if all_space {
            return v.to_string();
        }
        MdText::substr(&v, 1, n - 1)
    }
    /// The specification's own trick: a separate lower bound per delimiter
    /// character AND per run length modulo three, because the "rule of three"
    /// makes runs of different lengths fail to match for different reasons.
    pub fn openersSlot(cc: i64, origdelims: i64) -> i64 {
        let m: i64 = origdelims % 3;
        if cc == 95 {
            return 2 + m;
        }
        if cc == 42 {
            return 5 + m;
        }
        if cc == 126 {
            return 8 + m;
        }
        if cc == 43 {
            return 11 + m;
        }
        if cc == 61 {
            return 14 + m;
        }
        0
    }
    /// Where a run of exactly `run` backticks starts at or after `from`, or -1.
    pub fn closingTicks(v: &str, from: i64, run: i64) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut i: i64 = from;
        while i < n {
            if MdText::char_at(&v, i) == 96 {
                let mut j: i64 = i;
                while j < n && MdText::char_at(&v, j) == 96 {
                    j += 1;
                }
                if j - i == run {
                    return i;
                }
                i = j;
            } else {
                i += 1;
            }
        }
        -1
    }
    /// Case-folded and whitespace-collapsed. The specification folds with
    /// Unicode case folding; this folds ASCII, which is every label anybody
    /// writes — stated here rather than left to be discovered.
    pub fn normalizeReference(raw: &str) -> String {
        let n: i64 = MdText::str_len(&raw) as i64;
        let mut inner: String = raw.to_string();
        if n >= 2 {
            let first: i64 = MdText::char_at(&raw, 0);
            if first == 91 {
                inner = MdText::substr(&raw, 1, n - 1);
            }
        }
        let t: String = inner.trim().to_string();
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let m: i64 = MdText::str_len(&t) as i64;
        let mut last_was_space: bool = false;
        while i < m {
            let c: i64 = MdText::char_at(&t, i);
            if MdChar::isUnicodeWhitespace(c) {
                if false == last_was_space {
                    out = format!("{} ", out);
                }
                last_was_space = true;
            } else {
                last_was_space = false;
                let folded: i64 = MdChar::foldCase(c);
                if folded != c {
                    out = format!("{}{}", out, MdChar::fromCode(folded));
                } else {
                    out = format!("{}{}", out, MdText::substr(&t, i, i + 1));
                }
            }
            i += 1;
        }
        out
    }
    /// Backslash escapes and entity references, for the two places the parser
    /// has to resolve them itself: a link destination and a title.
    pub fn unescape(v: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&v) as i64;
        let mut ents: Rc<RefCell<MdEntities>> = Rc::new(RefCell::new(MdEntities::new()));
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if c == 92 {
                let mut nc: i64 = -1;
                if i + 1 < n {
                    nc = MdText::char_at(&v, i + 1);
                }
                if MdChar::isAsciiPunct(nc) {
                    out = format!("{}{}", out, MdChar::fromCode(nc));
                    i += 2;
                } else {
                    out = format!("{}\\", out);
                    i += 1;
                }
            } else {
                if c == 38 {
                    let taken: i64 = MdInline::entityAt(v, i, &ents);
                    if taken > 0 {
                        let decoded: String = MdInline::entityValue(v, i, taken, &ents);
                        out = format!("{}{}", out, decoded);
                        i += taken;
                    } else {
                        out = format!("{}&", out);
                        i += 1;
                    }
                } else {
                    out = format!("{}{}", out, MdText::substr(&v, i, i + 1));
                    i += 1;
                }
            }
        }
        out
    }
    /// The length of the entity starting at `i`, `&` and `;` included, or 0.
    pub fn entityAt(v: &str, i: i64, ents: &Rc<RefCell<MdEntities>>) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let amp: i64 = MdText::char_at(&v, i);
        if amp != 38 {
            return 0;
        }
        let mut j: i64 = i + 1;
        if j >= n {
            return 0;
        }
        let c: i64 = MdText::char_at(&v, j);
        if c == 35 {
            j += 1;
            if j >= n {
                return 0;
            }
            let c2: i64 = MdText::char_at(&v, j);
            let mut hex: bool = false;
            if c2 == 120 || c2 == 88 {
                hex = true;
                j += 1;
            }
            let mut digits: i64 = 0;
            let mut scanning: bool = true;
            while scanning {
                if j >= n {
                    scanning = false;
                }
                if scanning {
                    let d: i64 = MdText::char_at(&v, j);
                    let mut ok: bool = false;
                    if hex {
                        ok = MdChar::isHexDigit(d);
                    } else {
                        ok = MdChar::isDigit(d);
                    }
                    if false == ok {
                        scanning = false;
                    } else {
                        digits += 1;
                        j += 1;
                    }
                }
            }
            if digits == 0 {
                return 0;
            }
            let maxd: i64 = if hex { 6 } else { 7 };
            if digits > maxd {
                return 0;
            }
            if j >= n {
                return 0;
            }
            let semi: i64 = MdText::char_at(&v, j);
            if semi != 59 {
                return 0;
            }
            return (j + 1) - i;
        }
        let mut __len: i64 = 0;
        let mut going: bool = true;
        while going {
            if j >= n {
                going = false;
            }
            if going {
                let d2: i64 = MdText::char_at(&v, j);
                if false == MdChar::isAlnum(d2) {
                    going = false;
                } else {
                    __len += 1;
                    j += 1;
                }
            }
        }
        if __len < 2 {
            return 0;
        }
        if __len > 32 {
            return 0;
        }
        if j >= n {
            return 0;
        }
        let semi2: i64 = MdText::char_at(&v, j);
        if semi2 != 59 {
            return 0;
        }
        let name: String = MdText::substr(&v, i + 1, j);
        let found: String = ents.borrow_mut().lookup(&name);
        let flen: i64 = MdText::str_len(&found) as i64;
        if flen == 0 {
            return 0;
        }
        (j + 1) - i
    }
    pub fn entityValue(v: &str, i: i64, taken: i64, ents: &Rc<RefCell<MdEntities>>) -> String {
        let body: String = MdText::substr(&v, i + 1, (i + taken) - 1);
        let c: i64 = MdText::char_at(&body, 0);
        if c == 35 {
            let blen: i64 = MdText::str_len(&body) as i64;
            let rest: String = MdText::substr(&body, 1, blen);
            let mut cp: i64 = 0;
            let c2: i64 = MdText::char_at(&rest, 0);
            if c2 == 120 || c2 == 88 {
                let rlen: i64 = MdText::str_len(&rest) as i64;
                let hex_part: String = MdText::substr(&rest, 1, rlen);
                cp = MdEntities::parseHex(&hex_part);
            } else {
                cp = if ((rest.parse::<i64>().ok()).is_some()) { ((rest.parse::<i64>().ok()).unwrap()) } else { 0 };
            }
            if cp == 0 {
                cp = 65533;
            }
            if cp > 1114111 {
                cp = 65533;
            }
            if (cp >= 55296 && cp <= 57343) {
                cp = 65533;
            }
            return MdChar::fromCode(cp);
        }
        return ents.borrow_mut().lookup(&body);
    }
    pub fn isUriAutolink(v: &str) -> bool {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut colon: i64 = -1;
        let mut i: i64 = 0;
        while i < n {
            let ci: i64 = MdText::char_at(&v, i);
            if ci == 58 {
                colon = i;
                i = n;
            } else {
                i += 1;
            }
        }
        // The scheme is 2 to 32 characters. One is not a scheme — `<m:abc>`
        // is text, and reading it as a link is how a mail merge placeholder
        // turns into a hyperlink.
        if colon < 2 {
            return false;
        }
        if colon > 32 {
            return false;
        }
        let first: i64 = MdText::char_at(&v, 0);
        if false == MdChar::isLetter(first) {
            return false;
        }
        let mut j: i64 = 1;
        while j < colon {
            let c: i64 = MdText::char_at(&v, j);
            let mut ok: bool = MdChar::isAlnum(c);
            if c == 43 {
                ok = true;
            }
            if c == 45 {
                ok = true;
            }
            if c == 46 {
                ok = true;
            }
            if false == ok {
                return false;
            }
            j += 1;
        }
        let mut k: i64 = colon + 1;
        while k < n {
            let c2: i64 = MdText::char_at(&v, k);
            if c2 < 33 {
                return false;
            }
            if c2 == 60 {
                return false;
            }
            if c2 == 62 {
                return false;
            }
            k += 1;
        }
        true
    }
    pub fn isEmailAutolink(v: &str) -> bool {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut at: i64 = -1;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&v, i);
            if c == 64 {
                if at >= 0 {
                    return false;
                }
                at = i;
            }
            i += 1;
        }
        if at < 1 {
            return false;
        }
        if at == n - 1 {
            return false;
        }
        let mut j: i64 = 0;
        while j < at {
            let c1: i64 = MdText::char_at(&v, j);
            let mut ok: bool = MdChar::isAlnum(c1);
            if MdInline::isEmailLocalPunct(c1) {
                ok = true;
            }
            if false == ok {
                return false;
            }
            j += 1;
        }
        let mut k: i64 = at + 1;
        let mut label_len: i64 = 0;
        while k < n {
            let c2: i64 = MdText::char_at(&v, k);
            if c2 == 46 {
                if label_len == 0 {
                    return false;
                }
                label_len = 0;
            } else {
                let mut ok2: bool = MdChar::isAlnum(c2);
                if c2 == 45 {
                    ok2 = true;
                }
                if false == ok2 {
                    return false;
                }
                label_len += 1;
                if label_len > 63 {
                    return false;
                }
            }
            k += 1;
        }
        if label_len == 0 {
            return false;
        }
        true
    }
    pub fn isEmailLocalPunct(c: i64) -> bool {
        if c == 46 {
            return true;
        }
        if c == 33 {
            return true;
        }
        if c == 35 {
            return true;
        }
        if c == 36 {
            return true;
        }
        if c == 37 {
            return true;
        }
        if c == 38 {
            return true;
        }
        if c == 39 {
            return true;
        }
        if c == 42 {
            return true;
        }
        if c == 43 {
            return true;
        }
        if c == 47 {
            return true;
        }
        if c == 61 {
            return true;
        }
        if c == 63 {
            return true;
        }
        if c == 94 {
            return true;
        }
        if c == 95 {
            return true;
        }
        if c == 96 {
            return true;
        }
        if c == 123 {
            return true;
        }
        if c == 124 {
            return true;
        }
        if c == 125 {
            return true;
        }
        if c == 126 {
            return true;
        }
        if c == 45 {
            return true;
        }
        false
    }
    pub fn htmlTagLength(v: &str, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let lead: i64 = MdText::char_at(&v, i);
        if lead != 60 {
            return 0;
        }
        if i + 1 >= n {
            return 0;
        }
        let c: i64 = MdText::char_at(&v, i + 1);
        if c == 33 {
            let mut c2: i64 = -1;
            if i + 2 < n {
                c2 = MdText::char_at(&v, i + 2);
            }
            if c2 == 45 {
                return MdInline::scanComment(v, i);
            }
            if c2 == 91 {
                return MdInline::scanUntil(v, i, "]]>");
            }
            if MdChar::isLetter(c2) {
                return MdInline::scanUntil(v, i, ">");
            }
            return 0;
        }
        if c == 63 {
            return MdInline::scanUntil(v, i, "?>");
        }
        if c == 47 {
            return MdInline::scanCloseTag(v, i);
        }
        if MdChar::isLetter(c) {
            return MdInline::scanOpenTag(v, i);
        }
        0
    }
    pub fn scanUntil(v: &str, i: i64, marker: &str) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let m: i64 = MdText::str_len(&marker) as i64;
        let mut j: i64 = i + 1;
        while j + m <= n {
            let probe: String = MdText::substr(&v, j, j + m);
            if probe == marker {
                return (j + m) - i;
            }
            j += 1;
        }
        0
    }
    pub fn scanComment(v: &str, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        if i + 4 > n {
            return 0;
        }
        let open: String = MdText::substr(&v, i, i + 4);
        if open != "<!--" {
            return 0;
        }
        // `<!-->` and `<!--->` are EMPTY comments and end where they are, not
        // at some `-->` later on the line. CommonMark 0.31 added both; a
        // parser that only knows the general form reads the rest of the line
        // as comment text and swallows whatever comes after it.
        let mut after: i64 = -1;
        if i + 4 < n {
            after = MdText::char_at(&v, i + 4);
        }
        if after == 62 {
            return 5;
        }
        if after == 45 {
            let mut after2: i64 = -1;
            if i + 5 < n {
                after2 = MdText::char_at(&v, i + 5);
            }
            if after2 == 62 {
                return 6;
            }
        }
        let mut j: i64 = i + 4;
        while j + 3 <= n {
            let probe: String = MdText::substr(&v, j, j + 3);
            if probe == "-->" {
                return (j + 3) - i;
            }
            j += 1;
        }
        0
    }
    pub fn scanCloseTag(v: &str, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut j: i64 = i + 2;
        if j >= n {
            return 0;
        }
        let first: i64 = MdText::char_at(&v, j);
        if false == MdChar::isLetter(first) {
            return 0;
        }
        j += 1;
        let mut going: bool = true;
        while going {
            if j >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&v, j);
                let mut ok: bool = MdChar::isAlnum(c);
                if c == 45 {
                    ok = true;
                }
                if false == ok {
                    going = false;
                } else {
                    j += 1;
                }
            }
        }
        let mut ws: bool = true;
        while ws {
            if j >= n {
                ws = false;
            }
            if ws {
                let c2: i64 = MdText::char_at(&v, j);
                if false == MdChar::isUnicodeWhitespace(c2) {
                    ws = false;
                } else {
                    j += 1;
                }
            }
        }
        if j >= n {
            return 0;
        }
        let gt: i64 = MdText::char_at(&v, j);
        if gt != 62 {
            return 0;
        }
        (j + 1) - i
    }
    pub fn scanOpenTag(v: &str, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut j: i64 = i + 1;
        let first: i64 = MdText::char_at(&v, j);
        if false == MdChar::isLetter(first) {
            return 0;
        }
        j += 1;
        let mut name: bool = true;
        while name {
            if j >= n {
                name = false;
            }
            if name {
                let c: i64 = MdText::char_at(&v, j);
                let mut ok: bool = MdChar::isAlnum(c);
                if c == 45 {
                    ok = true;
                }
                if false == ok {
                    name = false;
                } else {
                    j += 1;
                }
            }
        }
        let mut more: bool = true;
        while more {
            let ws_start: i64 = j;
            let mut ws: bool = true;
            while ws {
                if j >= n {
                    ws = false;
                }
                if ws {
                    let c2: i64 = MdText::char_at(&v, j);
                    if false == MdChar::isUnicodeWhitespace(c2) {
                        ws = false;
                    } else {
                        j += 1;
                    }
                }
            }
            if j == ws_start {
                more = false;
            } else {
                let an: i64 = MdInline::scanAttribute(v, j);
                if an == 0 {
                    j = ws_start;
                    more = false;
                } else {
                    j += an;
                }
            }
        }
        let mut trail: bool = true;
        while trail {
            if j >= n {
                trail = false;
            }
            if trail {
                let c3: i64 = MdText::char_at(&v, j);
                if false == MdChar::isUnicodeWhitespace(c3) {
                    trail = false;
                } else {
                    j += 1;
                }
            }
        }
        if j >= n {
            return 0;
        }
        let slash: i64 = MdText::char_at(&v, j);
        if slash == 47 {
            j += 1;
        }
        if j >= n {
            return 0;
        }
        let gt: i64 = MdText::char_at(&v, j);
        if gt != 62 {
            return 0;
        }
        (j + 1) - i
    }
    pub fn scanAttribute(v: &str, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut j: i64 = i;
        let c: i64 = MdText::char_at(&v, j);
        let mut ok: bool = MdChar::isLetter(c);
        if c == 95 {
            ok = true;
        }
        if c == 58 {
            ok = true;
        }
        if false == ok {
            return 0;
        }
        j += 1;
        let mut name: bool = true;
        while name {
            if j >= n {
                name = false;
            }
            if name {
                let c2: i64 = MdText::char_at(&v, j);
                let mut ok2: bool = MdChar::isAlnum(c2);
                if c2 == 95 {
                    ok2 = true;
                }
                if c2 == 58 {
                    ok2 = true;
                }
                if c2 == 46 {
                    ok2 = true;
                }
                if c2 == 45 {
                    ok2 = true;
                }
                if false == ok2 {
                    name = false;
                } else {
                    j += 1;
                }
            }
        }
        let name_end: i64 = j;
        let mut k: i64 = j;
        let mut ws: bool = true;
        while ws {
            if k >= n {
                ws = false;
            }
            if ws {
                let c3: i64 = MdText::char_at(&v, k);
                if false == MdChar::isUnicodeWhitespace(c3) {
                    ws = false;
                } else {
                    k += 1;
                }
            }
        }
        if k >= n {
            return name_end - i;
        }
        let eq: i64 = MdText::char_at(&v, k);
        if eq != 61 {
            return name_end - i;
        }
        k += 1;
        let mut ws2: bool = true;
        while ws2 {
            if k >= n {
                ws2 = false;
            }
            if ws2 {
                let c4: i64 = MdText::char_at(&v, k);
                if false == MdChar::isUnicodeWhitespace(c4) {
                    ws2 = false;
                } else {
                    k += 1;
                }
            }
        }
        if k >= n {
            return 0;
        }
        let q: i64 = MdText::char_at(&v, k);
        if q == 34 || q == 39 {
            k += 1;
            while k < n {
                let c5: i64 = MdText::char_at(&v, k);
                if c5 == q {
                    return (k + 1) - i;
                }
                k += 1;
            }
            return 0;
        }
        let started: i64 = k;
        let mut bare: bool = true;
        while bare {
            if k >= n {
                bare = false;
            }
            if bare {
                let c6: i64 = MdText::char_at(&v, k);
                let mut stop: bool = MdChar::isUnicodeWhitespace(c6);
                if c6 == 34 {
                    stop = true;
                }
                if c6 == 39 {
                    stop = true;
                }
                if c6 == 61 {
                    stop = true;
                }
                if c6 == 60 {
                    stop = true;
                }
                if c6 == 62 {
                    stop = true;
                }
                if c6 == 96 {
                    stop = true;
                }
                if stop {
                    bare = false;
                } else {
                    k += 1;
                }
            }
        }
        if k == started {
            return 0;
        }
        k - i
    }
    /// ----------------------------------------------------------------------
    /// The node list
    /// ----------------------------------------------------------------------
    pub fn appendNode(&mut self, n: Rc<RefCell<MdNode>>) -> i64 {
        self.nodes.push(n.clone());
        self.nx.push(-1);
        self.pv.push(self.tail);
        let count: i64 = self.nodes.len() as i64;
        let idx: i64 = count - 1;
        if self.tail >= 0 {
            self.nx[(self.tail) as usize] = idx;
        } else {
            self.head = idx;
        }
        self.tail = idx;
        idx
    }
    pub fn insertAfter(&mut self, at: i64, n: Rc<RefCell<MdNode>>) -> i64 {
        self.nodes.push(n.clone());
        self.nx.push(-1);
        self.pv.push(-1);
        let count: i64 = self.nodes.len() as i64;
        let idx: i64 = count - 1;
        let after: i64 = self.nx[at as usize];
        self.pv[idx as usize] = at;
        self.nx[idx as usize] = after;
        self.nx[at as usize] = idx;
        if after >= 0 {
            self.pv[after as usize] = idx;
        } else {
            self.tail = idx;
        }
        idx
    }
    pub fn unlink(&mut self, i: i64) {
        let p: i64 = self.pv[i as usize];
        let n: i64 = self.nx[i as usize];
        if p >= 0 {
            self.nx[p as usize] = n;
        } else {
            self.head = n;
        }
        if n >= 0 {
            self.pv[n as usize] = p;
        } else {
            self.tail = p;
        }
        self.nx[i as usize] = -1;
        self.pv[i as usize] = -1;
    }
    pub fn nodeAt(&self, i: i64) -> Rc<RefCell<MdNode>> {
        self.nodes[i as usize].clone()
    }
    /// The last node in the list, or -1. Used by the newline rule, which has to
    /// look at the text that came just before it.
    pub fn lastIndex(&self) -> i64 {
        self.tail
    }
    /// ----------------------------------------------------------------------
    /// Entry point
    /// ----------------------------------------------------------------------
    pub fn parse(&mut self, block: Rc<RefCell<MdNode>>, raw: &str, _map: &HashMap<String,Rc<RefCell<MdLinkRef>>>) {
        self.parseFrom(block.clone(), raw, _map, 0);
    }
    /// `base` is where `raw` starts inside `block.literal`. It is not always
    /// zero: a task item's `[x] ` is taken off the front before the inline pass
    /// so that the checkbox is not three text nodes, and everything after it is
    /// that much further along.
    pub fn parseFrom(&mut self, block: Rc<RefCell<MdNode>>, raw: &str, _map: &HashMap<String,Rc<RefCell<MdLinkRef>>>, base: i64) {
        let arg_0 = base + MdInline::leadTrim(raw);
        self.parseRange(block.clone(),
            &block,
            raw.trim().to_string(),
            arg_0,
            _map.clone()
        );
        // What markdown-it's core rules do after its inline pass, and for the
        // same reason here: an emoticon, an abbreviation and a pair of quotes
        // are found in the TEXT a line came out as, which is only known once
        // code spans, links and emphasis have taken their characters.
        let mut _tmp_170 = self.subject.clone();
        MdTypography::apply(block.clone(), &_tmp_170, self.opts.clone());
    }
    /// The parse itself: `text` is `owner`'s literal from `base` on, and the
    /// nodes go to `target`. The two differ for the contents of `^…^` and
    /// `~…~`, which are parsed by a parser of their own into the node that
    /// holds them while their positions are still the block's.
    pub fn parseRange(&mut self, own: Rc<RefCell<MdNode>>, target: &Rc<RefCell<MdNode>>, text: String, base: i64, _map: HashMap<String,Rc<RefCell<MdLinkRef>>>) {
        self.owner = own.clone();
        self.subject = text.clone();
        self.subjBase = base;
        self.pos = 0;
        self.refs = _map.clone();
        self.nodes.clear();
        self.nx.clear();
        self.pv.clear();
        self.head = -1;
        self.tail = -1;
        self.delims.clear();
        self.delimTop = -1;
        self.brackets.clear();
        let mut more: bool = true;
        while more {
            more = self.parseInline();
        }
        self.processEmphasis(-1);
        self.collectInto(target);
    }
    pub fn collectInto(&self, block: &Rc<RefCell<MdNode>>) {
        let mut i: i64 = self.head;
        while i >= 0 {
            let mut n: Rc<RefCell<MdNode>> = self.nodes[i as usize].clone();
            block.borrow_mut().add(n.clone());
            i = self.nx[i as usize];
        }
    }
    /// A subject position, in the document.
    pub fn srcAt(&self, p: i64) -> i64 {
        return self.owner.borrow().srcOfLiteral(self.subjBase + p);
    }
    /// One node, from subject[from,to). For the constructs that build a child
    /// directly instead of putting it on the flat list.
    pub fn stampNode(&self, nd: &Rc<RefCell<MdNode>>, from: i64, to: i64) {
        nd.borrow_mut().srcStart = self.srcAt(from);
        nd.borrow_mut().srcEnd = self.srcAt(to);
        nd.borrow_mut().srcKnown = true;
    }
    /// Every node appended since `mark` came from subject[from,to).
    pub fn stampFrom(&self, mark: i64, from: i64, to: i64) {
        let n: i64 = self.nodes.len() as i64;
        let a: i64 = self.srcAt(from);
        let b: i64 = self.srcAt(to);
        let mut i: i64 = mark;
        while i < n {
            let mut nd: Rc<RefCell<MdNode>> = self.nodes[i as usize].clone();
            if false == nd.borrow().srcKnown {
                nd.borrow_mut().srcStart = a;
                nd.borrow_mut().srcEnd = b;
                nd.borrow_mut().srcKnown = true;
            }
            i += 1;
        }
    }
    pub fn peek(&self) -> i64 {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        if self.pos < n {
            return MdText::char_at(&self.subject, self.pos);
        }
        -1
    }
    pub fn peekAt(&self, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        if i < 0 {
            return -1;
        }
        if i < n {
            return MdText::char_at(&self.subject, i);
        }
        -1
    }
    pub fn addText(&mut self, v: &str) -> i64 {
        let mut n: Rc<RefCell<MdNode>> = MdNode::text(v.to_string());
        self.appendNode(n.clone())
    }
    pub fn parseInline(&mut self) -> bool {
        let c: i64 = self.peek();
        if c < 0 {
            return false;
        }
        let stamp_mark: i64 = self.nodes.len() as i64;
        let stamp_start: i64 = self.pos;
        let mut handled: bool = false;
        if c == 10 {
            handled = self.parseNewline();
        }
        if c == 92 {
            handled = self.parseBackslash();
        }
        if c == 96 {
            handled = self.parseBackticks();
        }
        if c == 36 {
            handled = self.parseDollar();
        }
        if c == 42 {
            handled = self.handleDelim(c);
        }
        if c == 95 {
            handled = self.handleDelim(c);
        }
        if self.opts.borrow().extensions {
            // `~x~` and `^x^` first: a single tilde that closes on the same
            // word is a subscript (markdown-it-sub) before it is GFM's
            // one-tilde strikethrough, which is what it falls back to.
            if c == 126 || c == 94 {
                handled = self.parseSubSup(c);
            }
            // `++` and `==` are delimiter runs, as `**` is (markdown-it-ins,
            // markdown-it-mark); one `+` or `=` is a character
            if c == 43 || c == 61 {
                if self.peekAt(self.pos + 1) == c {
                    handled = self.handleDelim(c);
                }
            }
        }
        if c == 126 && false == handled {
            handled = self.handleDelim(c);
        }
        if c == 91 {
            handled = self.parseOpenBracket();
        }
        if c == 94 && self.peekAt(self.pos + 1) == 91 {
            handled = self.parseInlineNote();
        }
        if c == 33 {
            handled = self.parseBang();
        }
        if c == 93 {
            handled = self.parseCloseBracket();
        }
        if c == 60 {
            handled = self.parseAutolink();
            if false == handled {
                handled = self.parseHtmlTag();
            }
        }
        if c == 38 {
            handled = self.parseEntity();
        }
        if false == handled && MdInline::isOrdinary(c) {
            handled = self.parseString();
        }
        if false == handled {
            self.pos += 1;
            let one: String = MdChar::fromCode(c);
            self.addText(&one);
        }
        self.stampFrom(stamp_mark, stamp_start, self.pos.clone());
        true
    }
    pub fn parseString(&mut self) -> bool {
        let start: i64 = self.pos;
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let mut going: bool = true;
        while going {
            if self.pos >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&self.subject, self.pos);
                let mut ordinary: bool = MdInline::isOrdinary(c);
                // `^` is ordinary text unless it opens an inline footnote,
                // `^[…]`, or a `^sup^` / `++ins++` / `==mark==` that the
                // extensions read
                if c == 94 && self.peekAt(self.pos + 1) == 91 {
                    ordinary = false;
                }
                if ordinary && self.pos > start {
                    if self.extensionAt(self.pos.clone()) {
                        ordinary = false;
                    }
                }
                if ordinary {
                    self.pos += 1;
                } else {
                    going = false;
                }
            }
        }
        if self.pos == start {
            return false;
        }
        let v: String = MdText::substr(&self.subject, start, self.pos);
        self.addText(&v);
        true
    }
    /// Does a construct of the plugins start at `i`? `+`, `=` and `^` are
    /// ordinary characters to CommonMark, so a run of text would carry them
    /// past the place one opens. Asked only there, and only when the
    /// document reads the plugins: `C++` is still one word of text.
    pub fn extensionAt(&self, i: i64) -> bool {
        if false == self.opts.borrow().extensions {
            return false;
        }
        let c: i64 = MdText::char_at(&self.subject, i);
        if c == 43 || c == 61 {
            return self.peekAt(i + 1) == c;
        }
        if c == 94 {
            return self.subSupClose(i) >= 0;
        }
        false
    }
    /// ----------------------------------------------------------------------
    /// `~sub~` and `^sup^` (markdown-it-sub, markdown-it-sup)
    /// ----------------------------------------------------------------------
    ///
    /// Not delimiter runs: the content may not hold a space or a newline that
    /// is not escaped, so `H~2~O` and `19^th^` are one word each and `x^2 + y`
    /// is text. The closer is the next marker. A tilde that is half of `~~`
    /// is strikethrough's, not this.
    pub fn subSupClose(&self, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let m: i64 = MdText::char_at(&self.subject, i);
        if m == 126 {
            if self.peekAt(i - 1) == 126 {
                return -1;
            }
            if self.peekAt(i + 1) == 126 {
                return -1;
            }
        }
        let mut j: i64 = i + 1;
        while j < n {
            let c: i64 = MdText::char_at(&self.subject, j);
            if c == m {
                if j == i + 1 {
                    return -1;
                }
                return j;
            }
            if (c == 32 || c == 9) || c == 10 {
                return -1;
            }
            if c == 92 {
                j += 2;
            } else {
                j += 1;
            }
        }
        -1
    }
    pub fn parseSubSup(&mut self, c: i64) -> bool {
        let close: i64 = self.subSupClose(self.pos.clone());
        if close < 0 {
            return false;
        }
        let kind: String = if (c == 126) { "sub".to_string() } else { "sup".to_string() };
        let mut node: Rc<RefCell<MdNode>> = MdNode::create(kind.clone());
        let _tmp_179 = self.srcAt(self.pos.clone());
        node.borrow_mut().srcStart = _tmp_179;
        node.borrow_mut().srcEnd = self.srcAt(close + 1);
        node.borrow_mut().srcKnown = true;
        // The contents are inline markdown of their own — `^*a*^` — parsed
        // where they stand: same block, same offsets, its own delimiter stack
        // so a `*` inside cannot pair with one outside.
        let mut inner: Rc<RefCell<MdInline>> = Rc::new(RefCell::new(MdInline::new()));
        inner.borrow_mut().opts = self.opts.clone();
        inner.borrow_mut().entities = self.entities.clone();
        let from: i64 = self.pos + 1;
        let mut _tmp_181 = self.owner.clone();
        inner.borrow_mut().parseRange(_tmp_181, &node, MdText::substr(&self.subject, from, close), self.subjBase + from, self.refs.clone());
        self.appendNode(node.clone());
        self.pos = close + 1;
        true
    }
    /// A newline is a soft break, or a hard one when the line before it ended
    /// in a backslash or in two or more spaces.
    pub fn parseNewline(&mut self) -> bool {
        self.pos += 1;
        let last: i64 = self.tail;
        let mut hard: bool = false;
        if last >= 0 {
            let mut ln: Rc<RefCell<MdNode>> = self.nodes[last as usize].clone();
            if ln.borrow().kind == "text" {
                let lit: String = ln.borrow().literal.clone();
                let cut: i64 = MdInline::trailingSpaces(&lit);
                if cut >= 2 {
                    hard = true;
                }
                if cut > 0 {
                    let keep: i64 = (MdText::str_len(&lit) as i64) - cut;
                    ln.borrow_mut().literal = MdText::substr(&lit, 0, keep);
                    // The trailing spaces are the BREAK now, not the run, and
                    // the run's span has to give them up or a caret placed at
                    // the end of the line lands two characters past the text.
                    if ln.borrow().srcEnd >= cut {
                        let srcEnd_v = ln.borrow().srcEnd - cut;
                        ln.borrow_mut().srcEnd = srcEnd_v;
                    }
                }
            }
        }
        let mut br: Rc<RefCell<MdNode>> = MdNode::create(if hard { "linebreak".to_string() } else { "softbreak".to_string() });
        self.appendNode(br.clone());
        // leading spaces on the next line are not content
        let n: i64 = MdText::str_len(&self.subject) as i64;
        while self.pos < n {
            let c: i64 = MdText::char_at(&self.subject, self.pos);
            if c != 32 {
                break;
            }
            self.pos += 1;
        }
        true
    }
    pub fn parseBackslash(&mut self) -> bool {
        self.pos += 1;
        let c: i64 = self.peek();
        if c == 10 {
            self.pos += 1;
            let mut br: Rc<RefCell<MdNode>> = MdNode::create("linebreak".to_string());
            self.appendNode(br.clone());
            return true;
        }
        if MdChar::isAsciiPunct(c) {
            self.pos += 1;
            self.addText(&MdChar::fromCode(c).clone());
            return true;
        }
        self.addText("\\");
        true
    }
    /// A backtick string of length n closes on the next backtick string of
    /// exactly n. This runs before everything else, which is why the inside of
    /// a code span is never emphasised.
    pub fn parseBackticks(&mut self) -> bool {
        let start: i64 = self.pos;
        let open_len: i64 = self.scanRun(96);
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let after_open: i64 = self.pos;
        let mut scan: i64 = self.pos;
        while scan < n {
            let c: i64 = MdText::char_at(&self.subject, scan);
            if c == 96 {
                let run_start: i64 = scan;
                while scan < n {
                    let c2: i64 = MdText::char_at(&self.subject, scan);
                    if c2 != 96 {
                        break;
                    }
                    scan += 1;
                }
                let run_len: i64 = scan - run_start;
                if run_len == open_len {
                    let body: String = MdText::substr(&self.subject, after_open, run_start);
                    let flat: String = MdInline::collapseNewlines(&body);
                    let mut code: Rc<RefCell<MdNode>> = MdNode::create("code".to_string());
                    code.borrow_mut().literal = MdInline::stripOneSpace(&flat);
                    self.appendNode(code.clone());
                    self.pos = scan;
                    return true;
                }
            } else {
                scan += 1;
            }
        }
        // no closer: the run is literal text
        self.pos = after_open;
        let lit: String = MdText::substr(&self.subject, start, after_open);
        self.addText(&lit);
        true
    }
    /// TeX math: `$…$` in the line, `$$…$$` set as a display. Pandoc's rule
    /// for the single dollar, so prices survive: the opener has a character
    /// right after it, the closer one right before it and no digit after it.
    /// `\$` is a dollar sign — the backslash pass took it before this ran.
    /// What does not close is a literal `$`.
    pub fn parseDollar(&mut self) -> bool {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let start: i64 = self.pos;
        let display: bool = self.peekAt(self.pos + 1) == 36;
        let mut open: i64 = 1;
        if display {
            open = 2;
        }
        let from: i64 = start + open;
        let first: i64 = self.peekAt(from);
        if first < 0 || first == 36 {
            return self.literalDollar(start);
        }
        if !display && ((first == 32 || first == 9) || first == 10) {
            return self.literalDollar(start);
        }
        let mut i: i64 = from;
        while i < n {
            let c: i64 = MdText::char_at(&self.subject, i);
            if c == 92 {
                i += 2;
            } else {
                if c == 36 {
                    if display {
                        if self.peekAt(i + 1) == 36 {
                            self.addMath(&MdText::substr(&self.subject, from, i).clone(), true);
                            self.pos = i + 2;
                            return true;
                        }
                        i += 1;
                    } else {
                        let before: i64 = MdText::char_at(&self.subject, i - 1);
                        let after: i64 = self.peekAt(i + 1);
                        let space_before: bool = (before == 32 || before == 9) || before == 10;
                        let digit_after: bool = (after >= 48 && after <= 57);
                        if !space_before && !digit_after {
                            self.addMath(&MdText::substr(&self.subject, from, i).clone(), false);
                            self.pos = i + 1;
                            return true;
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
        }
        self.literalDollar(start)
    }
    /// A `$` that opens nothing, kept in one text node with the word after it:
    /// `$10` is one word, and a line may not break inside it.
    pub fn literalDollar(&mut self, start: i64) -> bool {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        self.pos = start + 1;
        while self.pos < n && MdText::char_at(&self.subject, self.pos) == 36 {
            self.pos += 1;
        }
        while self.pos < n {
            let c: i64 = MdText::char_at(&self.subject, self.pos);
            if MdInline::isOrdinary(c) && (c != 32 && c != 9) {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.addText(&MdText::substr(&self.subject, start, self.pos).clone());
        true
    }
    pub fn addMath(&mut self, tex: &str, display: bool) {
        let mut m: Rc<RefCell<MdNode>> = MdNode::create("math".to_string());
        m.borrow_mut().literal = tex.trim().to_string();
        if display {
            m.borrow_mut().info = "display".to_string();
        }
        self.appendNode(m.clone());
    }
    /// The run of `c` starting at `pos`, consumed. Returns its length.
    pub fn scanRun(&mut self, c: i64) -> i64 {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let start: i64 = self.pos;
        while self.pos < n {
            let cur: i64 = MdText::char_at(&self.subject, self.pos);
            if cur != c {
                break;
            }
            self.pos += 1;
        }
        self.pos - start
    }
    /// ----------------------------------------------------------------------
    /// Emphasis: the delimiter stack
    /// ----------------------------------------------------------------------
    /// A run of `*`, `_` or `~`, and whether it may open, close, or both.
    /// "Left-flanking" and "right-flanking" are decided by the characters on
    /// either side of the whole run — which is why the run has to be measured
    /// before anything can be decided about it.
    pub fn handleDelim(&mut self, cc: i64) -> bool {
        let start_pos: i64 = self.pos;
        let mut char_before: i64 = 10;
        if start_pos > 0 {
            char_before = MdText::char_at(&self.subject, start_pos - 1);
        }
        let numdelims: i64 = self.scanRun(cc);
        if numdelims == 0 {
            return false;
        }
        let mut char_after: i64 = self.peek();
        if char_after < 0 {
            char_after = 10;
        }
        let after_is_ws: bool = MdChar::isUnicodeWhitespace(char_after);
        let after_is_punct: bool = MdChar::isUnicodePunct(char_after);
        let before_is_ws: bool = MdChar::isUnicodeWhitespace(char_before);
        let before_is_punct: bool = MdChar::isUnicodePunct(char_before);
        let mut left_flanking: bool = false;
        if false == after_is_ws {
            if false == after_is_punct {
                left_flanking = true;
            } else {
                if before_is_ws || before_is_punct {
                    left_flanking = true;
                }
            }
        }
        let mut right_flanking: bool = false;
        if false == before_is_ws {
            if false == before_is_punct {
                right_flanking = true;
            } else {
                if after_is_ws || after_is_punct {
                    right_flanking = true;
                }
            }
        }
        let mut can_open: bool = false;
        let mut can_close: bool = false;
        if cc == 95 {
            // `_` may not open or close inside a word, so that snake_case_names
            // survive being written down.
            if left_flanking && (false == right_flanking || before_is_punct) {
                can_open = true;
            }
            if right_flanking && (false == left_flanking || after_is_punct) {
                can_close = true;
            }
        } else {
            can_open = left_flanking;
            can_close = right_flanking;
        }
        let lit: String = MdText::substr(&self.subject, start_pos, self.pos);
        let idx: i64 = self.addText(&lit);
        let mut d: Rc<RefCell<MdDelim>> = Rc::new(RefCell::new(MdDelim::new()));
        d.borrow_mut().cc = cc;
        d.borrow_mut().numdelims = numdelims;
        d.borrow_mut().origdelims = numdelims;
        d.borrow_mut().node = idx;
        d.borrow_mut().canOpen = can_open;
        d.borrow_mut().canClose = can_close;
        d.borrow_mut().prev = self.delimTop;
        d.borrow_mut().next = -1;
        self.delims.push(d.clone());
        let dcount: i64 = self.delims.len() as i64;
        let di: i64 = dcount - 1;
        if self.delimTop >= 0 {
            let mut top: Rc<RefCell<MdDelim>> = self.delims[(self.delimTop) as usize].clone();
            top.borrow_mut().next = di;
        }
        self.delimTop = di;
        true
    }
    pub fn removeDelimiter(&mut self, i: i64) {
        let mut d: Rc<RefCell<MdDelim>> = self.delims[i as usize].clone();
        if d.borrow().removed {
            return;
        }
        d.borrow_mut().removed = true;
        if d.borrow().prev >= 0 {
            let mut p: Rc<RefCell<MdDelim>> = self.delims[(d.borrow().prev) as usize].clone();
            p.borrow_mut().next = d.borrow().next;
        }
        if d.borrow().next >= 0 {
            let mut n: Rc<RefCell<MdDelim>> = self.delims[(d.borrow().next) as usize].clone();
            n.borrow_mut().prev = d.borrow().prev;
        } else {
            self.delimTop = d.borrow().prev;
        }
    }
    pub fn removeDelimitersBetween(&self, opener: i64, closer: i64) {
        let mut o: Rc<RefCell<MdDelim>> = self.delims[opener as usize].clone();
        let mut c: Rc<RefCell<MdDelim>> = self.delims[closer as usize].clone();
        if o.borrow().next != closer {
            o.borrow_mut().next = closer;
            c.borrow_mut().prev = opener;
        }
    }
    /// The second pass. Walk forward from the bottom of the stack; at every
    /// closer, walk backwards for the nearest opener that may match it, and
    /// rewrite what lies between the two.
    pub fn processEmphasis(&mut self, stack_bottom: i64) {
        let mut openers_bottom: Vec<i64> = Vec::new();
        let mut ob: i64 = 0;
        while ob < 18 {
            openers_bottom.push(stack_bottom);
            ob += 1;
        }
        // the first closer above the bottom
        let mut closer: i64 = self.delimTop;
        while closer >= 0 {
            let mut cd0: Rc<RefCell<MdDelim>> = self.delims[closer as usize].clone();
            if cd0.borrow().prev == stack_bottom {
                break;
            }
            closer = cd0.borrow().prev;
        }
        while closer >= 0 {
            let mut cd: Rc<RefCell<MdDelim>> = self.delims[closer as usize].clone();
            if false == cd.borrow().canClose {
                closer = cd.borrow().next;
            } else {
                let mut _tmp_188 = cd.borrow().cc.clone();
                let slot: i64 = MdInline::openersSlot(_tmp_188, cd.borrow().origdelims.clone());
                let bottom: i64 = openers_bottom[slot as usize];
                let mut opener: i64 = cd.borrow().prev;
                let mut opener_found: bool = false;
                while opener >= 0 {
                    if opener == stack_bottom {
                        break;
                    }
                    if opener == bottom {
                        break;
                    }
                    let mut od0: Rc<RefCell<MdDelim>> = self.delims[opener as usize].clone();
                    let mut odd_match: bool = false;
                    // the rule of three is emphasis's; `++` and `==` pair two
                    // by two whatever the lengths
                    let counts: bool = cd.borrow().cc != 43 && cd.borrow().cc != 61;
                    if counts && (cd.borrow().canOpen || od0.borrow().canClose) {
                        if cd.borrow().origdelims % 3 != 0 {
                            let sum: i64 = od0.borrow().origdelims + cd.borrow().origdelims;
                            if sum % 3 == 0 {
                                odd_match = true;
                            }
                        }
                    }
                    if (od0.borrow().cc == cd.borrow().cc && od0.borrow().canOpen) && false == odd_match {
                        opener_found = true;
                        break;
                    }
                    opener = od0.borrow().prev;
                }
                let old_closer: i64 = closer;
                let mut did_pair: bool = false;
                if opener_found {
                    let mut od: Rc<RefCell<MdDelim>> = self.delims[opener as usize].clone();
                    let mut use_delims: i64 = 1;
                    if cd.borrow().numdelims >= 2 && od.borrow().numdelims >= 2 {
                        use_delims = 2;
                    }
                    let strike: bool = cd.borrow().cc == 126;
                    let pair_of_two: bool = cd.borrow().cc == 43 || cd.borrow().cc == 61;
                    let mut allowed: bool = true;
                    if pair_of_two {
                        // markdown-it-ins / -mark: a pair is two characters
                        // at each end, and a run left with one has none
                        use_delims = 2;
                        if cd.borrow().numdelims < 2 {
                            allowed = false;
                        }
                        if od.borrow().numdelims < 2 {
                            allowed = false;
                        }
                    }
                    if strike {
                        // GFM strikes through with one or two tildes and with
                        // nothing else: three is a fence, not emphasis.
                        if cd.borrow().origdelims > 2 {
                            allowed = false;
                        }
                        if od.borrow().origdelims > 2 {
                            allowed = false;
                        }
                        if allowed {
                            use_delims = if (od.borrow().numdelims < cd.borrow().numdelims) { od.borrow().numdelims } else { cd.borrow().numdelims };
                        }
                    }
                    if allowed {
                        did_pair = true;
                        let mut opener_node: Rc<RefCell<MdNode>> = self.nodes[(od.borrow().node) as usize].clone();
                        let mut closer_node: Rc<RefCell<MdNode>> = self.nodes[(cd.borrow().node) as usize].clone();
                        let numdelims_v = od.borrow().numdelims - use_delims;
                        od.borrow_mut().numdelims = numdelims_v;
                        let numdelims_v = cd.borrow().numdelims - use_delims;
                        cd.borrow_mut().numdelims = numdelims_v;
                        let ol: i64 = MdText::str_len(&opener_node.borrow().literal) as i64;
                        let literal_v = MdText::substr(&opener_node.borrow().literal, 0, ol - use_delims);
                        opener_node.borrow_mut().literal = literal_v;
                        let cl: i64 = MdText::str_len(&closer_node.borrow().literal) as i64;
                        let literal_v = MdText::substr(&closer_node.borrow().literal, 0, cl - use_delims);
                        closer_node.borrow_mut().literal = literal_v;
                        // The delimiters that paired are the LAST `useDelims`
                        // of the opener run and the FIRST of the closer run,
                        // so the construct runs between those two edges and
                        // the two runs keep what is left of themselves. This
                        // is decided here rather than in `parseInline` because
                        // nothing knew, at the time the `*` was read, which
                        // `*` it would pair with — or whether it would.
                        let wrap_start: i64 = opener_node.borrow().srcEnd - use_delims;
                        let wrap_end: i64 = closer_node.borrow().srcStart + use_delims;
                        if opener_node.borrow().srcEnd >= 0 {
                            opener_node.borrow_mut().srcEnd = wrap_start;
                        }
                        if closer_node.borrow().srcStart >= 0 {
                            closer_node.borrow_mut().srcStart = wrap_end;
                        }
                        let mut kind: String = if (use_delims == 1) { "emph".to_string() } else { "strong".to_string() };
                        if strike {
                            kind = "strike".to_string();
                        }
                        if cd.borrow().cc == 43 {
                            kind = "ins".to_string();
                        }
                        if cd.borrow().cc == 61 {
                            kind = "mark".to_string();
                        }
                        let mut wrap: Rc<RefCell<MdNode>> = MdNode::create(kind.clone());
                        wrap.borrow_mut().srcStart = wrap_start;
                        wrap.borrow_mut().srcEnd = wrap_end;
                        wrap.borrow_mut().srcKnown = true;
                        let mut tmp: i64 = self.nx[(od.borrow().node) as usize];
                        let mut scanning: bool = true;
                        while scanning {
                            if tmp < 0 {
                                scanning = false;
                            }
                            if scanning {
                                if tmp == cd.borrow().node {
                                    scanning = false;
                                } else {
                                    let nxt: i64 = self.nx[tmp as usize];
                                    self.unlink(tmp);
                                    let mut moved: Rc<RefCell<MdNode>> = self.nodes[tmp as usize].clone();
                                    wrap.borrow_mut().add(moved.clone());
                                    tmp = nxt;
                                }
                            }
                        }
                        self.insertAfter(od.borrow().node.clone(), wrap.clone());
                        self.removeDelimitersBetween(opener, closer);
                        if od.borrow().numdelims == 0 {
                            self.unlink(od.borrow().node.clone());
                            self.removeDelimiter(opener);
                        }
                        if cd.borrow().numdelims == 0 {
                            let after: i64 = cd.borrow().next;
                            self.unlink(cd.borrow().node.clone());
                            self.removeDelimiter(closer);
                            closer = after;
                        }
                    } else {
                        opener_found = false;
                    }
                }
                if false == did_pair {
                    if false == opener_found {
                        let mut old: Rc<RefCell<MdDelim>> = self.delims[old_closer as usize].clone();
                        openers_bottom[slot as usize] = old.borrow().prev;
                        if false == old.borrow().canOpen {
                            self.removeDelimiter(old_closer);
                        }
                        closer = old.borrow().next;
                    }
                }
            }
        }
        // everything above the bottom is spent
        let mut guard: bool = true;
        while guard {
            if self.delimTop < 0 {
                guard = false;
            }
            if guard {
                if self.delimTop == stack_bottom {
                    guard = false;
                } else {
                    self.removeDelimiter(self.delimTop.clone());
                }
            }
        }
    }
    /// ----------------------------------------------------------------------
    /// Links and images
    /// ----------------------------------------------------------------------
    pub fn parseOpenBracket(&mut self) -> bool {
        if self.peekAt(self.pos + 1) == 94 {
            if self.parseNoteRef() {
                return true;
            }
        }
        let start_pos: i64 = self.pos;
        self.pos += 1;
        let idx: i64 = self.addText("[");
        // The bracket's own position, not the one after it: a shortcut
        // reference takes its label from `[` to `]` INCLUDING the brackets,
        // which is what `normalizeReference` then strips.
        self.pushBracket(idx, start_pos, false);
        true
    }
    /// ----------------------------------------------------------------------
    /// Footnotes (markdown-it-footnote)
    /// ----------------------------------------------------------------------
    /// `[^label]`, when a definition of `label` exists: the label runs to the
    /// first `]` and may hold no space and no line break. Tried before a link,
    /// as the plugin's rule is, so `[^1]` is never a shortcut reference. A
    /// label nobody defined is left to the link rules — text, usually.
    pub fn parseNoteRef(&mut self) -> bool {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let mut i: i64 = self.pos + 2;
        while i < n {
            let c: i64 = MdText::char_at(&self.subject, i);
            if c == 32 || c == 10 {
                return false;
            }
            if c == 93 {
                break;
            }
            i += 1;
        }
        if i >= n {
            return false;
        }
        if i == self.pos + 2 {
            return false;
        }
        let label: String = MdText::substr(&self.subject, self.pos + 2, i);
        if false == self.notes.borrow().hasLabel(&label) {
            return false;
        }
        let mut r#ref: Rc<RefCell<MdNode>> = MdNode::create("footnote_ref".to_string());
        r#ref.borrow_mut().info = label.clone();
        let _tmp_200 = self.notes.borrow().defOf(&label);
        self.notes.clone().borrow_mut().refer(&r#ref, _tmp_200);
        self.appendNode(r#ref.clone());
        self.pos = i + 1;
        true
    }
    /// `^[text of the note]`: a footnote written where it is referenced. Its
    /// text runs to the matching `]` — brackets nest, and a backslash escape
    /// or a code span does not count — and is parsed as inline markdown of its
    /// own. Its body is a `footnote_def` with no label, holding one paragraph,
    /// and it is numbered with the others, in the order they are met.
    pub fn parseInlineNote(&mut self) -> bool {
        let start: i64 = self.pos;
        let label_start: i64 = self.pos + 2;
        let label_end: i64 = self.matchingBracket(self.pos + 1);
        if label_end < 0 {
            return false;
        }
        let mut fd: Rc<RefCell<MdNode>> = MdNode::create("footnote_def".to_string());
        fd.borrow_mut().srcStart = self.srcAt(start);
        fd.borrow_mut().srcEnd = self.srcAt(label_end + 1);
        fd.borrow_mut().srcKnown = true;
        let mut r#ref: Rc<RefCell<MdNode>> = MdNode::create("footnote_ref".to_string());
        // numbered before its own text is read, so a note inside it comes after
        self.notes.clone().borrow_mut().refer(&r#ref, fd.clone());
        let mut para: Rc<RefCell<MdNode>> = MdNode::create("paragraph".to_string());
        para.borrow_mut().open = false;
        // the paragraph's map is the slice of this block's map its text is
        // in, so every node in it says where it was typed
        let from: i64 = self.subjBase + label_start;
        let __len: i64 = label_end - label_start;
        let mut k: i64 = 0;
        while k < (self.owner.borrow().mapLit.len() as i64) {
            para.borrow_mut().addMapPiece(self.owner.borrow().mapLit[k as usize], self.owner.borrow().mapSrc[k as usize], self.owner.borrow().mapLen[k as usize]);
            k += 1;
        }
        para.borrow_mut().shiftMap(from, __len);
        para.borrow_mut().srcStart = self.srcAt(label_start);
        para.borrow_mut().srcEnd = self.srcAt(label_end);
        let body: String = MdText::substr(&self.subject, label_start, label_end);
        let mut inner: Rc<RefCell<MdInline>> = Rc::new(RefCell::new(MdInline::new()));
        inner.borrow_mut().entities = self.entities.clone();
        inner.borrow_mut().notes = self.notes.clone();
        inner.borrow_mut().parseFrom(para.clone(), &body, &self.refs.clone(), 0);
        fd.borrow_mut().add(para.clone());
        self.appendNode(r#ref.clone());
        self.pos = label_end + 1;
        true
    }
    /// Where the `]` that closes the `[` at `open` is, or -1.
    pub fn matchingBracket(&self, open: i64) -> i64 {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let mut level: i64 = 1;
        let mut i: i64 = open + 1;
        while i < n {
            let c: i64 = MdText::char_at(&self.subject, i);
            if c == 92 {
                i += 2;
                continue;
            }
            if c == 96 {
                // a code span is one token: its brackets are not brackets
                let mut run: i64 = 0;
                let mut j: i64 = i;
                while j < n && MdText::char_at(&self.subject, j) == 96 {
                    j += 1;
                    run += 1;
                }
                let close: i64 = MdInline::closingTicks(&self.subject.clone(), j, run);
                if close >= 0 {
                    i = close + run;
                } else {
                    i = j;
                }
                continue;
            }
            if c == 91 {
                level += 1;
            }
            if c == 93 {
                level -= 1;
                if level == 0 {
                    return i;
                }
            }
            i += 1;
        }
        -1
    }
    pub fn parseBang(&mut self) -> bool {
        let start_pos: i64 = self.pos;
        self.pos += 1;
        let nxt: i64 = self.peek();
        if nxt == 91 {
            self.pos += 1;
            let idx: i64 = self.addText("![");
            // The `[`, not the `!` and not the character after: an image's
            // shortcut label runs from the bracket to its partner.
            let bracket_pos: i64 = start_pos + 1;
            self.pushBracket(idx, bracket_pos, true);
            return true;
        }
        self.addText("!");
        true
    }
    pub fn pushBracket(&mut self, node: i64, index: i64, image: bool) {
        let n: i64 = self.brackets.len() as i64;
        if n > 0 {
            let mut top: Rc<RefCell<MdBracket>> = self.brackets[(n - 1) as usize].clone();
            top.borrow_mut().bracketAfter = true;
        }
        let mut b: Rc<RefCell<MdBracket>> = Rc::new(RefCell::new(MdBracket::new()));
        b.borrow_mut().node = node;
        b.borrow_mut().index = index;
        b.borrow_mut().image = image;
        b.borrow_mut().active = true;
        b.borrow_mut().prevDelim = self.delimTop;
        self.brackets.push(b.clone());
    }
    pub fn popBracket(&mut self) {
        let n: i64 = self.brackets.len() as i64;
        if n > 0 {
            self.brackets.pop();
        }
    }
    pub fn parseCloseBracket(&mut self) -> bool {
        self.pos += 1;
        let start_pos: i64 = self.pos;
        let bn: i64 = self.brackets.len() as i64;
        if bn == 0 {
            self.addText("]");
            return true;
        }
        let mut opener: Rc<RefCell<MdBracket>> = self.brackets[(bn - 1) as usize].clone();
        if false == opener.borrow().active {
            self.addText("]");
            self.popBracket();
            return true;
        }
        let is_image: bool = opener.borrow().image;
        let save_pos: i64 = self.pos;
        let mut matched: bool = false;
        let mut dest: String = String::new();
        let mut title: String = String::new();
        let nxt: i64 = self.peek();
        if nxt == 40 {
            self.pos += 1;
            self.spnl();
            let d: String = self.parseLinkDestination();
            if d != " FAIL" {
                self.spnl();
                let before_title: i64 = self.pos;
                let prev_ch: i64 = self.peekAt(self.pos - 1);
                if MdChar::isUnicodeWhitespace(prev_ch) {
                    let t: String = self.parseLinkTitle();
                    if t != " FAIL" {
                        title = t.clone();
                    } else {
                        self.pos = before_title;
                    }
                }
                self.spnl();
                let cl: i64 = self.peek();
                if cl == 41 {
                    self.pos += 1;
                    dest = d.clone();
                    matched = true;
                }
            }
            if false == matched {
                self.pos = save_pos;
                title = String::new();
            }
        }
        if false == matched {
            let before_label: i64 = self.pos;
            let n: i64 = self.parseLinkLabel();
            let mut reflabel: String = String::new();
            let mut have_label: bool = false;
            if n > 2 {
                reflabel = MdText::substr(&self.subject, before_label, before_label + n);
                have_label = true;
            } else {
                if false == opener.borrow().bracketAfter {
                    reflabel = MdText::substr(&self.subject, opener.borrow().index, start_pos);
                    have_label = true;
                }
            }
            if n == 0 {
                self.pos = save_pos;
            }
            if have_label {
                let key: String = MdInline::normalizeReference(&reflabel);
                if self.refs.contains_key(&key) {
                    let mut blank: Rc<RefCell<MdLinkRef>> = MdLinkRef::create(
                        String::new(),
                        String::new(),
                        String::new()
                    );
                    let mut r: Rc<RefCell<MdLinkRef>> = if self.refs.get(&key)
                        .cloned()
                        .is_some() { self.refs.get(&key).cloned().unwrap() } else { blank };
                    dest = r.borrow().destination.clone();
                    title = r.borrow().title.clone();
                    matched = true;
                }
            }
        }
        if matched {
            let kind: String = if is_image { "image".to_string() } else { "link".to_string() };
            let mut node: Rc<RefCell<MdNode>> = MdNode::create(kind.clone());
            node.borrow_mut().destination = dest.clone();
            node.borrow_mut().title = title.clone();
            // Its own step only consumed the `]` and what followed it. The
            // construct began at the `[` — or at the `!` for an image — which
            // `opener.index` remembered several steps ago.
            let mut open_index: i64 = opener.borrow().index;
            if is_image {
                open_index -= 1;
                if open_index < 0 {
                    open_index = 0;
                }
            }
            node.borrow_mut().srcStart = self.srcAt(open_index);
            let _tmp_213 = self.srcAt(self.pos.clone());
            node.borrow_mut().srcEnd = _tmp_213;
            node.borrow_mut().srcKnown = true;
            // Emphasis inside the link is resolved FIRST, while its nodes are
            // still on the flat list. The reference implementation does this
            // after moving them, because there the list IS the tree and a
            // moved node keeps its siblings; here the two are separate, and a
            // node moved into `node.children` can no longer be walked from
            // `nx`. Same pairs either way — `prevDelim` is what stops a `*`
            // inside the link pairing with one outside it.
            self.processEmphasis(opener.borrow().prevDelim.clone());
            let mut tmp: i64 = self.nx[(opener.borrow().node) as usize];
            while tmp >= 0 {
                let after: i64 = self.nx[tmp as usize];
                self.unlink(tmp);
                let mut moved: Rc<RefCell<MdNode>> = self.nodes[tmp as usize].clone();
                node.borrow_mut().add(moved.clone());
                tmp = after;
            }
            self.appendNode(node.clone());
            self.popBracket();
            self.unlink(opener.borrow().node.clone());
            if false == is_image {
                // No links inside links: every earlier link opener is spent.
                let mut i: i64 = 0;
                let cnt: i64 = self.brackets.len() as i64;
                while i < cnt {
                    let mut b: Rc<RefCell<MdBracket>> = self.brackets[i as usize].clone();
                    if false == b.borrow().image {
                        b.borrow_mut().active = false;
                    }
                    i += 1;
                }
            }
            return true;
        }
        self.popBracket();
        self.pos = start_pos;
        self.addText("]");
        true
    }
    /// Optional whitespace, at most one newline.
    pub fn spnl(&mut self) -> bool {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let mut seen_newline: bool = false;
        let mut going: bool = true;
        while going {
            if self.pos >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&self.subject, self.pos);
                if c == 10 {
                    if seen_newline {
                        going = false;
                    } else {
                        seen_newline = true;
                        self.pos += 1;
                    }
                } else {
                    if MdChar::isSpaceOrTab(c) {
                        self.pos += 1;
                    } else {
                        going = false;
                    }
                }
            }
        }
        true
    }
    /// `<a b>` or a bare run with balanced parentheses. The sentinel " FAIL"
    /// rather than an optional, so an EMPTY destination — which is legal —
    /// stays distinguishable from no destination at all.
    pub fn parseLinkDestination(&mut self) -> String {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let lead: i64 = self.peek();
        if lead == 60 {
            let mut i: i64 = self.pos + 1;
            let mut out: String = String::new();
            while i < n {
                let c: i64 = MdText::char_at(&self.subject, i);
                if c == 62 {
                    self.pos = i + 1;
                    return MdInline::unescape(&out);
                }
                if c == 60 {
                    return " FAIL".to_string();
                }
                if c == 10 {
                    return " FAIL".to_string();
                }
                if c == 92 {
                    let nc: i64 = self.peekAt(i + 1);
                    if MdChar::isAsciiPunct(nc) {
                        out = format!("{}{}", out, MdText::substr(&self.subject, i, i + 2));
                        i += 2;
                    } else {
                        out = format!("{}\\", out);
                        i += 1;
                    }
                } else {
                    out = format!("{}{}", out, MdText::substr(&self.subject, i, i + 1));
                    i += 1;
                }
            }
            return " FAIL".to_string();
        }
        let start: i64 = self.pos;
        let mut depth: i64 = 0;
        let mut i2: i64 = self.pos;
        let mut going: bool = true;
        while going {
            if i2 >= n {
                going = false;
            }
            if going {
                let c_1: i64 = MdText::char_at(&self.subject, i2);
                if c_1 == 92 {
                    let nc_1: i64 = self.peekAt(i2 + 1);
                    if MdChar::isAsciiPunct(nc_1) {
                        i2 += 2;
                    } else {
                        i2 += 1;
                    }
                } else {
                    if c_1 == 40 {
                        depth += 1;
                        if depth > 32 {
                            return " FAIL".to_string();
                        }
                        i2 += 1;
                    } else {
                        if c_1 == 41 {
                            if depth < 1 {
                                going = false;
                            } else {
                                depth -= 1;
                                i2 += 1;
                            }
                        } else {
                            if c_1 <= 32 {
                                // ASCII space and control, and nothing else:
                                // a non-breaking space is a character in a
                                // URL, not the end of one.
                                going = false;
                            } else {
                                if c_1 == 127 {
                                    going = false;
                                } else {
                                    i2 += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        if depth != 0 {
            return " FAIL".to_string();
        }
        if i2 == start {
            // `[a]()` has an empty destination; `[a]: ` has none at all.
            let next: i64 = self.peekAt(i2);
            if next != 41 {
                return " FAIL".to_string();
            }
        }
        let raw: String = MdText::substr(&self.subject, start, i2);
        self.pos = i2;
        MdInline::unescape(&raw)
    }
    pub fn parseLinkTitle(&mut self) -> String {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let c: i64 = self.peek();
        if c < 0 {
            return " FAIL".to_string();
        }
        let mut closer: i64 = 0;
        if c == 34 {
            closer = 34;
        }
        if c == 39 {
            closer = 39;
        }
        if c == 40 {
            closer = 41;
        }
        if closer == 0 {
            return " FAIL".to_string();
        }
        let mut i: i64 = self.pos + 1;
        let mut out: String = String::new();
        while i < n {
            let ch: i64 = MdText::char_at(&self.subject, i);
            if ch == closer {
                self.pos = i + 1;
                return MdInline::unescape(&out);
            }
            if c == 40 && ch == 40 {
                return " FAIL".to_string();
            }
            if ch == 92 {
                let nc: i64 = self.peekAt(i + 1);
                if MdChar::isAsciiPunct(nc) {
                    out = format!("{}{}", out, MdText::substr(&self.subject, i, i + 2));
                    i += 2;
                } else {
                    out = format!("{}\\", out);
                    i += 1;
                }
            } else {
                out = format!("{}{}", out, MdText::substr(&self.subject, i, i + 1));
                i += 1;
            }
        }
        " FAIL".to_string()
    }
    /// The length of a `[…]` label starting at `pos`, or 0. Consumes it.
    pub fn parseLinkLabel(&mut self) -> i64 {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let lead: i64 = self.peek();
        if lead != 91 {
            return 0;
        }
        let mut i: i64 = self.pos + 1;
        while i < n {
            let c: i64 = MdText::char_at(&self.subject, i);
            if c == 93 {
                let __len: i64 = (i + 1) - self.pos;
                if __len > 1002 {
                    return 0;
                }
                self.pos = i + 1;
                return __len;
            }
            if c == 91 {
                return 0;
            }
            if c == 92 {
                i += 2;
            } else {
                i += 1;
            }
        }
        0
    }
    /// ----------------------------------------------------------------------
    /// Entities, autolinks and raw HTML
    /// ----------------------------------------------------------------------
    pub fn parseEntity(&mut self) -> bool {
        let mut _tmp_216 = self.subject.clone();
        let mut _tmp_217 = self.pos.clone();
        let taken: i64 = MdInline::entityAt(&_tmp_216, _tmp_217, &self.entities.clone());
        if taken == 0 {
            return false;
        }
        let mut _tmp_219 = self.subject.clone();
        let mut _tmp_220 = self.pos.clone();
        let value: String = MdInline::entityValue(&_tmp_219, _tmp_220, taken, &self.entities.clone());
        self.addText(&value);
        self.pos += taken;
        true
    }
    /// `<https://example.com>` and `<user@example.com>`.
    pub fn parseAutolink(&mut self) -> bool {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let mut close: i64 = -1;
        let mut i: i64 = self.pos + 1;
        let mut going: bool = true;
        while going {
            if i >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&self.subject, i);
                if c == 62 {
                    close = i;
                    going = false;
                } else {
                    if c == 60 {
                        return false;
                    }
                    if MdChar::isUnicodeWhitespace(c) {
                        return false;
                    }
                    i += 1;
                }
            }
        }
        if close < 0 {
            return false;
        }
        let body: String = MdText::substr(&self.subject, self.pos + 1, close);
        let blen: i64 = MdText::str_len(&body) as i64;
        if blen == 0 {
            return false;
        }
        // The label is built as a CHILD rather than appended to the flat node
        // list, so the blanket stamp at the end of the step never reaches it —
        // it kept the default span of 0..0, which read as a node that had gone
        // backwards and showed up on fourteen of the specification's examples.
        // Its span is the text between the angle brackets.
        let body_start: i64 = self.pos + 1;
        if MdInline::isUriAutolink(&body) {
            let mut node: Rc<RefCell<MdNode>> = MdNode::create("link".to_string());
            node.borrow_mut().destination = body.clone();
            // an autolink's text IS its address: nothing typographic or
            // emoji may happen to it (`info` is what markdown-it calls it)
            node.borrow_mut().info = "auto".to_string();
            let mut label: Rc<RefCell<MdNode>> = MdNode::text(body.clone());
            self.stampNode(&label, body_start, close);
            node.borrow_mut().add(label.clone());
            self.appendNode(node.clone());
            self.pos = close + 1;
            return true;
        }
        if MdInline::isEmailAutolink(&body) {
            let mut node2: Rc<RefCell<MdNode>> = MdNode::create("link".to_string());
            node2.borrow_mut().destination = format!("mailto:{}", body);
            node2.borrow_mut().info = "auto".to_string();
            let mut label2: Rc<RefCell<MdNode>> = MdNode::text(body.clone());
            self.stampNode(&label2, body_start, close);
            node2.borrow_mut().add(label2.clone());
            self.appendNode(node2.clone());
            self.pos = close + 1;
            return true;
        }
        false
    }
    /// A raw HTML tag, comment, processing instruction, declaration or CDATA.
    /// Recognised and carried verbatim: the HTML exporter prints it, and the
    /// viewer shows it as what it is rather than obeying it.
    pub fn parseHtmlTag(&mut self) -> bool {
        let mut _tmp_222 = self.subject.clone();
        let taken: i64 = MdInline::htmlTagLength(&_tmp_222, self.pos.clone());
        if taken == 0 {
            return false;
        }
        let mut node: Rc<RefCell<MdNode>> = MdNode::create("html_inline".to_string());
        node.borrow_mut().literal = MdText::substr(&self.subject, self.pos, self.pos + taken);
        self.appendNode(node.clone());
        self.pos += taken;
        true
    }
    /// ----------------------------------------------------------------------
    /// Link reference definitions
    /// ----------------------------------------------------------------------
    ///
    /// `[label]: /url "title"` at the head of a paragraph. Called by the BLOCK
    /// parser, because a definition is not content: it is stripped off the
    /// front of the paragraph when the paragraph closes, and a paragraph that
    /// was nothing but definitions disappears. The scanners it needs — label,
    /// destination, title — are the ones a link already uses, which is why this
    /// lives here rather than there.
    ///
    /// Returns the number of characters consumed, or 0.
    pub fn parseReference(&mut self, text: String, mut _map: &mut HashMap<String,Rc<RefCell<MdLinkRef>>>) -> i64 {
        self.subject = text.clone();
        self.pos = 0;
        let start_pos: i64 = 0;
        let match_chars: i64 = self.parseLinkLabel();
        if match_chars == 0 {
            return 0;
        }
        let rawlabel: String = MdText::substr(&text, 0, match_chars);
        let colon: i64 = self.peek();
        if colon != 58 {
            return 0;
        }
        self.pos += 1;
        self.spnl();
        let dest: String = self.parseLinkDestination();
        if dest == " FAIL" {
            return 0;
        }
        let before_title: i64 = self.pos;
        self.spnl();
        let mut title: String = String::new();
        let mut have_title: bool = false;
        if self.pos != before_title {
            let t: String = self.parseLinkTitle();
            if t != " FAIL" {
                title = t.clone();
                have_title = true;
            }
        }
        if false == have_title {
            title = String::new();
            self.pos = before_title;
        }
        // Whatever follows has to be the end of the line. A title that is not
        // may still be a definition without it, so that case is retried.
        let mut at_end: bool = self.spaceToEndOfLine();
        if false == at_end {
            if false == have_title {
                return 0;
            }
            title = String::new();
            self.pos = before_title;
            at_end = self.spaceToEndOfLine();
            if false == at_end {
                return 0;
            }
        }
        let norm: String = MdInline::normalizeReference(&rawlabel);
        let nlen: i64 = MdText::str_len(&norm) as i64;
        if nlen == 0 {
            return 0;
        }
        if false == _map.contains_key(&norm) {
            let mut r#ref: Rc<RefCell<MdLinkRef>> = MdLinkRef::create(
                norm.clone(),
                dest.clone(),
                title.clone()
            );
            _map.insert(norm.clone(), r#ref.clone());
        }
        self.pos - start_pos
    }
    /// `*[HTML]: Hyper Text Markup Language` — markdown-it-abbr's definition,
    /// one line, at the head of what the block parser hands over. Like a link
    /// reference it is not content: the block parser strips it, and the label
    /// becomes an `abbr` wherever it stands as a whole word in the document's
    /// text (`MdTypography`). The first definition of a label wins.
    ///
    /// Returns the number of characters consumed, the newline included, or 0.
    pub fn parseAbbreviation(&self, text: &str) -> i64 {
        let n: i64 = MdText::str_len(&text) as i64;
        if n < 3 {
            return 0;
        }
        if MdText::char_at(&text, 0) != 42 {
            return 0;
        }
        if MdText::char_at(&text, 1) != 91 {
            return 0;
        }
        let mut i: i64 = 2;
        let mut label_end: i64 = -1;
        let mut label: String = String::new();
        while i < n && label_end < 0 {
            let c: i64 = MdText::char_at(&text, i);
            if c == 91 || c == 10 {
                return 0;
            }
            if c == 93 {
                label_end = i;
            } else {
                if c == 92 && i + 1 < n {
                    i += 1;
                }
                label = format!("{}{}", label, MdText::substr(&text, i, i + 1));
                i += 1;
            }
        }
        if label_end < 0 {
            return 0;
        }
        if label_end + 1 >= n {
            return 0;
        }
        if MdText::char_at(&text, label_end + 1) != 58 {
            return 0;
        }
        let mut eol: i64 = label_end + 2;
        while eol < n && MdText::char_at(&text, eol) != 10 {
            eol += 1;
        }
        let title: String = MdText::substr(&text, label_end + 2, eol).trim().to_string();
        if label.is_empty() {
            return 0;
        }
        if title.is_empty() {
            return 0;
        }
        self.opts.clone().borrow_mut().defineAbbreviation(&label, &title);
        if eol < n {
            return eol + 1;
        }
        eol
    }
    /// Spaces, then a newline or the end. Consumes the newline.
    pub fn spaceToEndOfLine(&mut self) -> bool {
        let n: i64 = MdText::str_len(&self.subject) as i64;
        let mut i: i64 = self.pos;
        let mut going: bool = true;
        while going {
            if i >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&self.subject, i);
                if MdChar::isSpaceOrTab(c) {
                    i += 1;
                } else {
                    going = false;
                }
            }
        }
        if i >= n {
            self.pos = i;
            return true;
        }
        let c2: i64 = MdText::char_at(&self.subject, i);
        if c2 == 10 {
            self.pos = i + 1;
            return true;
        }
        false
    }
}
