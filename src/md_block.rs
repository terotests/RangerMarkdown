// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::md_attrs::MdAttrs;
use crate::md_char::MdChar;
use crate::md_columns::MdColumns;
use crate::md_embed_kinds::MdEmbedKinds;
use crate::md_front_matter::MdFrontMatter;
use crate::md_inline::MdInline;
use crate::md_node::MdLinkRef;
use crate::md_node::MdNode;
use crate::md_node::MdNotes;
use crate::md_slide_layout::MdSlideLayout;
use crate::rgr::MdText;

/// ============================================================================
/// MdBlock.rgr — phase 1 of the CommonMark algorithm: the shape of a document.
/// ============================================================================
///
/// One pass, line by line, over a stack of blocks that are still open.
///
///   for each line
///       walk the stack from the top, asking each open block whether this line
///       continues it, and stop at the first that says no
///       try to open new blocks in what is left of the line
///       give the remainder to the deepest block that accepts text
///
/// Everything markdown does that surprises people falls out of those three
/// steps. A paragraph inside a block quote continues without its `>` because
/// the paragraph is still open and the quote was matched lazily. A list becomes
/// loose because a blank line was seen while an item was open. `---` is a
/// heading when a paragraph is open above it and a thematic break when one is
/// not — the same line, decided by the stack.
///
/// The stack is an array and everything refers to a block by its INDEX in it,
/// never by identity: closing a block is popping the array, and "the deepest
/// matched container" is a number. The reference implementation uses parent
/// pointers for this; an index is the same information without a cycle in the
/// tree, and Ranger has to serialise that tree on several targets.
///
/// What is deliberately not here: tabs are expanded by column exactly as the
/// specification says (a tab advances to the next four-column stop and may be
/// only partly consumed), because getting that wrong is silently wrong — a
/// document looks right and lays out with the wrong indentation.
/// ============================================================================
#[derive(Clone)]
pub struct MdBlock {
    pub doc: Rc<RefCell<MdNode>>,
    pub refs: HashMap<String,Rc<RefCell<MdLinkRef>>>,
    /// footnote definitions, as the blocks find them; numbered by the inlines
    pub notes: Rc<RefCell<MdNotes>>,
    pub inline: Rc<RefCell<MdInline>>,
    /// the open-block stack, root first
    pub stack: Vec<Rc<RefCell<MdNode>>>,
    /// the line being read
    pub line: String,
    pub lineNumber: i64,
    pub offset: i64,
    pub column: i64,
    pub nextNonspace: i64,
    pub nextNonspaceColumn: i64,
    pub indent: i64,
    pub indented: bool,
    pub blank: bool,
    pub partiallyConsumedTab: bool,
    pub allClosed: bool,
    pub lastMatched: i64,
    pub container: i64,
    /// where the current line starts in the document, for the source map
    pub lineStart: i64,
    /// What `parseListMarker` found. Fields rather than a record because the
    /// caller needs six values back and a function returns one.
    pub pendingOrdered: bool,
    pub pendingStart: i64,
    pub pendingDelim: String,
    pub pendingBullet: i64,
    pub pendingMarkerOffset: i64,
    pub pendingPadding: i64,
}

