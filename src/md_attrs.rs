// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::md_node::MdNode;
use crate::rgr::MdText;

/// ============================================================================
/// MdAttrs.rgr — `{.class #id key=value}`, which is how Hugo says it.
/// ============================================================================
///
/// ````markdown
/// A list of items:
///
/// - Item 1
/// - Item 2
/// - Item 3
/// {.c .c3}
///
/// ## Revenue {.section-title}
/// ````
///
/// This is the syntax Goldmark reads and therefore the syntax a Hugo site is
/// already written in — and `PLAN_SLIDES.md` §3 is the argument for taking it
/// rather than MDX: MDX brings JSX, expressions, imports and component
/// resolution, and what is needed here is a class name on a list.
///
///   {.c .c3}      ──► classes  ["c", "c3"]
///   {#totals}     ──► id       "totals"
///   {cols=3}      ──► cols     "3"
///   {a="two words"} ──► a      "two words"
///
/// What the classes MEAN is nobody's business here; `MdCss` decides that, the
/// way `PptxCss` decides what a CSS property means to DrawingML. This file
/// reads characters.
///
/// **The grammar is strict on purpose**, and §8.3 of the plan is why: to
/// CommonMark an attribute line is an ordinary paragraph, and the 651/652
/// ratchet is the gate this must pass through without moving it. So `{not an
/// attribute}` is a paragraph, `{}` is a paragraph, `{.a b}` is a paragraph —
/// EVERY token must be a class, an id or a key=value pair, and there must be at
/// least one. A line that is nearly an attribute block is text.
///
/// A heading's own `{…}` is read more forgivingly (`headingAttrs`): a group
/// that starts like attributes never stays in the title, the parts that read
/// are applied, and each part that does not is a note on the node
/// (`MdNode.attrNotes`) for the writer to see.
/// ============================================================================
#[derive(Clone)]
pub struct MdAttrs {
}