impl MdBlock {
    pub fn new() -> Self {
        Self {
            doc: MdNode::create("document".to_string()),
            refs: HashMap::new(),
            notes: Rc::new(RefCell::new(MdNotes::new())),
            inline: Rc::new(RefCell::new(MdInline::new())),
            stack: Vec::new(),
            line: String::new(),
            lineNumber: 0,
            offset: 0,
            column: 0,
            nextNonspace: 0,
            nextNonspaceColumn: 0,
            indent: 0,
            indented: false,
            blank: false,
            partiallyConsumedTab: false,
            allClosed: true,
            lastMatched: 0,
            container: 0,
            lineStart: 0,
            pendingOrdered: false,
            pendingStart: 1,
            pendingDelim: String::new(),
            pendingBullet: 0,
            pendingMarkerOffset: 0,
            pendingPadding: 0,
        }
    }
    pub fn codeIndent() -> i64 {
        4
    }
    /// ----------------------------------------------------------------------
    /// Entry point
    /// ----------------------------------------------------------------------
    /// A document as a deck reads it: CommonMark and GFM, markdown-it's
    /// plugins (`~sub~ ^sup^ ++ins++ ==mark==`, `:emoji:`, `*[abbr]:`) and its
    /// typographer, which the front matter can switch off (`typographer:
    /// false`).
    pub fn parseText(text: &str) -> Rc<RefCell<MdNode>> {
        let mut p: Rc<RefCell<MdBlock>> = Rc::new(RefCell::new(MdBlock::new()));
        return p.borrow_mut().run(text);
    }
    /// …and as the specification reads it: no plugins and no typographer,
    /// whatever the front matter says. What the parity score is taken against,
    /// since `"` and `--` and `^` are characters to CommonMark.
    pub fn parseCommonMark(text: &str) -> Rc<RefCell<MdNode>> {
        let mut p: Rc<RefCell<MdBlock>> = Rc::new(RefCell::new(MdBlock::new()));
        p.borrow().inline.borrow().opts.clone().borrow_mut().commonMarkOnly();
        return p.borrow_mut().run(text);
    }
    pub fn looksLikeYaml(lines: &[String], from: i64, to: i64) -> bool {
        let mut any: bool = false;
        let mut i: i64 = from;
        while i < to {
            let ln: String = lines[i as usize].clone();
            let t: String = ln.trim().to_string();
            let n: i64 = MdText::str_len(&t) as i64;
            if n > 0 {
                let first: i64 = MdText::char_at(&t, 0);
                if first != 35 {
                    let mut colon: i64 = -1;
                    let mut j: i64 = 0;
                    while j < n {
                        let c: i64 = MdText::char_at(&t, j);
                        if c == 58 && colon < 0 {
                            colon = j;
                        }
                        j += 1;
                    }
                    if colon < 1 {
                        return false;
                    }
                    any = true;
                }
            }
            i += 1;
        }
        any
    }
    /// Line endings, and the null character the specification replaces.
    /// CR LF and lone CR to LF, NUL to U+FFFD. The runs between them are
    /// copied whole and joined once (one character at a time was quadratic
    /// on the Go target: seconds for a 100 kB deck).
    pub fn normalizeSource(text: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut from: i64 = 0;
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&text) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&text, i);
            if c == 13 || c == 0 {
                if i > from {
                    parts.push(MdText::substr(&text, from, i));
                }
                if c == 13 {
                    parts.push("\n".to_string().to_string());
                    if i + 1 < n && MdText::char_at(&text, i + 1) == 10 {
                        i += 1;
                    }
                } else {
                    parts.push(MdChar::fromCode(65533));
                }
                from = i + 1;
            }
            i += 1;
        }
        if from == 0 {
            return text.to_string();
        }
        if n > from {
            parts.push(MdText::substr(&text, from, n));
        }
        parts.join(&*String::new())
    }
    pub fn canContain(parent_kind: &str, child_kind: &str) -> bool {
        if parent_kind == "document" {
            return child_kind != "item";
        }
        if parent_kind == "block_quote" {
            return child_kind != "item";
        }
        if parent_kind == "item" {
            return child_kind != "item";
        }
        if parent_kind == "list" {
            return child_kind == "item";
        }
        // the plugins' containers hold blocks as a quote does; a definition
        // list holds its terms and definitions and nothing else
        if parent_kind == "footnote_def" {
            return child_kind != "item";
        }
        if parent_kind == "container" {
            return child_kind != "item";
        }
        if parent_kind == "dd" {
            return child_kind != "item";
        }
        if parent_kind == "dl" {
            return child_kind == "dd";
        }
        false
    }
    pub fn acceptsLines(kind: &str) -> bool {
        if kind == "paragraph" {
            return true;
        }
        if kind == "code_block" {
            return true;
        }
        if kind == "html_block" {
            return true;
        }
        false
    }
    /// The first character of anything that can start a block. Purely a
    /// shortcut: a line of ordinary prose never gets as far as the starts.
    pub fn maybeSpecial(c: i64) -> bool {
        // `{` opens a Goldmark attribute block. It is the only character here
        // that CommonMark has no block meaning for at all, and that is the
        // whole risk: `startAttributes` refuses everything that is not exactly
        // one, so a paragraph beginning with a brace stays a paragraph.
        if c == 123 {
            return true;
        }
        if c == 35 {
            return true;
        }
        if c == 96 {
            return true;
        }
        if c == 126 {
            return true;
        }
        if c == 42 {
            return true;
        }
        if c == 43 {
            return true;
        }
        if c == 95 {
            return true;
        }
        if c == 61 {
            return true;
        }
        if c == 60 {
            return true;
        }
        if c == 62 {
            return true;
        }
        if c == 45 {
            return true;
        }
        // `:` opens a `::: container` or a `: definition`; `[` a `[^note]:`
        if c == 58 {
            return true;
        }
        if c == 91 {
            return true;
        }
        if MdChar::isDigit(c) {
            return true;
        }
        false
    }
    /// The length of a closing fence at `from`, or 0 when the rest of the line
    /// is not one: a closer needs the same character, at least as many of them,
    /// and nothing after it.
    pub fn closingFenceLength(ln: &str, from: i64, ch: i64) -> i64 {
        let n: i64 = MdText::str_len(&ln) as i64;
        let mut i: i64 = from;
        let mut count: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&ln, i);
            if c != ch {
                i = n;
            } else {
                count += 1;
                i += 1;
            }
        }
        if count < 3 {
            return 0;
        }
        let mut j: i64 = from + count;
        while j < n {
            let c2: i64 = MdText::char_at(&ln, j);
            if false == MdChar::isSpaceOrTab(c2) {
                return 0;
            }
            j += 1;
        }
        count
    }
    /// A word a sheet can name as a class: letters, digits, `-` and `_`.
    pub fn isContainerName(name: &str) -> bool {
        let n: i64 = MdText::str_len(&name) as i64;
        if n == 0 {
            return false;
        }
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&name, i);
            let mut ok: bool = MdChar::isAlnum(c);
            if c == 45 || c == 95 {
                ok = true;
            }
            if c > 127 {
                ok = true;
            }
            if false == ok {
                return false;
            }
            i += 1;
        }
        true
    }
    /// "## foo ##" is `foo`; "## foo #bar" is `foo #bar`; "## #" is empty.
    pub fn stripAtxClosing(raw: &str) -> String {
        let t: String = raw.trim().to_string();
        let n: i64 = MdText::str_len(&t) as i64;
        if n == 0 {
            return String::new();
        }
        let mut i: i64 = n;
        let mut hashes: i64 = 0;
        let mut scanning: bool = true;
        while scanning {
            if i <= 0 {
                scanning = false;
            }
            if scanning {
                let c: i64 = MdText::char_at(&t, i - 1);
                if c == 35 {
                    hashes += 1;
                    i -= 1;
                } else {
                    scanning = false;
                }
            }
        }
        if hashes == 0 {
            return t;
        }
        if i == 0 {
            return String::new();
        }
        let before: i64 = MdText::char_at(&t, i - 1);
        if false == MdChar::isSpaceOrTab(before) {
            return t;
        }
        MdText::substr(&t, 0, i).trim().to_string()
    }
    pub fn setextLevel(rest: &str) -> i64 {
        let t: String = rest.trim().to_string();
        let n: i64 = MdText::str_len(&t) as i64;
        if n == 0 {
            return 0;
        }
        let c: i64 = MdText::char_at(&t, 0);
        if c != 61 && c != 45 {
            return 0;
        }
        let mut i: i64 = 0;
        while i < n {
            let c2: i64 = MdText::char_at(&t, i);
            if c2 != c {
                return 0;
            }
            i += 1;
        }
        if c == 61 {
            return 1;
        }
        2
    }
    pub fn isThematicBreak(rest: &str) -> bool {
        let n: i64 = MdText::str_len(&rest) as i64;
        let mut ch: i64 = 0;
        let mut count: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&rest, i);
            if MdChar::isSpaceOrTab(c) {
                i += 1;
            } else {
                if (c != 42 && c != 45) && c != 95 {
                    return false;
                }
                if ch == 0 {
                    ch = c;
                }
                if c != ch {
                    return false;
                }
                count += 1;
                i += 1;
            }
        }
        count >= 3
    }
    pub fn listsMatch(lst: &Rc<RefCell<MdNode>>, ordered: bool, delim: &str, bullet: i64) -> bool {
        if lst.borrow().listOrdered != ordered {
            return false;
        }
        if ordered {
            return lst.borrow().listDelim == delim;
        }
        return lst.borrow().listBulletChar == bullet;
    }
    /// ----------------------------------------------------------------------
    /// HTML blocks: seven start conditions, five end conditions
    /// ----------------------------------------------------------------------
    pub fn htmlBlockOpens(t: i64, rest: &str) -> bool {
        let n: i64 = MdText::str_len(&rest) as i64;
        if n < 2 {
            return false;
        }
        // only the start is lowered: the longest name the kinds 1, 5 and 6
        // look for ("</" and a ten-letter tag name and the character after
        // it) fits in 32, and a long line is not lowered whole seven times
        let mut head: String = rest.to_string();
        if n > 32 {
            head = MdText::substr(&rest, 0, 32);
        }
        let low: String = MdChar::lower(&head);
        let ln: i64 = MdText::str_len(&low) as i64;
        if t == 1 {
            if MdBlock::startsWithAny(&low, "<script", "<pre", "<textarea", "<style") {
                let name: i64 = MdBlock::tagNameEndFrom(&low, 1);
                let mut after: i64 = -1;
                if name < ln {
                    after = MdText::char_at(&low, name);
                }
                if after < 0 {
                    return true;
                }
                if after == 62 {
                    return true;
                }
                if MdChar::isUnicodeWhitespace(after) {
                    return true;
                }
            }
            return false;
        }
        if t == 2 {
            return MdBlock::hasPrefix(rest, "<!--");
        }
        if t == 3 {
            return MdBlock::hasPrefix(rest, "<?");
        }
        if t == 4 {
            if false == MdBlock::hasPrefix(rest, "<!") {
                return false;
            }
            if n < 3 {
                return false;
            }
            return MdChar::isLetter(MdText::char_at(&rest, 2));
        }
        if t == 5 {
            return MdBlock::hasPrefix(&low, "<![cdata[");
        }
        if t == 6 {
            let mut i: i64 = 1;
            if MdText::char_at(&low, 1) == 47 {
                i = 2;
            }
            let mut j: i64 = i;
            while j < ln {
                let c: i64 = MdText::char_at(&low, j);
                if false == MdChar::isAlnum(c) {
                    j = ln;
                } else {
                    j += 1;
                }
            }
            let end: i64 = MdBlock::tagNameEndFrom(&low, i);
            let name_1: String = MdText::substr(&low, i, end);
            if false == MdBlock::isBlockTag(&name_1) {
                return false;
            }
            let mut after_1: i64 = -1;
            if end < ln {
                after_1 = MdText::char_at(&low, end);
            }
            if after_1 < 0 {
                return true;
            }
            if after_1 == 62 {
                return true;
            }
            if MdChar::isUnicodeWhitespace(after_1) {
                return true;
            }
            if after_1 == 47 {
                let mut after2: i64 = -1;
                if end + 1 < ln {
                    after2 = MdText::char_at(&low, end + 1);
                }
                if after2 == 62 {
                    return true;
                }
            }
            return false;
        }
        if t == 7 {
            let taken: i64 = MdInline::htmlTagLength(rest, 0);
            if taken == 0 {
                return false;
            }
            let mut k: i64 = taken;
            while k < n {
                let c2: i64 = MdText::char_at(&rest, k);
                if false == MdChar::isUnicodeWhitespace(c2) {
                    return false;
                }
                k += 1;
            }
            return true;
        }
        false
    }
    pub fn htmlBlockCloses(t: i64, rest: &str) -> bool {
        let low: String = MdChar::lower(rest);
        if t == 1 {
            if MdBlock::hasSub(&low, "</script>") {
                return true;
            }
            if MdBlock::hasSub(&low, "</pre>") {
                return true;
            }
            if MdBlock::hasSub(&low, "</textarea>") {
                return true;
            }
            if MdBlock::hasSub(&low, "</style>") {
                return true;
            }
            return false;
        }
        if t == 2 {
            return MdBlock::hasSub(rest, "-->");
        }
        if t == 3 {
            return MdBlock::hasSub(rest, "?>");
        }
        if t == 4 {
            return MdBlock::hasSub(rest, ">");
        }
        if t == 5 {
            return MdBlock::hasSub(rest, "]]>");
        }
        false
    }
    pub fn tagNameEndFrom(low: &str, from: i64) -> i64 {
        let n: i64 = MdText::str_len(&low) as i64;
        let mut j: i64 = from;
        let mut going: bool = true;
        while going {
            if j >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&low, j);
                if MdChar::isAlnum(c) {
                    j += 1;
                } else {
                    going = false;
                }
            }
        }
        j
    }
    pub fn hasPrefix(v: &str, prefix: &str) -> bool {
        let n: i64 = MdText::str_len(&prefix) as i64;
        let m: i64 = MdText::str_len(&v) as i64;
        if m < n {
            return false;
        }
        MdText::substr(&v, 0, n) == prefix
    }
    pub fn hasSub(v: &str, needle: &str) -> bool {
        let n: i64 = MdText::str_len(&needle) as i64;
        let m: i64 = MdText::str_len(&v) as i64;
        let mut i: i64 = 0;
        while i + n <= m {
            if MdText::substr(&v, i, i + n) == needle {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn startsWithAny(low: &str, a: &str, b: &str, c: &str, d: &str) -> bool {
        if MdBlock::hasPrefix(low, a) {
            return true;
        }
        if MdBlock::hasPrefix(low, b) {
            return true;
        }
        if MdBlock::hasPrefix(low, c) {
            return true;
        }
        if MdBlock::hasPrefix(low, d) {
            return true;
        }
        false
    }
    /// The block-level tag names of HTML block type 6, as the specification
    /// lists them. A name not here starts a type 7 block instead, which is the
    /// one that may not interrupt a paragraph.
    pub fn isBlockTag(name: &str) -> bool {
        let names: String = " address article aside base basefont blockquote body caption center col colgroup dd details dialog dir div dl dt fieldset figcaption figure footer form frame frameset h1 h2 h3 h4 h5 h6 head header hr html iframe legend li link main menu menuitem nav noframes ol optgroup option p param search section summary table tbody td tfoot th thead title tr track ul ".to_string();
        let probe: String = format!(" {} ", name);
        MdBlock::hasSub(&names, &probe)
    }
    /// ----------------------------------------------------------------------
    /// Closing a block
    /// ----------------------------------------------------------------------
    /// How many characters `trim` would take off the front. The map has to be
    /// shifted by exactly that, and asking the trimmed result how much was
    /// taken is the only way to be sure it matches whatever `trim` did.
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
    pub fn joinLines(lines: &[String]) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = lines.len() as i64;
        while i < n {
            out = format!("{}{}\n", out, lines[i as usize].clone());
            i += 1;
        }
        out
    }
    /// ----------------------------------------------------------------------
    /// …and where an attribute block ends up
    /// ----------------------------------------------------------------------
    ///
    /// `{.c .c3}` describes the block ABOVE it, so the node the parser made for
    /// the line is emptied into that block and removed. Two things follow from
    /// the fold rather than from the parse, and both matter:
    ///
    ///   * The attributed block's span grows to cover the attribute line. It is
    ///     part of that block's source now, which is what makes the layout's
    ///     per-block cache notice when somebody edits the classes — the cache
    ///     is keyed by the exact source a block was built from.
    ///
    ///   * An attribute block with nothing above it becomes a PARAGRAPH. The
    ///     text was in the file and a reader should see it; §8.2 of the plan is
    ///     the rule — anything not supported is left as literal text rather
    ///     than guessed at or quietly dropped.
    ///
    /// A heading carries its own on the same line, which is the other spelling
    /// Goldmark accepts, and that is trimmed off `literal` here for the same
    /// reason: the drawn heading is the heading's text.
    pub fn foldAttributes(n: &Rc<RefCell<MdNode>>) {
        if n.borrow().kind == "heading" {
            MdBlock::stripHeadingAttrs(n);
        }
        if n.borrow().kind == "html_block" {
            MdBlock::stripHtmlAttrs(n);
        }
        let mut kept: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let mut i: i64 = 0;
        let cnt: i64 = n.borrow().childCount();
        while i < cnt {
            let mut k: Rc<RefCell<MdNode>> = n.borrow().childAt(i);
            let mut taken: bool = false;
            if k.borrow().kind == "attrs" {
                let have: i64 = kept.len() as i64;
                if have > 0 {
                    let mut prev: Rc<RefCell<MdNode>> = kept[(have - 1) as usize].clone();
                    prev.borrow_mut().takeAttrsFrom(&k);
                    if k.borrow().srcEnd > prev.borrow().srcEnd {
                        prev.borrow_mut().srcEnd = k.borrow().srcEnd;
                    }
                    taken = true;
                } else {
                    k.borrow_mut().kind = "paragraph".to_string();
                }
            }
            if false == taken {
                kept.push(k.clone());
            }
            i += 1;
        }
        n.borrow_mut().children = kept.clone();
        let mut j: i64 = 0;
        while j < (kept.len() as i64) {
            MdBlock::foldAttributes(&kept[j as usize].clone());
            j += 1;
        }
    }
    /// An HTML block runs on to the blank line, so `{cells=…}` written on the
    /// line under `</table>` is the block's last line. It is the block's
    /// attributes, as it is under any other block.
    pub fn stripHtmlAttrs(n: &Rc<RefCell<MdNode>>) {
        let lit: String = n.borrow().literal.clone();
        let mut end: i64 = MdText::str_len(&lit) as i64;
        while end > 0 && MdAttrs::isSpaceCode(MdText::char_at(&lit, end - 1)) {
            end -= 1;
        }
        let nl: i64 = MdText::last_index_of(&MdText::substr(&lit, 0, end), "\n");
        if nl < 0 {
            return;
        }
        let tail: String = MdText::substr(&lit, nl + 1, end).trim().to_string();
        if tail.is_empty() || MdText::char_at(&tail, 0) != 123 {
            return;
        }
        if false == MdAttrs::isBlock(&tail) {
            return;
        }
        if false == MdAttrs::applyTo(n, &tail) {
            return;
        }
        n.borrow_mut().literal = MdText::substr(&lit, 0, nl + 1);
        n.borrow_mut().shiftMap(0, nl + 1);
    }
    pub fn stripHeadingAttrs(n: &Rc<RefCell<MdNode>>) {
        // the notes into a list of its own, then onto the node: a field
        // handed to a function that pushes to it is a copy on Rust
        let mut notes: Vec<String> = Vec::new();
        let at: i64 = MdAttrs::headingAttrs(n, &n.borrow().literal.clone(), &mut notes);
        for note in notes.iter().cloned() {
            n.borrow_mut().attrNotes.push(note.clone());
        }
        if at < 0 {
            return;
        }
        let mut cut: i64 = at;
        while cut > 0 {
            if false == MdAttrs::isSpaceCode(MdText::char_at(&n.borrow().literal, cut - 1)) {
                break;
            }
            cut -= 1;
        }
        let literal_v = MdText::substr(&n.borrow().literal, 0, cut);
        n.borrow_mut().literal = literal_v;
        n.borrow_mut().shiftMap(0, cut);
    }
    /// The info strings that name a drawing rather than a language. The list
    /// itself is in `MdEmbedKinds`, which has no imports and is read by the
    /// renderer too — the parser deciding "code" while the renderer decides
    /// "diagram" leaves a hole nobody fills, so there is one table and both
    /// read it.
    pub fn isEmbedInfo(info: &str) -> bool {
        let first: String = MdBlock::firstWord(info);
        MdEmbedKinds::isDrawing(&first)
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
    pub fn stripTrailingNewlines(v: &str) -> String {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut i: i64 = n;
        let mut going: bool = true;
        while going {
            if i <= 0 {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&v, i - 1);
                if c == 10 {
                    i -= 1;
                } else {
                    going = false;
                }
            }
        }
        MdText::substr(&v, 0, i)
    }
    /// An indented code block loses the blank lines at its end, and keeps one
    /// newline: `    foo\n\n\n` is one line of code.
    ///
    /// Only spaces that FOLLOW a newline go: `    foo  ` keeps its two trailing
    /// spaces, because they are code and not layout.
    pub fn stripTrailingBlankLines(v: &str) -> String {
        let n: i64 = MdText::str_len(&v) as i64;
        let mut cut: i64 = n;
        let mut i: i64 = n;
        let mut going: bool = true;
        while going {
            if i <= 0 {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&v, i - 1);
                if c == 32 {
                    i -= 1;
                } else {
                    if c == 10 {
                        i -= 1;
                        cut = i;
                    } else {
                        going = false;
                    }
                }
            }
        }
        if cut == 0 {
            return String::new();
        }
        format!("{}\n", MdText::substr(&v, 0, cut))
    }
    pub fn listIsTight(lst: &Rc<RefCell<MdNode>>) -> bool {
        let items: i64 = lst.borrow().childCount();
        let mut i: i64 = 0;
        while i < items {
            let mut item: Rc<RefCell<MdNode>> = lst.borrow().childAt(i);
            let is_last: bool = i == items - 1;
            let item_blank: bool = MdBlock::endsWithBlankLine(&item);
            if item_blank && false == is_last {
                return false;
            }
            let kids: i64 = item.borrow().childCount();
            let mut j: i64 = 0;
            while j < kids {
                let mut sub: Rc<RefCell<MdNode>> = item.borrow().childAt(j);
                let sub_last: bool = j == kids - 1;
                if MdBlock::endsWithBlankLine(&sub) {
                    if false == is_last || false == sub_last {
                        return false;
                    }
                }
                j += 1;
            }
            i += 1;
        }
        true
    }
    /// markdown-it-deflist's rule: a definition list is loose when a blank
    /// line ends a definition that something follows in the list, or when
    /// one separates two blocks inside a definition. A blank line between a
    /// term and its definition does not loosen it.
    pub fn dlIsTight(dl: &Rc<RefCell<MdNode>>) -> bool {
        let n: i64 = dl.borrow().childCount();
        let mut i: i64 = 0;
        while i < n {
            let mut dd: Rc<RefCell<MdNode>> = dl.borrow().childAt(i);
            if dd.borrow().kind == "dd" {
                if dd.borrow().lastLineBlank && i < n - 1 {
                    return false;
                }
                let kids: i64 = dd.borrow().childCount();
                let mut j: i64 = 0;
                while j < kids - 1 {
                    if MdBlock::endsWithBlankLine(&dd.borrow().childAt(j)) {
                        return false;
                    }
                    j += 1;
                }
            }
            i += 1;
        }
        true
    }
    /// A block's own flag, or — for a list or an item — the flag of whatever
    /// it ends with. The flag is reset on every ancestor of the line being
    /// read, so by the time a list's tightness is decided its own flag may be
    /// false while the item that actually met the blank line still says true.
    pub fn endsWithBlankLine(node: &Rc<RefCell<MdNode>>) -> bool {
        let mut cur: Rc<RefCell<MdNode>> = node.clone();
        let mut going: bool = true;
        let mut guard: i64 = 0;
        while going {
            if cur.borrow().lastLineBlank {
                return true;
            }
            let kind: String = cur.borrow().kind.clone();
            let mut descend: bool = false;
            if kind == "list" {
                descend = true;
            }
            if kind == "item" {
                descend = true;
            }
            if descend && cur.borrow().hasChildren() {
                let last: Rc<RefCell<MdNode>> = cur.borrow().lastChild();
                cur = last;
            } else {
                going = false;
            }
            guard += 1;
            if guard > 1000 {
                going = false;
            }
        }
        false
    }
    /// One cell, given the place in the FILE its text came from.
    ///
    /// The piece is the whole cell: `literal` is the trimmed cell text and the
    /// span says where exactly those characters are, so the map is one piece at
    /// literal offset 0. Everything downstream — `MdInline.srcAt`, the layout's
    /// `segSrc`, a click — is then the ordinary path a paragraph already takes.
    pub fn stampCell(cell: &Rc<RefCell<MdNode>>, para: &Rc<RefCell<MdNode>>, spans: &[i64], index: i64, line_off: i64) {
        let at: i64 = index * 2;
        if at + 1 >= (spans.len() as i64) {
            return;
        }
        let off: i64 = spans[at as usize];
        if off < 0 {
            return;
        }
        let __len: i64 = spans[(at + 1) as usize];
        if __len <= 0 {
            return;
        }
        if __len != (MdText::str_len(&cell.borrow().literal) as i64) {
            return;
        }
        let src: i64 = para.borrow().srcOfLiteral(line_off + off);
        if src < 0 {
            return;
        }
        cell.borrow_mut().srcStart = src;
        cell.borrow_mut().srcEnd = src + __len;
        cell.borrow_mut().srcKnown = true;
        cell.borrow_mut().addMapPiece(0, src, __len);
    }
    /// The same split `splitRow` does, in OFFSETS rather than in strings: two
    /// ints per cell — where the cell's trimmed text begins in `raw`, and how
    /// long it is.
    ///
    /// A table cell had no source range at all. `maybeTable` gave every cell a
    /// `literal` and no map, so `srcOfLiteral` answered -1 for every character
    /// in the grid, the inline nodes under it were unstamped, and the layout
    /// stamped each cell's boxes with the TABLE's own start. A click anywhere
    /// in a table therefore put the caret on the opening pipe — which is why
    /// `PLAN_WYSIWYG.md` §5 Stage H listed "editing a table cell in place" as
    /// open. It is a source range like any other; it only had to say so.
    ///
    /// A cell that contains a `\` gets a start of -1 and stays unmapped:
    /// `splitRow` collapses `\|` to `|`, so the string it returns is shorter
    /// than the source it came from and there is no one-to-one map to write.
    /// Better one cell a click cannot reach than a whole table pointing one
    /// character out from there on.
    pub fn rowSpans(raw: &str) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        let t: String = raw.trim().to_string();
        let n: i64 = MdText::str_len(&t) as i64;
        if n == 0 {
            return out;
        }
        let mut start: i64 = 0;
        let mut end: i64 = n;
        if MdText::char_at(&t, 0) == 124 {
            start = 1;
        }
        if n > 1 {
            let last_ch: i64 = MdText::char_at(&t, n - 1);
            let prev: i64 = MdText::char_at(&t, n - 2);
            if last_ch == 124 && prev != 92 {
                end = n - 1;
            }
        }
        if start >= end {
            return out;
        }
        // Where the unescaped pipes are, plus the end of the row.
        let mut cuts: Vec<i64> = Vec::new();
        let mut i: i64 = start;
        while i < end {
            let c: i64 = MdText::char_at(&t, i);
            if c == 92 {
                i += 2;
            } else {
                if c == 124 {
                    cuts.push(i);
                }
                i += 1;
            }
        }
        cuts.push(end);
        let lead: i64 = MdBlock::leadTrim(raw);
        let mut from: i64 = start;
        let mut k: i64 = 0;
        let cn: i64 = cuts.len() as i64;
        while k < cn {
            let to: i64 = cuts[k as usize];
            let mut a: i64 = from;
            let mut b: i64 = to;
            while a < b {
                let ca: i64 = MdText::char_at(&t, a);
                if ca == 32 {
                    a += 1;
                } else {
                    if ca == 9 {
                        a += 1;
                    } else {
                        break;
                    }
                }
            }
            while b > a {
                let cb: i64 = MdText::char_at(&t, b - 1);
                if cb == 32 {
                    b -= 1;
                } else {
                    if cb == 9 {
                        b -= 1;
                    } else {
                        break;
                    }
                }
            }
            let mut esc: bool = false;
            let mut e: i64 = a;
            while e < b {
                if MdText::char_at(&t, e) == 92 {
                    esc = true;
                }
                e += 1;
            }
            if esc {
                out.push(-1);
                out.push(0);
            } else {
                out.push(a + lead);
                out.push(b - a);
            }
            from = to + 1;
            k += 1;
        }
        out
    }
    /// `| --- | :-: |` → the alignments, or an empty list when the line is not
    /// a delimiter row at all.
    pub fn parseDelimiterRow(raw: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cells: Vec<String> = MdBlock::splitRow(raw);
        let n: i64 = cells.len() as i64;
        if n == 0 {
            return out;
        }
        let mut i: i64 = 0;
        while i < n {
            let cell: String = cells[i as usize].clone().trim().to_string();
            let m: i64 = MdText::str_len(&cell) as i64;
            if m == 0 {
                out.clear();
                return out;
            }
            let mut left: bool = false;
            let mut right: bool = false;
            let mut start: i64 = 0;
            let mut end: i64 = m;
            if MdText::char_at(&cell, 0) == 58 {
                left = true;
                start = 1;
            }
            if MdText::char_at(&cell, m - 1) == 58 {
                right = true;
                end = m - 1;
            }
            if start >= end {
                out.clear();
                return out;
            }
            let mut j: i64 = start;
            while j < end {
                let c: i64 = MdText::char_at(&cell, j);
                if c != 45 {
                    out.clear();
                    return out;
                }
                j += 1;
            }
            let mut a: String = "".to_string();
            if left && right {
                a = "center".to_string();
            }
            if left && false == right {
                a = "left".to_string();
            }
            if false == left && right {
                a = "right".to_string();
            }
            out.push(a.to_string());
            i += 1;
        }
        out
    }
    /// A row into its cells: the outer pipes are optional, and `\|` is a pipe
    /// in a cell rather than a cell boundary.
    pub fn splitRow(raw: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let t: String = raw.trim().to_string();
        let n: i64 = MdText::str_len(&t) as i64;
        if n == 0 {
            return out;
        }
        let mut start: i64 = 0;
        let mut end: i64 = n;
        if MdText::char_at(&t, 0) == 124 {
            start = 1;
        }
        if n > 1 {
            let last_ch: i64 = MdText::char_at(&t, n - 1);
            let prev: i64 = MdText::char_at(&t, n - 2);
            if last_ch == 124 && prev != 92 {
                end = n - 1;
            }
        }
        if start >= end {
            return out;
        }
        let mut cur: String = String::new();
        let mut i: i64 = start;
        while i < end {
            let c: i64 = MdText::char_at(&t, i);
            if c == 92 {
                let mut nc: i64 = -1;
                if i + 1 < end {
                    nc = MdText::char_at(&t, i + 1);
                }
                if nc == 124 {
                    cur = format!("{}|", cur);
                    i += 2;
                } else {
                    cur = format!("{}{}", cur, MdText::substr(&t, i, i + 1));
                    i += 1;
                }
            } else {
                if c == 124 {
                    out.push(cur.clone());
                    cur = String::new();
                    i += 1;
                } else {
                    cur = format!("{}{}", cur, MdText::substr(&t, i, i + 1));
                    i += 1;
                }
            }
        }
        out.push(cur);
        out
    }
    /// ----------------------------------------------------------------------
    /// $$ displays
    /// ----------------------------------------------------------------------
    ///
    ///     Display equations:
    ///     $$\\frac{a}{b}$$
    ///
    /// A `$$` that begins a line of a paragraph opens a display, which runs to
    /// the `$$` that ends a line — the same line, or one further down. It is
    /// not a block start of its own: TeX writers put a display right under the
    /// sentence that introduces it, with no blank line, and CommonMark would
    /// read that as one paragraph. So the paragraph is cut after the fact, into
    /// the text before, the `math_block`, and the text after, each keeping its
    /// share of the source map.
    pub fn splitMath(node: &Rc<RefCell<MdNode>>) {
        if node.borrow().isLeaf() {
            return;
        }
        let mut out: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let mut changed: bool = false;
        let mut i: i64 = 0;
        let n: i64 = node.borrow().childCount();
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            if c.borrow().kind == "paragraph" && MdText::index_of(&c.borrow().literal, "$$") >= 0 {
                let mut parts: Vec<Rc<RefCell<MdNode>>> = MdBlock::mathPieces(&c);
                if (parts.len() as i64) > 1 {
                    changed = true;
                } else {
                    if (parts.len() as i64) == 1 {
                        let mut only: Rc<RefCell<MdNode>> = parts[0].clone();
                        if only.borrow().kind != "paragraph" {
                            changed = true;
                        }
                    }
                }
                let mut k: i64 = 0;
                while k < (parts.len() as i64) {
                    out.push(parts[k as usize].clone());
                    k += 1;
                }
            } else {
                MdBlock::splitMath(&c);
                out.push(c.clone());
            }
            i += 1;
        }
        if changed {
            node.borrow_mut().children = out.clone();
        }
    }
    /// The paragraph cut at its displays; itself alone when it has none.
    pub fn mathPieces(p: &Rc<RefCell<MdNode>>) -> Vec<Rc<RefCell<MdNode>>> {
        let mut out: Vec<Rc<RefCell<MdNode>>> = Vec::new();
        let lit: String = p.borrow().literal.clone();
        let n: i64 = MdText::str_len(&lit) as i64;
        let mut text_from: i64 = 0;
        let mut line_start: i64 = 0;
        while line_start < n {
            let line_end: i64 = MdBlock::lineEndAt(&lit, line_start);
            let a: i64 = line_start + MdInline::leadTrim(&MdText::substr(&lit, line_start, line_end));
            let mut opens: bool = false;
            if a + 1 < line_end {
                opens = MdText::char_at(&lit, a) == 36 && MdText::char_at(&lit, a + 1) == 36;
            }
            let mut close_end: i64 = -1;
            let mut close_at: i64 = -1;
            if opens {
                // the first `$$` after the opener that ends its line
                let mut j: i64 = a + 2;
                while j + 1 < n {
                    if MdText::char_at(&lit, j) == 36 && MdText::char_at(&lit, j + 1) == 36 {
                        let le: i64 = MdBlock::lineEndAt(&lit, j);
                        if (MdText::substr(&lit, j + 2, le).trim().to_string()).is_empty() {
                            close_at = j;
                            close_end = le;
                            j = n;
                        } else {
                            j += 2;
                        }
                    } else {
                        if MdText::char_at(&lit, j) == 92 {
                            j += 2;
                        } else {
                            j += 1;
                        }
                    }
                }
            }
            if close_at >= 0 {
                if line_start > text_from {
                    out.push(MdBlock::slicePara(p, text_from, line_start, "paragraph"));
                }
                let mut m: Rc<RefCell<MdNode>> = MdBlock::slicePara(p, a, close_at + 2, "math_block");
                m.borrow_mut().literal = MdText::substr(&lit, a + 2, close_at).trim().to_string();
                out.push(m.clone());
                text_from = close_end + 1;
                line_start = text_from;
            } else {
                line_start = line_end + 1;
            }
        }
        if out.is_empty() {
            out.push(p.clone());
            return out;
        }
        if text_from < n {
            if !MdText::substr(&lit, text_from, n).trim().to_string().is_empty() {
                out.push(MdBlock::slicePara(p, text_from, n, "paragraph"));
            }
        }
        // the paragraph's {.attributes} go with its last piece
        let mut last: Rc<RefCell<MdNode>> = out[((out.len() as i64) - 1) as usize].clone();
        last.borrow_mut().takeAttrsFrom(p);
        out
    }
    pub fn lineEndAt(s: &str, from: i64) -> i64 {
        let n: i64 = MdText::str_len(&s) as i64;
        let mut i: i64 = from;
        while i < n {
            if MdText::char_at(&s, i) == 10 {
                return i;
            }
            i += 1;
        }
        n
    }
    /// literal[a,b) of a paragraph as a node of its own, with its source map.
    pub fn slicePara(p: &Rc<RefCell<MdNode>>, a: i64, b0: i64, kind: &str) -> Rc<RefCell<MdNode>> {
        let mut b: i64 = b0;
        let lit: String = p.borrow().literal.clone();
        // a paragraph piece does not end in the newline before the display
        while b > a && MdText::char_at(&lit, b - 1) == 10 {
            b -= 1;
        }
        let mut q: Rc<RefCell<MdNode>> = MdNode::create(kind.to_string());
        q.borrow_mut().literal = MdText::substr(&lit, a, b);
        q.borrow_mut().open = false;
        let mut i: i64 = 0;
        while i < (p.borrow().mapLit.len() as i64) {
            q.borrow_mut().addMapPiece(p.borrow().mapLit[i as usize], p.borrow().mapSrc[i as usize], p.borrow().mapLen[i as usize]);
            i += 1;
        }
        q.borrow_mut().shiftMap(a, b - a);
        q.borrow_mut().srcStart = p.borrow().srcOfLiteral(a);
        q.borrow_mut().srcEnd = p.borrow().srcOfLiteral(b);
        if q.borrow().srcStart < 0 {
            q.borrow_mut().srcStart = p.borrow().srcStart;
        }
        if q.borrow().srcEnd < 0 {
            q.borrow_mut().srcEnd = p.borrow().srcEnd;
        }
        q
    }
    /// GFM task lists: an item whose first paragraph opens with `[ ]` or `[x]`
    /// is a checkbox, and the marker is not part of the text. Returns the text
    /// with the marker taken off.
    pub fn markTaskItem(raw: &str, parent: &Rc<RefCell<MdNode>>) -> String {
        if parent.borrow().kind != "item" {
            return raw.to_string();
        }
        let n: i64 = MdText::str_len(&raw) as i64;
        if n < 3 {
            return raw.to_string();
        }
        let open: i64 = MdText::char_at(&raw, 0);
        let close: i64 = MdText::char_at(&raw, 2);
        if open != 91 {
            return raw.to_string();
        }
        if close != 93 {
            return raw.to_string();
        }
        let mark: i64 = MdText::char_at(&raw, 1);
        let mut state: i64 = -1;
        if mark == 32 {
            state = 0;
        }
        if mark == 120 {
            state = 1;
        }
        if mark == 88 {
            state = 1;
        }
        if state < 0 {
            return raw.to_string();
        }
        if n > 3 {
            let after: i64 = MdText::char_at(&raw, 3);
            if false == MdChar::isSpaceOrTab(after) {
                return raw.to_string();
            }
        }
        parent.borrow_mut().taskState = state;
        let rest: String = MdText::substr(&raw, 3, n);
        MdBlock::stripOneLeadingSpace(&rest)
    }
    pub fn stripOneLeadingSpace(v: &str) -> String {
        let n: i64 = MdText::str_len(&v) as i64;
        if n == 0 {
            return v.to_string();
        }
        let c: i64 = MdText::char_at(&v, 0);
        if MdChar::isSpaceOrTab(c) {
            return MdText::substr(&v, 1, n);
        }
        v.to_string()
    }
    pub fn run(&mut self, text: &str) -> Rc<RefCell<MdNode>> {
        self.doc = MdNode::create("document".to_string());
        self.notes = Rc::new(RefCell::new(MdNotes::new()));
        self.stack.clear();
        self.stack.push(self.doc.clone());
        self.lineNumber = 0;
        self.lineStart = 0;
        let src: String = MdBlock::normalizeSource(text);
        let mut lines: Vec<String> = src.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        let n: i64 = lines.len() as i64;
        // A trailing newline makes `strsplit` produce one empty line that was
        // never in the document. Reading it would end a paragraph twice.
        let mut count: i64 = n;
        if count > 0 {
            let last_line: String = lines[(count - 1) as usize].clone();
            let last_len: i64 = MdText::str_len(&last_line) as i64;
            let src_len: i64 = MdText::str_len(&src) as i64;
            if last_len == 0 && src_len > 0 {
                count -= 1;
            }
        }
        let mut i: i64 = self.frontMatter(&lines, count);
        // Before the first block: an abbreviation is defined while the
        // blocks are read, and only a document that reads the plugins has any.
        let arg_0 = MdFrontMatter::of(&self.doc.clone()).clone();
        self.inline.borrow().opts.clone().borrow_mut().readMeta(&arg_0);
        while i < count {
            let ln: String = lines[i as usize].clone();
            self.incorporateLine(ln.clone());
            let ln_len: i64 = MdText::str_len(&ln) as i64;
            self.lineStart = (self.lineStart + ln_len) + 1;
            i += 1;
        }
        let mut guard: i64 = 0;
        let mut depth: i64 = self.stack.len() as i64;
        while depth > 0 {
            self.finalizeTop();
            depth = self.stack.len() as i64;
            guard += 1;
            if guard > 100000 {
                depth = 0;
            }
        }
        MdBlock::foldAttributes(&self.doc.clone());
        MdBlock::splitMath(&self.doc.clone());
        self.inline.borrow_mut().notes = self.notes.clone();
        let mut _tmp_345 = self.doc.clone();
        self.processInlines(_tmp_345, &self.doc.clone(), 0);
        // `{layout=columns}` sections and side-by-side `::: col` blocks
        MdColumns::wrapLayouts(&self.doc.clone());
        MdSlideLayout::hideStatementTitles(&self.doc.clone());
        self.doc.borrow_mut().notes = self.notes.borrow().list.clone();
        self.doc.borrow_mut().readingSig = (self.inline.borrow().opts).borrow().signature();
        self.doc.clone()
    }
    /// YAML front matter: `---` on the very first line, and everything to the
    /// next `---` or `...`. Not CommonMark — under CommonMark the first line is
    /// a thematic break and the rest is prose — but it is how every markdown
    /// document that has metadata carries it, and a viewer that renders
    /// `title: …` as a paragraph is a viewer nobody would use.
    ///
    /// Returns the line to start the real parse at.
    pub fn frontMatter(&mut self, lines: &[String], count: i64) -> i64 {
        if count < 3 {
            return 0;
        }
        let first: String = lines[0].clone();
        if first != "---" {
            return 0;
        }
        let mut i: i64 = 1;
        while i < count {
            let ln: String = lines[i as usize].clone();
            if ln == "---" || ln == "..." {
                // Every non-empty line between the fences has to carry a key,
                // or this is not metadata. Without the test, `---` over `Foo`
                // over `---` — a thematic break and a setext heading, which
                // the specification has an example for — is eaten as though
                // it were front matter.
                if false == MdBlock::looksLikeYaml(lines, 1, i) {
                    return 0;
                }
                let mut node: Rc<RefCell<MdNode>> = MdNode::create("front_matter".to_string());
                let mut body: String = String::new();
                let mut j: i64 = 1;
                while j < i {
                    body = format!("{}{}\n", body, lines[j as usize].clone());
                    j += 1;
                }
                node.borrow_mut().literal = body.clone();
                node.borrow_mut().srcStart = 0;
                self.doc.clone().borrow_mut().add(node.clone());
                let mut k: i64 = 0;
                while k <= i {
                    let consumed: String = lines[k as usize].clone();
                    let clen: i64 = MdText::str_len(&consumed) as i64;
                    self.lineStart = (self.lineStart + clen) + 1;
                    k += 1;
                }
                // …and where it ended, which is where the real parse begins.
                // It had none — front matter is consumed before the block
                // parser starts, so nothing later stamps it — and a block whose
                // span is empty is a block nothing downstream can tell apart
                // from another one.
                node.borrow_mut().srcEnd = self.lineStart;
                return i + 1;
            }
            i += 1;
        }
        0
    }
    /// ----------------------------------------------------------------------
    /// The stack
    /// ----------------------------------------------------------------------
    pub fn tipIndex(&self) -> i64 {
        let n: i64 = self.stack.len() as i64;
        n - 1
    }
    pub fn tip(&self) -> Rc<RefCell<MdNode>> {
        let n: i64 = self.stack.len() as i64;
        self.stack[(n - 1) as usize].clone()
    }
    pub fn nodeAt(&self, i: i64) -> Rc<RefCell<MdNode>> {
        self.stack[i as usize].clone()
    }
    pub fn addChild(&mut self, kind: &str, start_col: i64) -> Rc<RefCell<MdNode>> {
        let mut going: bool = true;
        while going {
            let mut t: Rc<RefCell<MdNode>> = self.tip();
            if MdBlock::canContain(&t.borrow().kind.clone(), kind) {
                going = false;
            } else {
                self.finalizeTop();
            }
        }
        let mut node: Rc<RefCell<MdNode>> = MdNode::create(kind.to_string());
        node.borrow_mut().srcStart = self.lineStart + start_col;
        let srcEnd_v = node.borrow().srcStart;
        node.borrow_mut().srcEnd = srcEnd_v;
        node.borrow_mut().firstLineNo = self.lineNumber;
        node.borrow_mut().lastLineNo = self.lineNumber;
        let mut parent: Rc<RefCell<MdNode>> = self.tip();
        parent.borrow_mut().add(node.clone());
        self.stack.push(node.clone());
        self.container = self.tipIndex();
        node
    }
    pub fn closeUnmatchedBlocks(&mut self) {
        if self.allClosed {
            return;
        }
        let mut going: bool = true;
        while going {
            let top: i64 = self.tipIndex();
            if top > self.lastMatched {
                self.finalizeTop();
            } else {
                going = false;
            }
        }
        self.allClosed = true;
    }
    /// ----------------------------------------------------------------------
    /// Scanning the current line
    /// ----------------------------------------------------------------------
    pub fn charAtOffset(&self, i: i64) -> i64 {
        let n: i64 = MdText::str_len(&self.line) as i64;
        if i < 0 {
            return -1;
        }
        if i >= n {
            return -1;
        }
        MdText::char_at(&self.line, i)
    }
    pub fn findNextNonspace(&mut self) {
        let mut i: i64 = self.offset;
        let mut cols: i64 = self.column;
        let n: i64 = MdText::str_len(&self.line) as i64;
        let mut going: bool = true;
        while going {
            if i >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&self.line, i);
                if c == 32 {
                    i += 1;
                    cols += 1;
                } else {
                    if c == 9 {
                        i += 1;
                        cols += 4 - cols % 4;
                    } else {
                        going = false;
                    }
                }
            }
        }
        self.blank = i >= n;
        self.nextNonspace = i;
        self.nextNonspaceColumn = cols;
        self.indent = self.nextNonspaceColumn - self.column;
        self.indented = self.indent >= 4;
    }
    /// `columns` true measures the advance in COLUMNS, which is what a partly
    /// consumed tab means: `>\tfoo` inside a quote leaves two of the tab's four
    /// columns behind, and they are content.
    pub fn advanceOffset(&mut self, count: i64, columns: bool) {
        let n: i64 = MdText::str_len(&self.line) as i64;
        let mut left: i64 = count;
        let mut going: bool = true;
        while going {
            if left <= 0 {
                going = false;
            }
            if going {
                if self.offset >= n {
                    going = false;
                }
            }
            if going {
                let c: i64 = MdText::char_at(&self.line, self.offset);
                if c == 9 {
                    let chars_to_tab: i64 = 4 - self.column % 4;
                    if columns {
                        let is_partial: bool = chars_to_tab > left;
                        self.partiallyConsumedTab = is_partial;
                        let adv: i64 = if is_partial { left } else { chars_to_tab };
                        self.column += adv;
                        if false == is_partial {
                            self.offset += 1;
                        }
                        left -= adv;
                    } else {
                        self.partiallyConsumedTab = false;
                        self.column += chars_to_tab;
                        self.offset += 1;
                        left -= 1;
                    }
                } else {
                    self.partiallyConsumedTab = false;
                    self.offset += 1;
                    self.column += 1;
                    left -= 1;
                }
            }
        }
    }
    pub fn advanceNextNonspace(&mut self) {
        self.offset = self.nextNonspace;
        self.column = self.nextNonspaceColumn;
        self.partiallyConsumedTab = false;
    }
    pub fn addLine(&mut self) {
        let mut t: Rc<RefCell<MdNode>> = self.tip();
        let mut prefix: String = String::new();
        let mut tab_at: i64 = -1;
        if self.partiallyConsumedTab {
            // The tab is the character at `offset`, and it is being replaced by
            // the spaces that carry the line to the next tab stop. Those spaces
            // are not in the document: one source character stands for all of
            // them, so the map names the tab once and the rest of the run has
            // no source. See MdNode's header.
            tab_at = self.lineStart + self.offset;
            self.offset += 1;
            let chars_to_tab: i64 = 4 - self.column % 4;
            let mut k: i64 = 0;
            while k < chars_to_tab {
                prefix = format!("{} ", prefix);
                k += 1;
            }
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let rest: String = MdText::substr(&self.line, self.offset, n);
        t.borrow_mut().lines.push(format!("{}{}", prefix, rest));
        t.borrow_mut().lastLineNo = self.lineNumber;
        // …and where every character of it came from. Built here, against the
        // raw text, because this is the only place that still knows both.
        let pad: i64 = MdText::str_len(&prefix) as i64;
        let base: i64 = t.borrow().rawLen;
        if pad > 0 {
            t.borrow_mut().addMapPiece(base, tab_at, 1);
        }
        let rest_len: i64 = MdText::str_len(&rest) as i64;
        t.borrow_mut().addMapPiece(base + pad, self.lineStart + self.offset, rest_len);
        // The newline `joinLines` puts back is the one the document already
        // had at the end of this line. A caret at the end of a line is a place
        // a reader puts one, so it is mapped rather than left out.
        t.borrow_mut().addMapPiece((base + pad) + rest_len, self.lineStart + n, 1);
        t.borrow_mut().rawLen = ((base + pad) + rest_len) + 1;
    }
    /// ----------------------------------------------------------------------
    /// One line
    /// ----------------------------------------------------------------------
    pub fn incorporateLine(&mut self, ln: String) {
        self.line = ln.clone();
        self.lineNumber += 1;
        self.offset = 0;
        self.column = 0;
        self.blank = false;
        self.partiallyConsumedTab = false;
        let old_tip: i64 = self.tipIndex();
        let mut all_matched: bool = true;
        self.container = 0;
        let mut idx: i64 = 1;
        let depth: i64 = self.stack.len() as i64;
        let mut going: bool = true;
        while going {
            if idx >= depth {
                going = false;
            }
            if going {
                let mut node: Rc<RefCell<MdNode>> = self.stack[idx as usize].clone();
                self.findNextNonspace();
                let r: i64 = self.continueBlock(&node, idx);
                if r == 0 {
                    self.container = idx;
                    idx += 1;
                }
                if r == 1 {
                    all_matched = false;
                    self.container = idx - 1;
                    going = false;
                }
                if r == 2 {
                    // the line was a closing fence, and is spent
                    return;
                }
            }
        }
        self.allClosed = self.container == old_tip;
        self.lastMatched = self.container;
        let mut t0: Rc<RefCell<MdNode>> = self.stack[(self.container) as usize].clone();
        let mut matched_leaf: bool = false;
        if t0.borrow().kind != "paragraph" {
            matched_leaf = MdBlock::acceptsLines(&t0.borrow().kind);
        }
        let mut trying: bool = true;
        while trying {
            if matched_leaf {
                trying = false;
            }
            if trying {
                self.findNextNonspace();
                let lead: i64 = self.charAtOffset(self.nextNonspace.clone());
                if false == self.indented && false == MdBlock::maybeSpecial(lead) {
                    self.advanceNextNonspace();
                    trying = false;
                } else {
                    let r_1: i64 = self.tryStarts();
                    if r_1 == 0 {
                        self.advanceNextNonspace();
                        trying = false;
                    }
                    if r_1 == 1 {
                        self.container = self.tipIndex();
                    }
                    if r_1 == 2 {
                        self.container = self.tipIndex();
                        matched_leaf = true;
                    }
                }
            }
        }
        let mut tip_now: Rc<RefCell<MdNode>> = self.tip();
        if (false == self.allClosed && false == self.blank) && tip_now.borrow().kind == "paragraph" {
            // a lazy continuation line: the paragraph is still open even
            // though the container that holds it was not matched
            self.addLine();
            return;
        }
        self.closeUnmatchedBlocks();
        let mut cont: Rc<RefCell<MdNode>> = self.stack[(self.container) as usize].clone();
        let has_kids: bool = cont.borrow().hasChildren();
        if self.blank && has_kids {
            let mut last_kid: Rc<RefCell<MdNode>> = cont.borrow().lastChild();
            last_kid.borrow_mut().lastLineBlank = true;
        }
        let mut last_line_blank: bool = self.blank;
        if cont.borrow().kind == "block_quote" {
            last_line_blank = false;
        }
        if cont.borrow().kind == "code_block" && cont.borrow().fenced {
            last_line_blank = false;
        }
        if cont.borrow().kind == "item" {
            let kids: bool = cont.borrow().hasChildren();
            if false == kids && cont.borrow().srcStart >= self.lineStart {
                last_line_blank = false;
            }
        }
        let mut up: i64 = self.container;
        while up >= 0 {
            let mut anc: Rc<RefCell<MdNode>> = self.stack[up as usize].clone();
            anc.borrow_mut().lastLineBlank = last_line_blank;
            up -= 1;
        }
        if MdBlock::acceptsLines(&cont.borrow().kind.clone()) {
            self.addLine();
            if cont.borrow().kind == "html_block" {
                if cont.borrow().htmlBlockType >= 1 && cont.borrow().htmlBlockType <= 5 {
                    let n: i64 = MdText::str_len(&self.line) as i64;
                    let rest: String = MdText::substr(&self.line, self.offset, n);
                    if MdBlock::htmlBlockCloses(cont.borrow().htmlBlockType.clone(), &rest) {
                        self.finalizeThrough(self.container.clone());
                    }
                }
            }
            return;
        }
        let n2: i64 = MdText::str_len(&self.line) as i64;
        if self.offset < n2 && false == self.blank {
            self.addChild("paragraph", self.offset.clone());
            self.advanceNextNonspace();
            self.addLine();
        }
    }
    /// Close everything down to and including the block at `idx`.
    pub fn finalizeThrough(&mut self, idx: i64) {
        let mut going: bool = true;
        while going {
            let top: i64 = self.tipIndex();
            if top >= idx {
                self.finalizeTop();
            } else {
                going = false;
            }
            if top <= 0 {
                going = false;
            }
        }
    }
    /// ----------------------------------------------------------------------
    /// Does this line continue that block?  0 yes, 1 no, 2 the line is spent
    /// ----------------------------------------------------------------------
    pub fn continueBlock(&mut self, node: &Rc<RefCell<MdNode>>, idx: i64) -> i64 {
        let kind: String = node.borrow().kind.clone();
        if kind == "block_quote" {
            let c: i64 = self.charAtOffset(self.nextNonspace.clone());
            if false == self.indented && c == 62 {
                self.advanceNextNonspace();
                self.advanceOffset(1, false);
                let after: i64 = self.charAtOffset(self.offset.clone());
                if MdChar::isSpaceOrTab(after) {
                    self.advanceOffset(1, true);
                }
                return 0;
            }
            return 1;
        }
        if kind == "item" {
            if self.blank {
                let kids: bool = node.borrow().hasChildren();
                if false == kids {
                    return 1;
                }
                self.advanceNextNonspace();
                return 0;
            }
            let want: i64 = node.borrow().markerOffset + node.borrow().padding;
            if self.indent >= want {
                self.advanceOffset(want, true);
                return 0;
            }
            return 1;
        }
        if kind == "list" {
            return 0;
        }
        if kind == "document" {
            return 0;
        }
        // A definition list goes on while its definitions do: a line that is
        // neither in one nor starts one ends it, because a list cannot hold a
        // paragraph (`canContain`) — the way a bullet list ends.
        if kind == "dl" {
            return 0;
        }
        // A definition: its content is indented two past its marker
        // (markdown-it-deflist's `ddIndent`), and blank lines inside it are
        // its own until a line less indented ends it.
        if kind == "dd" {
            if self.blank {
                self.advanceNextNonspace();
                return 0;
            }
            let want_dd: i64 = node.borrow().markerOffset + node.borrow().padding;
            if self.indent >= want_dd {
                self.advanceOffset(want_dd, true);
                return 0;
            }
            return 1;
        }
        // A footnote's continuation is indented four (markdown-it-footnote
        // adds four to the block indent), and a lazy line continues its
        // paragraph as anywhere else.
        if kind == "footnote_def" {
            if self.blank {
                self.advanceNextNonspace();
                return 0;
            }
            if self.indent >= 4 {
                self.advanceOffset(4, true);
                return 0;
            }
            return 1;
        }
        // `:::` closes the container it is at least as long as — before the
        // blocks inside it are asked, so a fence or a paragraph inside cannot
        // carry on past it, which is what markdown-it-container's search for
        // its end marker means. The closing line is part of the container.
        if kind == "container" {
            if false == self.indented {
                let mut _tmp_372 = self.line.clone();
                let close_colons: i64 = MdBlock::closingFenceLength(&_tmp_372, self.nextNonspace.clone(), 58);
                if close_colons >= node.borrow().fenceLength {
                    self.finalizeThrough(idx);
                    node.borrow_mut()
                        .srcEnd = self.lineStart + (MdText::str_len(&self.line) as i64);
                    return 2;
                }
            }
            return 0;
        }
        if kind == "heading" {
            return 1;
        }
        if kind == "attrs" {
            return 1;
        }
        if kind == "thematic_break" {
            return 1;
        }
        if kind == "table" {
            return 1;
        }
        if kind == "front_matter" {
            return 1;
        }
        if kind == "code_block" {
            if node.borrow().fenced {
                if false == self.indented {
                    let mut _tmp_374 = self.line.clone();
                    let mut _tmp_375 = self.nextNonspace.clone();
                    let close_len: i64 = MdBlock::closingFenceLength(&_tmp_374, _tmp_375, node.borrow().fenceChar.clone());
                    if close_len >= node.borrow().fenceLength {
                        self.finalizeThrough(idx);
                        return 2;
                    }
                }
                let eat: i64 = if (node.borrow().fenceOffset < self.indent) { node.borrow().fenceOffset } else { self.indent };
                self.advanceOffset(eat, true);
                return 0;
            }
            if self.indent >= 4 {
                self.advanceOffset(4, true);
                return 0;
            }
            if self.blank {
                self.advanceNextNonspace();
                return 0;
            }
            return 1;
        }
        if kind == "html_block" {
            if self.blank && node.borrow().htmlBlockType >= 6 {
                return 1;
            }
            return 0;
        }
        if kind == "paragraph" {
            if self.blank {
                return 1;
            }
            return 0;
        }
        0
    }
    /// ----------------------------------------------------------------------
    /// New blocks.  0 nothing, 1 a container opened, 2 a leaf opened
    /// ----------------------------------------------------------------------
    pub fn tryStarts(&mut self) -> i64 {
        let mut r: i64 = self.startAttributes();
        if r != 0 {
            return r;
        }
        r = self.startBlockQuote();
        if r != 0 {
            return r;
        }
        r = self.startAtxHeading();
        if r != 0 {
            return r;
        }
        r = self.startFootnoteDef();
        if r != 0 {
            return r;
        }
        r = self.startCustomContainer();
        if r != 0 {
            return r;
        }
        r = self.startFencedCode();
        if r != 0 {
            return r;
        }
        r = self.startHtmlBlock();
        if r != 0 {
            return r;
        }
        r = self.startSetextHeading();
        if r != 0 {
            return r;
        }
        r = self.startThematicBreak();
        if r != 0 {
            return r;
        }
        r = self.startDefinition();
        if r != 0 {
            return r;
        }
        r = self.startListItem();
        if r != 0 {
            return r;
        }
        r = self.startIndentedCode();
        if r != 0 {
            return r;
        }
        0
    }
    /// A line that is nothing but `{.c .c3}`.
    ///
    /// It is tried FIRST and it interrupts a paragraph, which is what makes
    ///
    ///     - Item 3
    ///     {.c .c3}
    ///
    /// attach to the LIST rather than become the last word of the last item.
    /// Goldmark's attribute parser can interrupt a paragraph for exactly this
    /// reason, and a block start is tried before a lazy continuation here, so
    /// the same line behaves the same way.
    ///
    /// The node it makes is folded into the block above it — see
    /// `foldAttributes` — and only ever survives as a paragraph, which is what
    /// an attribute block with nothing above it is.
    pub fn startAttributes(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let c: i64 = self.charAtOffset(self.nextNonspace.clone());
        if c != 123 {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let rest: String = MdText::substr(&self.line, self.nextNonspace, n);
        if false == MdAttrs::isBlock(&rest) {
            return 0;
        }
        self.closeUnmatchedBlocks();
        let mut a: Rc<RefCell<MdNode>> = self.addChild("attrs", self.nextNonspace.clone());
        a.borrow_mut().literal = rest.clone();
        a.borrow_mut().clearMap();
        a.borrow_mut().addMapPiece(0, self.lineStart + self.nextNonspace, (MdText::str_len(&rest) as i64));
        a.borrow_mut().rawLen = MdText::str_len(&rest) as i64;
        MdAttrs::applyTo(&a, &rest);
        self.advanceOffset(n - self.offset, false);
        2
    }
    /// ----------------------------------------------------------------------
    /// markdown-it's standard plugins
    /// ----------------------------------------------------------------------
    ///
    /// Three block syntaxes that are not CommonMark and that every markdown-it
    /// demo document uses. Each opens a container here exactly as a block
    /// quote or a list item does — the same stack, the same continuation
    /// question per line — so what may be inside them, lazy continuation lines
    /// and blank lines all come from the algorithm above rather than from a
    /// second parser. The rules are the plugins' own, cited where they bite.
    /// `[^label]: text` (markdown-it-footnote). The label runs to the first
    /// `]` and may not be empty or hold a space; the `:` must follow at once.
    /// It is tried before a paragraph and a link reference definition, so it
    /// interrupts a paragraph and `[^x]: y` is never a link to `y`.
    pub fn startFootnoteDef(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let s: i64 = self.nextNonspace;
        if self.charAtOffset(s) != 91 {
            return 0;
        }
        if self.charAtOffset(s + 1) != 94 {
            return 0;
        }
        let mut i: i64 = s + 2;
        while i < n {
            let c: i64 = MdText::char_at(&self.line, i);
            if c == 32 {
                return 0;
            }
            if c == 93 {
                break;
            }
            i += 1;
        }
        if i == s + 2 {
            return 0;
        }
        if i >= n {
            return 0;
        }
        if self.charAtOffset(i + 1) != 58 {
            return 0;
        }
        let label: String = MdText::substr(&self.line, s + 2, i);
        self.closeUnmatchedBlocks();
        let mut fd: Rc<RefCell<MdNode>> = self.addChild("footnote_def", s);
        fd.borrow_mut().info = label.clone();
        self.notes.clone().borrow_mut().define(&label, fd.clone());
        self.advanceNextNonspace();
        self.advanceOffset((i + 2) - self.offset, false);
        // the spaces after the colon are not content
        self.findNextNonspace();
        self.advanceNextNonspace();
        1
    }
    /// `::: warning` (markdown-it-container): three or more colons and a
    /// name, then anything — a title. Any name is a container; it becomes the
    /// block's class, so a sheet styles `.warning`. It closes at a line of at
    /// least as many colons and nothing else, so a container is nested in
    /// another by giving the outer one more (`::::`), as markdown-it needs.
    pub fn startCustomContainer(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let s: i64 = self.nextNonspace;
        if self.charAtOffset(s) != 58 {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let mut i: i64 = s;
        while i < n && MdText::char_at(&self.line, i) == 58 {
            i += 1;
        }
        let count: i64 = i - s;
        if count < 3 {
            return 0;
        }
        let params: String = MdText::substr(&self.line, i, n).trim().to_string();
        let name: String = MdBlock::firstWord(&params);
        if false == MdBlock::isContainerName(&name) {
            return 0;
        }
        self.closeUnmatchedBlocks();
        let mut c: Rc<RefCell<MdNode>> = self.addChild("container", s);
        c.borrow_mut().fenceChar = 58;
        c.borrow_mut().fenceLength = count;
        c.borrow_mut().info = name.clone();
        c.borrow_mut()
            .title = MdText::substr(&params, (MdText::str_len(&name) as i64), (MdText::str_len(&params) as i64))
            .trim()
            .to_string();
        c.borrow_mut().addClass(&name);
        self.advanceOffset(n - self.offset, false);
        2
    }
    /// `: definition` or `~ definition` (markdown-it-deflist), under a term.
    ///
    /// The term is a paragraph of exactly one line, right above the marker or
    /// one blank line above it; a paragraph of two lines is prose with a colon
    /// in it. Once a list is open, a marker line that is not inside the
    /// definition before it is the next definition of the same term, and a
    /// new term right after the list carries the same list on.
    ///
    /// Every test here is made BEFORE anything is closed: a marker line that
    /// turns out not to be a definition may still be a lazy continuation of a
    /// paragraph in a quote above it.
    pub fn startDefinition(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let s: i64 = self.nextNonspace;
        let c: i64 = self.charAtOffset(s);
        if c != 58 && c != 126 {
            return 0;
        }
        if false == MdChar::isSpaceOrTab(self.charAtOffset(s + 1)) {
            return 0;
        }
        // no empty definitions
        let n: i64 = MdText::str_len(&self.line) as i64;
        let mut k: i64 = s + 1;
        while k < n && MdChar::isSpaceOrTab(MdText::char_at(&self.line, k)) {
            k += 1;
        }
        if k >= n {
            return 0;
        }
        let mut cont: Rc<RefCell<MdNode>> = self.stack[(self.container) as usize].clone();
        if cont.borrow().kind == "dl" {
            self.closeUnmatchedBlocks();
            return self.openDefinition();
        }
        if cont.borrow().kind == "paragraph" {
            if (cont.borrow().lines.len() as i64) != 1 {
                return 0;
            }
            let term_at: i64 = cont.borrow().srcStart;
            self.closeUnmatchedBlocks();
            self.finalizeTop();
            self.container = self.tipIndex();
            let mut parent: Rc<RefCell<MdNode>> = self.tip();
            if false == parent.borrow().hasChildren() {
                return 0;
            }
            let mut term: Rc<RefCell<MdNode>> = parent.borrow().lastChild();
            // a line that was only a link reference definition is gone
            if term.borrow().kind != "paragraph" || term.borrow().srcStart != term_at {
                return 0;
            }
            return self.termToList(&parent, term.clone());
        }
        if false == cont.borrow().hasChildren() {
            return 0;
        }
        let mut last: Rc<RefCell<MdNode>> = cont.borrow().lastChild();
        if last.borrow().kind != "paragraph" || last.borrow().open {
            return 0;
        }
        if last.borrow().firstLineNo != last.borrow().lastLineNo {
            return 0;
        }
        if last.borrow().lastLineNo != self.lineNumber - 2 {
            return 0;
        }
        self.closeUnmatchedBlocks();
        self.termToList(&cont, last.clone())
    }
    /// The closed one-line paragraph at the end of `parent` becomes a term:
    /// of the list just before it when there is one, else of a new list.
    pub fn termToList(&mut self, parent: &Rc<RefCell<MdNode>>, term: Rc<RefCell<MdNode>>) -> i64 {
        parent.borrow_mut().children.pop();
        term.borrow_mut().kind = "dt".to_string();
        let mut reuse: bool = false;
        if parent.borrow().hasChildren() {
            let mut prev: Rc<RefCell<MdNode>> = parent.borrow().lastChild();
            if prev.borrow().kind == "dl" && false == prev.borrow().open {
                reuse = true;
            }
        }
        if reuse {
            let mut dl: Rc<RefCell<MdNode>> = parent.borrow().lastChild();
            dl.borrow_mut().open = true;
            self.stack.push(dl.clone());
            dl.borrow_mut().add(term.clone());
        } else {
            let mut dl2: Rc<RefCell<MdNode>> = self.addChild("dl", self.nextNonspace.clone());
            dl2.borrow_mut().srcStart = term.borrow().srcStart;
            dl2.borrow_mut().add(term.clone());
        }
        self.openDefinition()
    }
    /// A definition, at the marker on this line. Its content starts one
    /// column after the marker's space; the rest of the spaces are the
    /// content's own indent, so `:     code` is code, as in markdown-it.
    pub fn openDefinition(&mut self) -> i64 {
        let marker_indent: i64 = self.indent;
        let at: i64 = self.nextNonspace;
        self.advanceNextNonspace();
        self.advanceOffset(1, false);
        self.advanceOffset(1, true);
        let mut dd: Rc<RefCell<MdNode>> = self.addChild("dd", at);
        dd.borrow_mut().markerOffset = marker_indent;
        dd.borrow_mut().padding = 2;
        1
    }
    pub fn startBlockQuote(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let c: i64 = self.charAtOffset(self.nextNonspace.clone());
        if c != 62 {
            return 0;
        }
        self.advanceNextNonspace();
        self.advanceOffset(1, false);
        let after: i64 = self.charAtOffset(self.offset.clone());
        if MdChar::isSpaceOrTab(after) {
            self.advanceOffset(1, true);
        }
        self.closeUnmatchedBlocks();
        self.addChild("block_quote", self.nextNonspace.clone());
        1
    }
    pub fn startAtxHeading(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let mut i: i64 = self.nextNonspace;
        let mut hashes: i64 = 0;
        let mut scanning: bool = true;
        while scanning {
            if i >= n {
                scanning = false;
            }
            if scanning {
                let c: i64 = MdText::char_at(&self.line, i);
                if c == 35 {
                    hashes += 1;
                    i += 1;
                } else {
                    scanning = false;
                }
            }
        }
        if hashes < 1 {
            return 0;
        }
        if hashes > 6 {
            return 0;
        }
        let after: i64 = self.charAtOffset(i);
        if after >= 0 {
            if false == MdChar::isSpaceOrTab(after) {
                return 0;
            }
        }
        self.advanceNextNonspace();
        self.advanceOffset(hashes, false);
        self.closeUnmatchedBlocks();
        let mut h: Rc<RefCell<MdNode>> = self.addChild("heading", self.nextNonspace.clone());
        h.borrow_mut().level = hashes;
        let rest: String = MdText::substr(&self.line, self.offset, n);
        h.borrow_mut().literal = MdBlock::stripAtxClosing(&rest);
        // This heading never went through `addLine`, so its one piece is put
        // here: the whole of `rest`, then shifted past what the trim and the
        // closing hashes took off.
        h.borrow_mut().clearMap();
        h.borrow_mut().addMapPiece(0, self.lineStart + self.offset, (MdText::str_len(&rest) as i64));
        h.borrow_mut().rawLen = MdText::str_len(&rest) as i64;
        h.borrow_mut().shiftMap(MdBlock::leadTrim(&rest), (MdText::str_len(&h.borrow().literal) as i64));
        self.advanceOffset(n - self.offset, false);
        2
    }
    pub fn startFencedCode(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let mut i: i64 = self.nextNonspace;
        let ch: i64 = self.charAtOffset(i);
        if ch != 96 && ch != 126 {
            return 0;
        }
        let mut count: i64 = 0;
        let mut scanning: bool = true;
        while scanning {
            if i >= n {
                scanning = false;
            }
            if scanning {
                let c: i64 = MdText::char_at(&self.line, i);
                if c == ch {
                    count += 1;
                    i += 1;
                } else {
                    scanning = false;
                }
            }
        }
        if count < 3 {
            return 0;
        }
        if ch == 96 {
            // A backtick fence may not have a backtick in its info string, or
            // `` `a` `` in a paragraph would open a code block.
            let mut j: i64 = i;
            while j < n {
                let c2: i64 = MdText::char_at(&self.line, j);
                if c2 == 96 {
                    return 0;
                }
                j += 1;
            }
        }
        self.closeUnmatchedBlocks();
        let mut cb: Rc<RefCell<MdNode>> = self.addChild("code_block", self.nextNonspace.clone());
        cb.borrow_mut().fenced = true;
        cb.borrow_mut().fenceLength = count;
        cb.borrow_mut().fenceChar = ch;
        cb.borrow_mut().fenceOffset = self.indent;
        self.advanceNextNonspace();
        self.advanceOffset(count, false);
        2
    }
    pub fn startHtmlBlock(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let c: i64 = self.charAtOffset(self.nextNonspace.clone());
        if c != 60 {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let rest: String = MdText::substr(&self.line, self.nextNonspace, n);
        let mut cont: Rc<RefCell<MdNode>> = self.stack[(self.container) as usize].clone();
        let mut t: i64 = 1;
        while t <= 7 {
            if MdBlock::htmlBlockOpens(t, &rest) {
                let mut ok: bool = true;
                if t == 7 {
                    // Type 7 may not interrupt a paragraph — otherwise every
                    // line beginning with a tag would end the one above it.
                    if cont.borrow().kind == "paragraph" {
                        ok = false;
                    }
                    let mut tip_now: Rc<RefCell<MdNode>> = self.tip();
                    if (false == self.allClosed && false == self.blank) && tip_now.borrow().kind == "paragraph" {
                        ok = false;
                    }
                }
                if ok {
                    self.closeUnmatchedBlocks();
                    let mut hb: Rc<RefCell<MdNode>> = self.addChild("html_block", self.offset.clone());
                    hb.borrow_mut().htmlBlockType = t;
                    return 2;
                }
            }
            t += 1;
        }
        0
    }
    pub fn startSetextHeading(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let mut cont: Rc<RefCell<MdNode>> = self.stack[(self.container) as usize].clone();
        if cont.borrow().kind != "paragraph" {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let rest: String = MdText::substr(&self.line, self.nextNonspace, n);
        let level: i64 = MdBlock::setextLevel(&rest);
        if level == 0 {
            return 0;
        }
        self.closeUnmatchedBlocks();
        let mut para: Rc<RefCell<MdNode>> = self.tip();
        if para.borrow().kind != "paragraph" {
            return 0;
        }
        self.stripReferences(&para);
        let joined: String = MdBlock::joinLines(&para.borrow().lines.clone());
        let body: String = joined.trim().to_string();
        let blen: i64 = MdText::str_len(&body) as i64;
        if blen == 0 {
            return 0;
        }
        para.borrow_mut().kind = "heading".to_string();
        para.borrow_mut().level = level;
        para.borrow_mut().literal = body.clone();
        // The reference definitions came off the FRONT, so what is left is a
        // suffix of the raw text the map was built against, and the difference
        // in length is how far it moved.
        para.borrow_mut().shiftMap((para.borrow().rawLen - (MdText::str_len(&joined) as i64)) + MdBlock::leadTrim(&joined), blen);
        para.borrow_mut().lines.clear();
        self.advanceOffset(n - self.offset, false);
        2
    }
    pub fn startThematicBreak(&mut self) -> i64 {
        if self.indented {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let rest: String = MdText::substr(&self.line, self.nextNonspace, n);
        if false == MdBlock::isThematicBreak(&rest) {
            return 0;
        }
        self.closeUnmatchedBlocks();
        self.addChild("thematic_break", self.nextNonspace.clone());
        self.advanceOffset(n - self.offset, false);
        2
    }
    pub fn startListItem(&mut self) -> i64 {
        let mut cont: Rc<RefCell<MdNode>> = self.stack[(self.container) as usize].clone();
        if self.indented && cont.borrow().kind != "list" {
            return 0;
        }
        let marker: i64 = self.parseListMarker(&cont);
        if marker == 0 {
            return 0;
        }
        self.closeUnmatchedBlocks();
        let mut t: Rc<RefCell<MdNode>> = self.tip();
        let mut need_list: bool = true;
        if t.borrow().kind == "list" {
            let mut _tmp_397 = self.pendingOrdered.clone();
            let mut _tmp_398 = self.pendingDelim.clone();
            if MdBlock::listsMatch(&t, _tmp_397, &_tmp_398, self.pendingBullet.clone()) {
                need_list = false;
            }
        }
        if need_list {
            let mut lst: Rc<RefCell<MdNode>> = self.addChild("list", self.nextNonspace.clone());
            lst.borrow_mut().listOrdered = self.pendingOrdered;
            lst.borrow_mut().listStart = self.pendingStart;
            lst.borrow_mut().listDelim = self.pendingDelim.clone();
            lst.borrow_mut().listBulletChar = self.pendingBullet;
            lst.borrow_mut().listTight = true;
        }
        let mut item: Rc<RefCell<MdNode>> = self.addChild("item", self.nextNonspace.clone());
        item.borrow_mut().markerOffset = self.pendingMarkerOffset;
        item.borrow_mut().padding = self.pendingPadding;
        item.borrow_mut().listOrdered = self.pendingOrdered;
        1
    }
    /// The marker's shape is returned through fields rather than a record,
    /// because the caller needs six values and Ranger has one return.
    pub fn parseListMarker(&mut self, cont: &Rc<RefCell<MdNode>>) -> i64 {
        if self.indent >= 4 {
            return 0;
        }
        let n: i64 = MdText::str_len(&self.line) as i64;
        let i: i64 = self.nextNonspace;
        let c: i64 = self.charAtOffset(i);
        let mut marker_len: i64 = 0;
        self.pendingOrdered = false;
        self.pendingStart = 1;
        self.pendingDelim = String::new();
        self.pendingBullet = 0;
        self.pendingMarkerOffset = self.indent;
        if (c == 42 || c == 43) || c == 45 {
            self.pendingBullet = c;
            marker_len = 1;
        } else {
            let mut digits: i64 = 0;
            let mut value: i64 = 0;
            let mut j: i64 = i;
            let mut scanning: bool = true;
            while scanning {
                if j >= n {
                    scanning = false;
                }
                if scanning {
                    let d: i64 = MdText::char_at(&self.line, j);
                    if MdChar::isDigit(d) {
                        value = value * 10 + (d - 48);
                        digits += 1;
                        j += 1;
                    } else {
                        scanning = false;
                    }
                }
            }
            if digits == 0 {
                return 0;
            }
            if digits > 9 {
                return 0;
            }
            let d2: i64 = self.charAtOffset(j);
            if d2 != 46 && d2 != 41 {
                return 0;
            }
            // An ordered list may only interrupt a paragraph when it starts
            // at 1, so that "The year 1988. It was" stays one paragraph.
            if cont.borrow().kind == "paragraph" {
                if value != 1 {
                    return 0;
                }
            }
            self.pendingOrdered = true;
            self.pendingStart = value;
            self.pendingDelim = if (d2 == 46) { ".".to_string() } else { ")".to_string() };
            marker_len = digits + 1;
        }
        let after_marker: i64 = self.charAtOffset(i + marker_len);
        if after_marker >= 0 {
            if false == MdChar::isSpaceOrTab(after_marker) {
                return 0;
            }
        }
        // A list may not interrupt a paragraph with an empty first item.
        if cont.borrow().kind == "paragraph" {
            let mut k: i64 = i + marker_len;
            let mut has_content: bool = false;
            while k < n {
                let c3: i64 = MdText::char_at(&self.line, k);
                if false == MdChar::isSpaceOrTab(c3) {
                    has_content = true;
                    k = n;
                } else {
                    k += 1;
                }
            }
            if false == has_content {
                return 0;
            }
        }
        self.advanceNextNonspace();
        self.advanceOffset(marker_len, true);
        let spaces_start_col: i64 = self.column;
        let spaces_start_offset: i64 = self.offset;
        let mut going: bool = true;
        while going {
            self.advanceOffset(1, true);
            let nc: i64 = self.charAtOffset(self.offset.clone());
            let within: bool = self.column - spaces_start_col < 5;
            if false == within {
                going = false;
            }
            if going {
                if false == MdChar::isSpaceOrTab(nc) {
                    going = false;
                }
            }
        }
        let after_spaces: i64 = self.charAtOffset(self.offset.clone());
        let blank_item: bool = after_spaces < 0;
        let spaces_after: i64 = self.column - spaces_start_col;
        if (spaces_after >= 5 || spaces_after < 1) || blank_item {
            self.pendingPadding = marker_len + 1;
            self.column = spaces_start_col;
            self.offset = spaces_start_offset;
            let sp: i64 = self.charAtOffset(self.offset.clone());
            if MdChar::isSpaceOrTab(sp) {
                self.advanceOffset(1, true);
            }
        } else {
            self.pendingPadding = marker_len + spaces_after;
        }
        1
    }
    pub fn startIndentedCode(&mut self) -> i64 {
        if false == self.indented {
            return 0;
        }
        let mut t: Rc<RefCell<MdNode>> = self.tip();
        if t.borrow().kind == "paragraph" {
            return 0;
        }
        if self.blank {
            return 0;
        }
        self.advanceOffset(4, true);
        self.closeUnmatchedBlocks();
        self.addChild("code_block", self.offset.clone());
        2
    }
    pub fn finalizeTop(&mut self) {
        let n: i64 = self.stack.len() as i64;
        if n == 0 {
            return;
        }
        let mut node: Rc<RefCell<MdNode>> = self.stack[(n - 1) as usize].clone();
        node.borrow_mut().open = false;
        node.borrow_mut().srcEnd = self.lineStart;
        let kind: String = node.borrow().kind.clone();
        if kind == "paragraph" {
            self.stripReferences(&node);
            let joined: String = MdBlock::joinLines(&node.borrow().lines.clone());
            let body: String = joined.trim().to_string();
            node.borrow_mut().literal = body.clone();
            node.borrow_mut().shiftMap((node.borrow().rawLen - (MdText::str_len(&joined) as i64)) + MdBlock::leadTrim(&joined), (MdText::str_len(&body) as i64));
            node.borrow_mut().lines.clear();
            let blen: i64 = MdText::str_len(&body) as i64;
            if blen == 0 {
                // nothing but reference definitions: the paragraph was never
                // content, and leaving it would print an empty <p>
                self.stack.pop();
                if n - 2 >= 0 {
                    let mut parent: Rc<RefCell<MdNode>> = self.stack[(n - 2) as usize].clone();
                    parent.borrow_mut().children.pop();
                }
                return;
            }
            self.maybeTable(&node);
        }
        if kind == "heading" {
            let hl: i64 = node.borrow().lines.len() as i64;
            if hl > 0 {
                let joined2: String = MdBlock::joinLines(&node.borrow().lines.clone());
                node.borrow_mut().literal = joined2.trim().to_string();
                node.borrow_mut().shiftMap(MdBlock::leadTrim(&joined2), (MdText::str_len(&node.borrow().literal) as i64));
                node.borrow_mut().lines.clear();
            }
        }
        if kind == "html_block" {
            let raw: String = MdBlock::joinLines(&node.borrow().lines.clone());
            node.borrow_mut().literal = MdBlock::stripTrailingNewlines(&raw);
            node.borrow_mut().shiftMap(0, (MdText::str_len(&node.borrow().literal) as i64));
            node.borrow_mut().lines.clear();
        }
        if kind == "code_block" {
            if node.borrow().fenced {
                let mut all: Vec<String> = node.borrow().lines.clone();
                let cnt: i64 = all.len() as i64;
                let mut info_line: String = String::new();
                if cnt > 0 {
                    info_line = all[0].clone();
                }
                node.borrow_mut().info = MdInline::unescape(&info_line.trim().to_string());
                let mut body_1: String = String::new();
                let mut i: i64 = 1;
                while i < cnt {
                    body_1 = format!("{}{}\n", body_1, all[i as usize].clone());
                    i += 1;
                }
                node.borrow_mut().literal = body_1.clone();
                // The info line and its newline are not in the body, so the
                // body starts that far into the raw text.
                let mut info_drop: i64 = 0;
                if cnt > 0 {
                    info_drop = (MdText::str_len(&all[0].clone()) as i64) + 1;
                }
                node.borrow_mut().shiftMap(info_drop, (MdText::str_len(&body_1) as i64));
                // A fence whose info string names something drawable is not
                // code: it is a slot the layout has to leave a hole for. The
                // block structure is identical either way, so this is a
                // relabelling at the end rather than a second block start.
                if MdBlock::isEmbedInfo(&node.borrow().info.clone()) {
                    node.borrow_mut().kind = "embed".to_string();
                }
                // ```math is GitHub's other spelling of a $$ display
                if MdEmbedKinds::firstWord(&node.borrow().info.clone()) == "math" {
                    node.borrow_mut().kind = "math_block".to_string();
                    let literal_v = node.borrow().literal.trim().to_string();
                    node.borrow_mut().literal = literal_v;
                }
                // ```js {.numbers lines=3-5}: the attributes of a code block
                // may close its info string, as Pandoc and Quarto write them;
                // a diagram's the same (```mermaid {style=cartoon tour=on})
                if node.borrow().kind == "code_block" || node.borrow().kind == "embed" {
                    let mut ia: i64 = MdAttrs::trailingAt(&node.borrow().info.clone());
                    if ia < 0 && MdAttrs::isBlock(&node.borrow().info.clone()) {
                        ia = 0;
                    }
                    if ia >= 0 {
                        if MdAttrs::applyTo(&node, &MdText::substr(&node.borrow().info, ia, (MdText::str_len(&node.borrow().info) as i64))) {
                            let info_v = MdText::substr(&node.borrow().info, 0, ia).trim().to_string();
                            node.borrow_mut().info = info_v;
                        }
                    }
                }
            } else {
                let raw2: String = MdBlock::joinLines(&node.borrow().lines.clone());
                node.borrow_mut().literal = MdBlock::stripTrailingBlankLines(&raw2);
                node.borrow_mut().shiftMap(0, (MdText::str_len(&node.borrow().literal) as i64));
            }
            node.borrow_mut().lines.clear();
        }
        if kind == "list" {
            node.borrow_mut().listTight = MdBlock::listIsTight(&node);
        }
        if kind == "dl" {
            node.borrow_mut().listTight = MdBlock::dlIsTight(&node);
        }
        // `srcEnd` was stamped from the start of the line this block is being
        // closed ON, which for a block that opens and closes on one line is the
        // line it STARTED on — so a one-line HTML block came out as [32,32] and
        // six of the specification's own examples had a block whose span was
        // empty. The map knows where the block really ended; it is built from
        // the lines that went into it.
        let map_end: i64 = node.borrow().srcEndOfMap();
        if map_end > node.borrow().srcEnd {
            node.borrow_mut().srcEnd = map_end;
        }
        self.stack.pop();
    }
    /// Link reference definitions at the head of a paragraph, moved into the
    /// map and off the front of the text.
    pub fn stripReferences(&mut self, para: &Rc<RefCell<MdNode>>) {
        let joined: String = MdBlock::joinLines(&para.borrow().lines.clone());
        let mut rest: String = joined.clone();
        let mut going: bool = true;
        while going {
            let n: i64 = MdText::str_len(&rest) as i64;
            if n == 0 {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&rest, 0);
                if c != 91 {
                    going = false;
                    // `*[HTML]: …`, markdown-it-abbr's definition, comes off
                    // the front the same way a link reference does
                    if c == 42 && self.inline.borrow().opts.borrow().extensions {
                        let took: i64 = self.inline.borrow().parseAbbreviation(&rest);
                        if took > 0 {
                            rest = MdText::substr(&rest, took, n);
                            going = true;
                        }
                    }
                }
            }
            if (going && !rest.is_empty()) && MdText::char_at(&rest, 0) == 91 {
                let taken: i64 = self.inline.borrow_mut().parseReference(rest.clone(), &mut self.refs);
                if taken == 0 {
                    going = false;
                } else {
                    let n2: i64 = MdText::str_len(&rest) as i64;
                    rest = MdText::substr(&rest, taken, n2);
                }
            }
        }
        if rest != joined {
            para.borrow_mut().lines.clear();
            let mut parts: Vec<String> = rest.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
            let mut i: i64 = 0;
            let cnt: i64 = parts.len() as i64;
            let last: i64 = cnt - 1;
            while i < cnt {
                let p: String = parts[i as usize].clone();
                let plen: i64 = MdText::str_len(&p) as i64;
                let mut skip: bool = false;
                if i == last && plen == 0 {
                    skip = true;
                }
                if false == skip {
                    para.borrow_mut().lines.push(p.clone());
                }
                i += 1;
            }
        }
    }
    /// ----------------------------------------------------------------------
    /// GFM tables
    /// ----------------------------------------------------------------------
    ///
    /// A table is recognised where the reference implementation recognises it:
    /// when a paragraph is closed and its SECOND line is a delimiter row with
    /// the same number of cells as its first. Until that second line arrives
    /// the two are indistinguishable from prose, which is why this is a
    /// rewrite at finalize rather than a block start.
    pub fn maybeTable(&self, para: &Rc<RefCell<MdNode>>) {
        let mut lines: Vec<String> = para.borrow()
            .literal.split("\n")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        let n: i64 = lines.len() as i64;
        if n < 2 {
            return;
        }
        let header: String = lines[0].clone();
        let delim: String = lines[1].clone();
        if false == MdBlock::hasSub(&delim, "|") {
            // A table has pipes in it. Without this, `Foo` over `---` is a
            // one-column table rather than what everyone writes it as.
            return;
        }
        let mut aligns: Vec<String> = MdBlock::parseDelimiterRow(&delim);
        let cols: i64 = aligns.len() as i64;
        if cols == 0 {
            return;
        }
        let mut head_cells: Vec<String> = MdBlock::splitRow(&header);
        let head_count: i64 = head_cells.len() as i64;
        if head_count != cols {
            return;
        }
        // Where each line begins inside the paragraph's own literal, so that
        // a cell can be turned back into a place in the FILE. Computed before
        // `para.literal` is cleared below; the map `srcOfLiteral` reads is in
        // its own arrays and survives that.
        let mut line_at: Vec<i64> = Vec::new();
        let mut acc: i64 = 0;
        let mut li: i64 = 0;
        while li < n {
            line_at.push(acc);
            acc = (acc + (MdText::str_len(&lines[li as usize].clone()) as i64)) + 1;
            li += 1;
        }
        para.borrow_mut().kind = "table".to_string();
        para.borrow_mut().literal = String::new();
        let mut head: Rc<RefCell<MdNode>> = MdNode::create("table_row".to_string());
        head.borrow_mut().isHeader = true;
        let mut head_spans: Vec<i64> = MdBlock::rowSpans(&header);
        let mut i: i64 = 0;
        while i < cols {
            let mut cell: Rc<RefCell<MdNode>> = MdNode::create("table_cell".to_string());
            cell.borrow_mut().isHeader = true;
            cell.borrow_mut().align = aligns[i as usize].clone();
            cell.borrow_mut().literal = head_cells[i as usize].clone().trim().to_string();
            MdBlock::stampCell(&cell, para, &head_spans, i, line_at[0]);
            head.borrow_mut().add(cell.clone());
            i += 1;
        }
        para.borrow_mut().add(head.clone());
        let mut r: i64 = 2;
        while r < n {
            let row_text: String = lines[r as usize].clone();
            let mut cells: Vec<String> = MdBlock::splitRow(&row_text);
            let mut spans: Vec<i64> = MdBlock::rowSpans(&row_text);
            let mut row: Rc<RefCell<MdNode>> = MdNode::create("table_row".to_string());
            let mut c: i64 = 0;
            while c < cols {
                let mut cell2: Rc<RefCell<MdNode>> = MdNode::create("table_cell".to_string());
                cell2.borrow_mut().align = aligns[c as usize].clone();
                let cn: i64 = cells.len() as i64;
                if c < cn {
                    cell2.borrow_mut().literal = cells[c as usize].clone().trim().to_string();
                } else {
                    cell2.borrow_mut().literal = String::new();
                }
                MdBlock::stampCell(&cell2, para, &spans, c, line_at[r as usize]);
                row.borrow_mut().add(cell2.clone());
                c += 1;
            }
            para.borrow_mut().add(row.clone());
            r += 1;
        }
    }
    /// ----------------------------------------------------------------------
    /// Inlines, once the whole tree is known
    /// ----------------------------------------------------------------------
    pub fn processInlines(&self, node: Rc<RefCell<MdNode>>, parent: &Rc<RefCell<MdNode>>, index: i64) {
        if node.borrow().isLeaf() {
            let mut raw: String = node.borrow().literal.clone();
            // Before the inline pass, not after: `[x]` becomes a bracket the
            // link parser has already taken apart into three text nodes, and
            // the checkbox is no longer a thing you can see in the tree.
            let mut shift: i64 = 0;
            if node.borrow().kind == "paragraph" && index == 0 {
                let before: i64 = MdText::str_len(&raw) as i64;
                raw = MdBlock::markTaskItem(&raw, parent);
                // The marker came off the FRONT, so everything that is left is
                // that much further into the literal — and into the file.
                shift = before - (MdText::str_len(&raw) as i64);
            }
            node.borrow_mut().literal = String::new();
            let mut _tmp_432 = self.refs.clone();
            self.inline.clone().borrow_mut().parseFrom(node.clone(), &raw, &_tmp_432, shift);
            return;
        }
        let mut i: i64 = 0;
        let n: i64 = node.borrow().childCount();
        while i < n {
            let mut c: Rc<RefCell<MdNode>> = node.borrow().childAt(i);
            self.processInlines(c.clone(), &node, i);
            i += 1;
        }
    }
}