impl MdAttrs {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn isSpaceCode(c: i64) -> bool {
        if c == 32 {
            return true;
        }
        if c == 9 {
            return true;
        }
        false
    }
    /// What a class, an id and a key may be spelled with: letters, digits,
    /// `-`, `_`, `:` and `.` — CSS's own identifier set, minus the escapes
    /// nobody writing a deck will type.
    pub fn isNameCode(c: i64) -> bool {
        if (c >= 97 && c <= 122) {
            return true;
        }
        if (c >= 65 && c <= 90) {
            return true;
        }
        if (c >= 48 && c <= 57) {
            return true;
        }
        if c == 45 {
            return true;
        }
        if c == 95 {
            return true;
        }
        if c == 58 {
            return true;
        }
        if c == 46 {
            return true;
        }
        false
    }
    /// ----------------------------------------------------------------------
    /// Reading one
    /// ----------------------------------------------------------------------
    ///
    /// `read` is the whole parser and everything else asks it. It returns how
    /// many tokens it accepted, or -1 when the text is not an attribute block —
    /// and when `n` is a real node it writes them onto it as it goes. Nothing
    /// is written when the answer is -1: a half-applied attribute block is
    /// worse than none, because the caller has already decided it is text.
    pub fn tokenCount(text: &str) -> i64 {
        let mut none: Rc<RefCell<MdNode>> = Rc::new(RefCell::new(MdNode::new()));
        MdAttrs::read(&none, text, false)
    }
    /// Is the whole of `text` one attribute block?
    pub fn isBlock(text: &str) -> bool {
        MdAttrs::tokenCount(text) > 0
    }
    pub fn applyTo(n: &Rc<RefCell<MdNode>>, text: &str) -> bool {
        MdAttrs::read(n, text, true) > 0
    }
    pub fn read(n: &Rc<RefCell<MdNode>>, text: &str, write: bool) -> i64 {
        let __len: i64 = MdText::str_len(&text) as i64;
        let mut a: i64 = 0;
        let mut b: i64 = __len;
        while a < b {
            if false == MdAttrs::isSpaceCode(MdText::char_at(&text, a)) {
                break;
            }
            a += 1;
        }
        while b > a {
            if false == MdAttrs::isSpaceCode(MdText::char_at(&text, b - 1)) {
                break;
            }
            b -= 1;
        }
        if b - a < 3 {
            return -1;
        }
        if MdText::char_at(&text, a) != 123 {
            return -1;
        }
        if MdText::char_at(&text, b - 1) != 125 {
            return -1;
        }
        // The `}` that closes it has to be the last character. One earlier
        // would mean this is not one block but two things with text between
        // them, which is a paragraph. A brace inside a quoted value is the
        // value's: `{title="a {b}"}` is one block.
        if MdAttrs::closeAt(text, a, b) != b - 1 {
            return -1;
        }
        let mut none: Vec<String> = Vec::new();
        MdAttrs::tokens(n, text, a + 1, b - 1, write, false, &mut none)
    }
    /// The tokens between a group's braces, from `i` to `stop`. Strict (`loose`
    /// false): the count, or -1 at the first that is not a token, having
    /// written nothing. Loose — a heading's, where the group has already been
    /// recognised as attributes — writes the good tokens, skips a bad one and
    /// says why in `notes`, and the count is of the good ones.
    pub fn tokens(n: &Rc<RefCell<MdNode>>, text: &str, i0: i64, stop: i64, write: bool, loose: bool, mut notes: &mut Vec<String>) -> i64 {
        let mut i: i64 = i0;
        // kramdown writes the same block `{: .lead}`
        if i < stop && MdText::char_at(&text, i) == 58 {
            let after: i64 = i + 1;
            if after >= stop {
                i = after;
            } else {
                let ac: i64 = MdText::char_at(&text, after);
                if MdAttrs::isSpaceCode(ac) || (ac == 46 || ac == 35) {
                    i = after;
                }
            }
        }
        if !loose && write {
            // all or nothing: check first, then write
            if MdAttrs::tokens(n, text, i, stop, false, false, &mut notes) < 0 {
                return -1;
            }
        }
        let mut found: i64 = 0;
        // the last good token, when it was a bare key=value: a word after it
        // is most likely the rest of its value
        let mut bare_from: i64 = -1;
        let mut bare_to: i64 = -1;
        while i < stop {
            if MdAttrs::isSpaceCode(MdText::char_at(&text, i)) {
                i += 1;
            } else {
                let taken: i64 = MdAttrs::token(n, text, i, stop, write);
                if taken < 0 {
                    if !loose {
                        return -1;
                    }
                    let skip: i64 = MdAttrs::badEnd(text, i, stop);
                    let frag: String = MdText::substr(&text, i, skip);
                    if bare_from >= 0 && MdText::index_of(&frag, "=") < 0 {
                        let pair: String = MdText::substr(&text, bare_from, bare_to);
                        let eq: i64 = MdText::index_of(&pair, "=");
                        let key: String = MdText::substr(&pair, 0, eq).trim().to_string();
                        let val: String = MdText::substr(&pair, eq + 1, (MdText::str_len(&pair) as i64)).trim().to_string();
                        notes.push(format!("\"{}\" is not a .class, an #id or key=value. If it is part of {}, quote the value: {}=\"{} {}\"", frag, key, key, val, frag));
                        bare_to = skip;
                    } else {
                        notes.push(MdAttrs::whyNot(&frag));
                        bare_from = -1;
                    }
                    i = skip;
                } else {
                    bare_from = -1;
                    if MdAttrs::isBarePair(text, i, taken) {
                        bare_from = i;
                        bare_to = taken;
                    }
                    i = taken;
                    found += 1;
                }
            }
        }
        if found == 0 && !loose {
            return -1;
        }
        found
    }
    /// Is the token from `i` to `e` a key with an unquoted value?
    pub fn isBarePair(text: &str, i: i64, e: i64) -> bool {
        let t: String = MdText::substr(&text, i, e);
        let eq: i64 = MdText::index_of(&t, "=");
        if eq <= 0 {
            return false;
        }
        let v: String = MdText::substr(&t, eq + 1, (MdText::str_len(&t) as i64)).trim().to_string();
        if v.is_empty() {
            return false;
        }
        let q: i64 = MdText::char_at(&v, 0);
        q != 34 && q != 39
    }
    /// Where a token that could not be read ends: at the next space, or past a
    /// quoted value that has spaces in it.
    pub fn badEnd(text: &str, i: i64, stop: i64) -> i64 {
        let mut e: i64 = i;
        let mut q: i64 = 0;
        while e < stop {
            let c: i64 = MdText::char_at(&text, e);
            if q != 0 {
                if c == 92 {
                    e += 1;
                } else {
                    if c == q {
                        q = 0;
                    }
                }
            } else {
                if MdAttrs::isSpaceCode(c) {
                    return e;
                }
                if c == 34 || c == 39 {
                    if MdAttrs::afterEquals(text, i, e) {
                        q = c;
                    }
                }
            }
            e += 1;
        }
        if e > stop {
            return stop;
        }
        e
    }
    /// What to tell the writer about a token that was left out.
    pub fn whyNot(frag: &str) -> String {
        let eq: i64 = MdText::index_of(&frag, "=");
        if eq > 0 {
            let v: String = MdText::substr(&frag, eq + 1, (MdText::str_len(&frag) as i64)).trim().to_string();
            if v.is_empty() {
                return format!("\"{}\" has no value; write {}something, or quote an empty one: {}\"\"", frag, frag, frag);
            }
            let q: i64 = MdText::char_at(&v, 0);
            if q == 34 || q == 39 {
                return format!("the quote in \"{}\" is never closed", frag);
            }
            return format!("\"{}\" could not be read; a key is letters, digits, - _ : and . — put a value with spaces in quotes", frag);
        }
        format!("\"{}\" is not a .class, an #id or key=value; a value with spaces goes in quotes: key=\"two words\"", frag)
    }
    /// One token, from `i`. Returns where it ended, or -1 when it is not one.
    pub fn token(n: &Rc<RefCell<MdNode>>, text: &str, i: i64, stop: i64, write: bool) -> i64 {
        let c: i64 = MdText::char_at(&text, i);
        if c == 46 {
            let e: i64 = MdAttrs::name(text, i + 1, stop);
            if e < 0 {
                return -1;
            }
            if false == MdAttrs::endsToken(text, e, stop) {
                return -1;
            }
            if write {
                n.borrow_mut().addClass(&MdText::substr(&text, i + 1, e));
            }
            return e;
        }
        if c == 35 {
            let e2: i64 = MdAttrs::name(text, i + 1, stop);
            if e2 < 0 {
                return -1;
            }
            if false == MdAttrs::endsToken(text, e2, stop) {
                return -1;
            }
            if write {
                n.borrow_mut().nodeId = MdText::substr(&text, i + 1, e2);
            }
            return e2;
        }
        // key=value, quoted or bare; `key = value` is the same
        let k: i64 = MdAttrs::name(text, i, stop);
        if k < 0 {
            return -1;
        }
        let key: String = MdText::substr(&text, i, k);
        let eq_at: i64 = MdAttrs::skipSpace(text, k, stop);
        if eq_at >= stop {
            return -1;
        }
        if MdText::char_at(&text, eq_at) != 61 {
            return -1;
        }
        let v: i64 = MdAttrs::skipSpace(text, eq_at + 1, stop);
        if v >= stop {
            return -1;
        }
        let q: i64 = MdText::char_at(&text, v);
        if q == 34 || q == 39 {
            // `\"` inside is a quote, `\\` a backslash
            let mut parts: Vec<String> = Vec::new();
            let mut from: i64 = v + 1;
            let mut e3: i64 = v + 1;
            while e3 < stop {
                let ec: i64 = MdText::char_at(&text, e3);
                if ec == 92 && e3 + 1 < stop {
                    let nx: i64 = MdText::char_at(&text, e3 + 1);
                    if nx == q || nx == 92 {
                        parts.push(MdText::substr(&text, from, e3));
                        from = e3 + 1;
                        e3 += 1;
                    }
                } else {
                    if ec == q {
                        let after: i64 = e3 + 1;
                        let ends: bool = MdAttrs::endsToken(text, after, stop);
                        if !ends {
                            return -1;
                        }
                        if write {
                            parts.push(MdText::substr(&text, from, e3));
                            n.borrow_mut().setAttr(&key, &parts.join(&*String::new()));
                        }
                        return e3 + 1;
                    }
                }
                e3 += 1;
            }
            return -1;
        }
        // A bare value is everything to the next space: a path or a share
        // (`bg=media/kansi.jpg`, `width=60%`), a code block's `lines=3-5,8|10`,
        // a colour (`background=#fff8`, `rgba(0,0,0,.5)`), a name with
        // letters ASCII does not have (`owner=Väinö`).
        let mut e4: i64 = v;
        while e4 < stop {
            let vc: i64 = MdText::char_at(&text, e4);
            if MdAttrs::isSpaceCode(vc) {
                break;
            }
            if vc == 123 || vc == 125 {
                return -1;
            }
            e4 += 1;
        }
        let mut ve: i64 = e4;
        // `{a=1, b=2}`: the comma between pairs is not the value's
        if ve - v > 1 && MdText::char_at(&text, ve - 1) == 44 {
            ve -= 1;
        }
        if write {
            n.borrow_mut().setAttr(&key, &MdText::substr(&text, v, ve));
        }
        e4
    }
    /// A token ends at a space or at the end; `.a.b` or `#x"y` is not one.
    pub fn endsToken(text: &str, e: i64, stop: i64) -> bool {
        if e >= stop {
            return true;
        }
        MdAttrs::isSpaceCode(MdText::char_at(&text, e))
    }
    pub fn skipSpace(text: &str, i: i64, stop: i64) -> i64 {
        let mut e: i64 = i;
        while e < stop {
            if false == MdAttrs::isSpaceCode(MdText::char_at(&text, e)) {
                break;
            }
            e += 1;
        }
        e
    }
    /// The end of a name starting at `i`, or -1 when there is not one there.
    pub fn name(text: &str, i: i64, stop: i64) -> i64 {
        let mut e: i64 = i;
        while e < stop {
            if false == MdAttrs::isNameCode(MdText::char_at(&text, e)) {
                break;
            }
            e += 1;
        }
        if e == i {
            return -1;
        }
        e
    }
    /// Is the quote at `at` a value's opening quote — after `=`, spaces
    /// allowed — rather than an apostrophe in a bare value (`it's`)?
    pub fn afterEquals(text: &str, from: i64, at: i64) -> bool {
        let mut k: i64 = at - 1;
        while k >= from {
            let c: i64 = MdText::char_at(&text, k);
            if false == MdAttrs::isSpaceCode(c) {
                return c == 61;
            }
            k -= 1;
        }
        false
    }
    /// Where the group opened by the `{` at `open` closes: the first `}` that is
    /// not inside a quoted value. -1 when nothing closes it before `stop`, -2
    /// when another `{` opens first.
    pub fn closeAt(text: &str, open: i64, stop: i64) -> i64 {
        let mut i: i64 = open + 1;
        let mut q: i64 = 0;
        while i < stop {
            let c: i64 = MdText::char_at(&text, i);
            if q != 0 {
                if c == 92 {
                    i += 1;
                } else {
                    if c == q {
                        q = 0;
                    }
                }
            } else {
                if c == 125 {
                    return i;
                }
                if c == 123 {
                    return -2;
                }
                if c == 34 || c == 39 {
                    if MdAttrs::afterEquals(text, open + 1, i) {
                        q = c;
                    }
                }
            }
            i += 1;
        }
        -1
    }
    /// `\{` is a brace the writer wanted drawn.
    pub fn escapedAt(text: &str, i: i64) -> bool {
        if i <= 0 {
            return false;
        }
        MdText::char_at(&text, i - 1) == 92
    }
    /// The `{` of the group that closes exactly at `close`, or -1: the first
    /// one from the left that does, so a `{` inside a quoted value
    /// (`{title="a { b"}`) is the value's and not the group's start.
    pub fn openOf(text: &str, close: i64) -> i64 {
        let mut j: i64 = 0;
        while j < close {
            if MdText::char_at(&text, j) == 123 && false == MdAttrs::escapedAt(text, j) {
                let stop_at: i64 = close + 1;
                if MdAttrs::closeAt(text, j, stop_at) == close {
                    return j;
                }
            }
            j += 1;
        }
        -1
    }
    /// ----------------------------------------------------------------------
    /// …and the one on the end of a heading
    /// ----------------------------------------------------------------------
    ///
    /// `## Revenue {.section-title}` — the block is the tail of the line, with
    /// whitespace before it. Returns where it starts, or -1. The caller trims
    /// `literal` there, so the heading's own text is what is drawn. This is
    /// the strict reading a code block's info string and a gallery cell use;
    /// a heading has `headingAttrs`, below.
    pub fn trailingAt(text: &str) -> i64 {
        let n: i64 = MdText::str_len(&text) as i64;
        if n < 4 {
            return -1;
        }
        if MdText::char_at(&text, n - 1) != 125 {
            return -1;
        }
        let i: i64 = MdAttrs::openOf(text, n - 1);
        if i <= 0 {
            return -1;
        }
        if false == MdAttrs::isSpaceCode(MdText::char_at(&text, i - 1)) {
            return -1;
        }
        if false == MdAttrs::isBlock(&MdText::substr(&text, i, n)) {
            return -1;
        }
        // The text before it has to be something. `# {.a}` is a heading whose
        // whole content is an attribute block, and CommonMark says that is a
        // heading with that text in it.
        let mut head: i64 = i - 1;
        while head > 0 {
            if false == MdAttrs::isSpaceCode(MdText::char_at(&text, head - 1)) {
                break;
            }
            head -= 1;
        }
        if head == 0 {
            return -1;
        }
        i
    }
    /// Does the group from `i` to `stop` read as attributes, by its first
    /// token? `.name`, `#name` (a letter first: `{#1 seller}` is words) or
    /// `key=`. `{in €}`, `{a, b}` and `{beta}` are words in braces.
    pub fn attrLike(text: &str, i0: i64, stop: i64) -> bool {
        let mut i: i64 = MdAttrs::skipSpace(text, i0, stop);
        if i < stop && MdText::char_at(&text, i) == 58 {
            i = MdAttrs::skipSpace(text, i + 1, stop);
        }
        if i >= stop {
            return false;
        }
        let c: i64 = MdText::char_at(&text, i);
        if c == 46 || c == 35 {
            if i + 1 >= stop {
                return false;
            }
            let f: i64 = MdText::char_at(&text, i + 1);
            if (f >= 97 && f <= 122) {
                return true;
            }
            if (f >= 65 && f <= 90) {
                return true;
            }
            return f == 95 || f == 45;
        }
        let k: i64 = MdAttrs::name(text, i, stop);
        if k < 0 {
            return false;
        }
        let eq_at: i64 = MdAttrs::skipSpace(text, k, stop);
        if eq_at >= stop {
            return false;
        }
        MdText::char_at(&text, eq_at) == 61
    }
    /// The attributes closing a heading's text, as forgiving as the title is
    /// worth: the title is what the audience sees, so a group that is plainly
    /// attributes never stays in it. Returns where the attributes start (the
    /// caller cuts the text there), or -1 when the heading has none.
    ///
    ///   ## Revenue {.a #b key=v}            one group
    ///   ## Revenue {transition=fade} {.a}   several, all read
    ///   ## Revenue{.a}                      no space before it
    ///   ## Revenue {title="a {b}"}          braces inside quotes are the value's
    ///   ## Revenue {.a b}                   .a read, `b` left out and said so
    ///   ## Revenue {transition=fade         no `}`: read, and said so
    ///   ## Price {in €} today               words in braces, the title's
    ///
    /// Every token that could not be read is a line in `notes`; the good ones
    /// are written onto `n`.
    pub fn headingAttrs(n: &Rc<RefCell<MdNode>>, text: &str, mut notes: &mut Vec<String>) -> i64 {
        let mut b: i64 = MdText::str_len(&text) as i64;
        while b > 0 {
            if false == MdAttrs::isSpaceCode(MdText::char_at(&text, b - 1)) {
                break;
            }
            b -= 1;
        }
        if b < 3 {
            return -1;
        }
        let mut cut: i64 = -1;
        let mut end: i64 = b;
        // closed groups, from the right
        while end > 0 {
            if MdText::char_at(&text, end - 1) != 125 {
                break;
            }
            let open: i64 = MdAttrs::openOf(text, end - 1);
            if open < 0 {
                break;
            }
            let inner: i64 = open + 1;
            let last: i64 = end - 1;
            let like: bool = MdAttrs::attrLike(text, inner, last);
            if !like {
                break;
            }
            cut = open;
            end = open;
            while end > 0 {
                if false == MdAttrs::isSpaceCode(MdText::char_at(&text, end - 1)) {
                    break;
                }
                end -= 1;
            }
        }
        // …or one never closed, at the very end: `{transition=fade`
        let mut open_end: bool = false;
        if cut < 0 {
            let mut j: i64 = b - 1;
            while j >= 0 {
                if MdText::char_at(&text, j) == 123 && false == MdAttrs::escapedAt(text, j) {
                    if MdAttrs::closeAt(text, j, b) == -1 && MdAttrs::attrLike(text, j + 1, b) {
                        cut = j;
                        end = j;
                        open_end = true;
                        while end > 0 {
                            if false == MdAttrs::isSpaceCode(MdText::char_at(&text, end - 1)) {
                                break;
                            }
                            end -= 1;
                        }
                    }
                    j = -1;
                }
                j -= 1;
            }
        }
        if cut < 0 {
            return -1;
        }
        // `# {.a}` — nothing before it — is a heading with that text in it
        if end == 0 {
            return -1;
        }
        if open_end {
            let body: String = MdText::substr(&text, cut, b);
            let had: i64 = notes.len() as i64;
            MdAttrs::tokens(n, text, cut + 1, b, true, true, &mut notes);
            // a `}` inside a quote that never closed has its own note
            if (notes.len() as i64) == had || MdText::char_at(&text, b - 1) != 125 {
                notes.push(format!("\"{}{}", body, "\" has no closing }; it was read as attributes".to_string()));
            }
            return cut;
        }
        let mut g: i64 = cut;
        while g < b {
            if MdText::char_at(&text, g) == 123 {
                let close: i64 = MdAttrs::closeAt(text, g, b);
                MdAttrs::tokens(n, text, g + 1, close, true, true, &mut notes);
                g = close + 1;
            } else {
                g += 1;
            }
        }
        cut
    }
    /// The headings anywhere in `doc` whose `{…}` had something that could not
    /// be read (MdNode.attrNotes), in document order, for a note to the writer.
    pub fn notedHeadings(doc: Rc<RefCell<MdNode>>) -> Vec<Rc<RefCell<MdNode>>> {
        let mut out: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        MdAttrs::collectNoted(doc.clone(), &mut out);
        out
    }
    pub fn collectNoted(node: Rc<RefCell<MdNode>>, mut out: &mut Vec<Rc<RefCell<MdNode>>>) {
        if node.borrow().kind == "heading" && !node.borrow().attrNotes.is_empty() {
            out.push(node.clone());
        }
        let mut i: i64 = 0;
        while i < node.borrow().childCount() {
            MdAttrs::collectNoted(node.borrow().childAt(i).clone(), &mut out);
            i += 1;
        }
    }
    /// `{heading=hidden}` (or `none` / `off`) on a heading: it names its
    /// slide, is in the contents and is read out, and is not drawn.
    pub fn headingHidden(n: &Rc<RefCell<MdNode>>) -> bool {
        if n.borrow().kind != "heading" {
            return false;
        }
        let v: String = MdChar::lower(&n.borrow().attrValue("heading").trim().to_string());
        (v == "hidden" || v == "none") || v == "off"
    }
}
