// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::md_char::MdChar;
use crate::rgr::MdText;

/// ============================================================================
/// MdCodeHighlight.rgr — colour for a fenced code block.
/// ============================================================================
///
/// Deliberately unambitious. There is no highlighter in this repository —
/// `text_editor`'s `EditorStyle` carries a weight and a face per run and
/// nothing lexical — so rather than reach for a parser per language this is one
/// small lexer with a table: a line-comment marker, a block-comment pair, the
/// quote characters, and a keyword list.
///
/// Six colours, and they are the things a reader's eye actually uses to find
/// its place in a printed listing:
///
///   keyword   comment   string   number   plain
///
/// That covers fourteen languages to the standard a page wants, in a few
/// hundred lines. Where the repository has a real parser — `gallery/ts_parser`,
/// `gallery/js_parser` — it can be swapped in for those two languages later;
/// that is an upgrade for two, not a prerequisite for fourteen.
///
/// **State crosses lines**, because a block comment does. The caller keeps the
/// number it got back and hands it in with the next line; a highlighter that
/// forgets is one that colours the second line of a comment as code.
/// ============================================================================
#[derive(Clone)]
pub struct MdTokenKind {
}

impl MdTokenKind {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn plain() -> i64 {
        0
    }
    pub fn keyword() -> i64 {
        1
    }
    pub fn string() -> i64 {
        2
    }
    pub fn comment() -> i64 {
        3
    }
    pub fn number() -> i64 {
        4
    }
}

#[derive(Clone)]
pub struct MdToken {
    pub text: String,
    pub kind: i64,
}

impl MdToken {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            kind: 0,
        }
    }
    pub fn create(text: String, kind: i64) -> Rc<RefCell<MdToken>> {
        let mut t: Rc<RefCell<MdToken>> = Rc::new(RefCell::new(MdToken::new()));
        t.borrow_mut().text = text.clone();
        t.borrow_mut().kind = kind;
        t
    }
}

/// One line's tokens, and what the NEXT line inherits: 0 ordinary, 1 inside a
/// block comment.
#[derive(Clone)]
pub struct MdHighlightedLine {
    pub tokens: Vec<Rc<RefCell<MdToken>>>,
    pub state: i64,
}

impl MdHighlightedLine {
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            state: 0,
        }
    }
}

/// How one language is spelled. Not a grammar — the four facts a listing needs
/// coloured correctly.
#[derive(Clone)]
pub struct MdLanguage {
    pub known: bool,
    pub lineComment: String,
    pub lineComment2: String,
    pub blockOpen: String,
    pub blockClose: String,
    /// the quote characters, as a string of them
    pub quotes: String,
    /// space-delimited, space-terminated, so a lookup is one substring test
    pub keywords: String,
}

impl MdLanguage {
    pub fn new() -> Self {
        Self {
            known: false,
            lineComment: String::new(),
            lineComment2: String::new(),
            blockOpen: String::new(),
            blockClose: String::new(),
            quotes: "\"'".to_string(),
            keywords: " ".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct MdCodeHighlight {
}

impl MdCodeHighlight {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The info string's first word, lowercased, into a language. An unknown
    /// name is not an error: it is a listing with no keywords, which still gets
    /// its strings, its numbers and its comments if the defaults happen to fit.
    pub fn languageFor(name: &str) -> Rc<RefCell<MdLanguage>> {
        let n: String = MdChar::lower(name);
        let mut l: Rc<RefCell<MdLanguage>> = Rc::new(RefCell::new(MdLanguage::new()));
        if ((n == "js" || n == "javascript") || n == "jsx") || n == "mjs" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().quotes = "\"'`".to_string();
            l.borrow_mut().keywords = " await async break case catch class const continue debugger default delete do else export extends finally for from function get if import in instanceof let new of return set static super switch this throw try typeof var void while with yield true false null undefined ".to_string();
            return l;
        }
        if (n == "ts" || n == "typescript") || n == "tsx" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().quotes = "\"'`".to_string();
            l.borrow_mut().keywords = " abstract any as asserts await async boolean break case catch class const constructor continue declare default delete do else enum export extends finally for from function get if implements import in infer instanceof interface is keyof let namespace never new number of private protected public readonly return set static string super switch symbol this throw try type typeof undefined union unknown var void while yield true false null ".to_string();
            return l;
        }
        if n == "go" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().quotes = "\"'`".to_string();
            l.borrow_mut().keywords = " break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var nil true false make new len cap append copy delete panic recover string int int8 int16 int32 int64 uint uint8 uint16 uint32 uint64 float32 float64 bool byte rune error ".to_string();
            return l;
        }
        if n == "py" || n == "python" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "#".to_string();
            l.borrow_mut().quotes = "\"'".to_string();
            l.borrow_mut().keywords = " and as assert async await break class continue def del elif else except finally for from global if import in is lambda None nonlocal not or pass raise return True False try while with yield self print len range str int float dict list set tuple ".to_string();
            return l;
        }
        if n == "rs" || n == "rust" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().keywords = " as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while String Vec Option Some None Result Ok Err Box i8 i16 i32 i64 u8 u16 u32 u64 usize isize f32 f64 bool char str ".to_string();
            return l;
        }
        if n == "swift" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().keywords = " associatedtype break case catch class continue default defer deinit do else enum extension fallthrough false fileprivate for func guard if import in init inout internal is let nil open operator private protocol public repeat rethrows return self Self static struct subscript super switch throw throws true try typealias var where while Int Double String Bool Array Dictionary Optional ".to_string();
            return l;
        }
        if n == "kt" || n == "kotlin" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().keywords = " as break by catch class companion const continue crossinline data do else enum external false final finally for fun get if import in infix init inline interface internal is lateinit null object open operator out override package private protected public reified return sealed set super suspend this throw true try typealias val var vararg when where while Int Long Double Float String Boolean List Map Set Array Unit Any ".to_string();
            return l;
        }
        if (n == "cs" || n == "csharp") || n == "c#" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().keywords = " abstract as base bool break byte case catch char checked class const continue decimal default delegate do double else enum event explicit extern false finally fixed float for foreach get goto if implicit in int interface internal is lock long namespace new null object operator out override params private protected public readonly ref return sbyte sealed set short sizeof stackalloc static string struct switch this throw true try typeof uint ulong unchecked unsafe ushort using var virtual void volatile while var async await ".to_string();
            return l;
        }
        if n == "java" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().keywords = " abstract assert boolean break byte case catch char class const continue default do double else enum extends final finally float for goto if implements import instanceof int interface long native new package private protected public return short static strictfp super switch synchronized this throw throws transient try var void volatile while true false null String Integer List Map ".to_string();
            return l;
        }
        if ((((n == "c" || n == "cpp") || n == "c++") || n == "cc") || n == "h") || n == "hpp" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().keywords = " auto bool break case catch char class const constexpr continue default delete do double else enum explicit extern false float for friend goto if inline int long namespace new nullptr operator private protected public return short signed sizeof static struct switch template this throw true try typedef typename union unsigned using virtual void volatile while size_t std string vector ".to_string();
            return l;
        }
        if n == "sql" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "--".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().quotes = "'\"".to_string();
            l.borrow_mut().keywords = " ADD ALL ALTER AND AS ASC BETWEEN BY CASE CHECK COLUMN CONSTRAINT CREATE CROSS DEFAULT DELETE DESC DISTINCT DROP ELSE END EXISTS FOREIGN FROM FULL GROUP HAVING IN INDEX INNER INSERT INTO IS JOIN KEY LEFT LIKE LIMIT NOT NULL ON OR ORDER OUTER PRIMARY REFERENCES RIGHT SELECT SET TABLE THEN UNION UNIQUE UPDATE VALUES VIEW WHEN WHERE WITH add all alter and as asc between by case check column constraint create cross default delete desc distinct drop else end exists foreign from full group having in index inner insert into is join key left like limit not null on or order outer primary references right select set table then union unique update values view when where with int integer text varchar boolean timestamp serial decimal numeric date ".to_string();
            return l;
        }
        if n == "json" {
            l.borrow_mut().known = true;
            l.borrow_mut().quotes = "\"".to_string();
            l.borrow_mut().keywords = " true false null ".to_string();
            return l;
        }
        if (((n == "sh" || n == "bash") || n == "shell") || n == "zsh") || n == "console" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "#".to_string();
            l.borrow_mut().quotes = "\"'".to_string();
            l.borrow_mut().keywords = " case do done elif else esac fi for function if in return select then time until while cd echo export local read set source unset npm node git bash python3 ".to_string();
            return l;
        }
        if n == "rgr" || n == "ranger" {
            l.borrow_mut().known = true;
            // Ranger is LISP-shaped: `;` opens a comment and runs to the end.
            l.borrow_mut().lineComment = ";".to_string();
            l.borrow_mut().quotes = "\"".to_string();
            l.borrow_mut().keywords = " class def fn sfn return if while for break continue new push pop switch case default Import Extends Enum record shape trait operator flag plugin true false null this self void int double string boolean char buffer charbuffer ".to_string();
            return l;
        }
        if (n == "css" || n == "scss") || n == "less" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "//".to_string();
            l.borrow_mut().blockOpen = "/*".to_string();
            l.borrow_mut().blockClose = "*/".to_string();
            l.borrow_mut().keywords = " important media supports keyframes import from to and not only var calc rgb rgba hsl url ".to_string();
            return l;
        }
        if ((n == "yml" || n == "yaml") || n == "toml") || n == "ini" {
            l.borrow_mut().known = true;
            l.borrow_mut().lineComment = "#".to_string();
            l.borrow_mut().quotes = "\"'".to_string();
            l.borrow_mut().keywords = " true false null yes no on off ".to_string();
            return l;
        }
        // Not a language this table knows. Comments and keywords are left
        // alone; a quoted run is still a quoted run in almost everything, so
        // the strings are still coloured.
        l
    }
    /// One line, with the state the previous line ended in.
    pub fn line(text: &str, lang: &Rc<RefCell<MdLanguage>>, state: i64) -> Rc<RefCell<MdHighlightedLine>> {
        let mut out: Rc<RefCell<MdHighlightedLine>> = Rc::new(RefCell::new(MdHighlightedLine::new()));
        out.borrow_mut().state = state;
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = 0;
        let mut plain_from: i64 = 0;
        // Inside a block comment from the line before.
        if state == 1 {
            let close: i64 = MdCodeHighlight::findSub(text, 0, &lang.borrow().blockClose.clone());
            if close < 0 {
                out.borrow_mut().tokens.push(MdToken::create(text.to_string(), MdTokenKind::comment()));
                return out;
            }
            let end_at: i64 = close + (MdText::str_len(&lang.borrow().blockClose) as i64);
            out.borrow_mut().tokens.push(MdToken::create(MdText::substr(&text, 0, end_at), MdTokenKind::comment()));
            out.borrow_mut().state = 0;
            i = end_at;
            plain_from = end_at;
        }
        let mut going: bool = true;
        while going {
            if i >= n {
                going = false;
            }
            if going {
                let c: i64 = MdText::char_at(&text, i);
                // a line comment runs to the end
                let lc: i64 = MdCodeHighlight::matchAt(text, i, &lang.borrow().lineComment.clone());
                let lc2: i64 = MdCodeHighlight::matchAt(text, i, &lang.borrow().lineComment2.clone());
                if lc > 0 || lc2 > 0 {
                    MdCodeHighlight::flush(&out, text, plain_from, i);
                    out.borrow_mut().tokens.push(MdToken::create(MdText::substr(&text, i, n), MdTokenKind::comment()));
                    i = n;
                    plain_from = n;
                    going = false;
                } else {
                    let bo: i64 = MdCodeHighlight::matchAt(text, i, &lang.borrow().blockOpen.clone());
                    if bo > 0 {
                        MdCodeHighlight::flush(&out, text, plain_from, i);
                        let close_1: i64 = MdCodeHighlight::findSub(text, i + bo, &lang.borrow().blockClose.clone());
                        if close_1 < 0 {
                            out.borrow_mut().tokens.push(MdToken::create(MdText::substr(&text, i, n), MdTokenKind::comment()));
                            out.borrow_mut().state = 1;
                            i = n;
                            plain_from = n;
                        } else {
                            let end_at_1: i64 = close_1 + (MdText::str_len(&lang.borrow().blockClose) as i64);
                            out.borrow_mut().tokens.push(MdToken::create(MdText::substr(&text, i, end_at_1), MdTokenKind::comment()));
                            i = end_at_1;
                            plain_from = end_at_1;
                        }
                    } else {
                        if MdCodeHighlight::isQuote(c, &lang.borrow().quotes.clone()) {
                            MdCodeHighlight::flush(&out, text, plain_from, i);
                            let end_at_2: i64 = MdCodeHighlight::endOfString(text, i, c);
                            out.borrow_mut().tokens.push(MdToken::create(MdText::substr(&text, i, end_at_2), MdTokenKind::string()));
                            i = end_at_2;
                            plain_from = end_at_2;
                        } else {
                            if MdChar::isDigit(c) {
                                // a digit inside a name is part of the name
                                let mut prev_ok: bool = true;
                                if i > 0 {
                                    let p: i64 = MdText::char_at(&text, i - 1);
                                    if MdCodeHighlight::isWordChar(p) {
                                        prev_ok = false;
                                    }
                                }
                                if prev_ok {
                                    MdCodeHighlight::flush(&out, text, plain_from, i);
                                    let end_at_3: i64 = MdCodeHighlight::endOfNumber(text, i);
                                    out.borrow_mut().tokens.push(MdToken::create(MdText::substr(&text, i, end_at_3), MdTokenKind::number()));
                                    i = end_at_3;
                                    plain_from = end_at_3;
                                } else {
                                    i += 1;
                                }
                            } else {
                                if MdCodeHighlight::isWordStart(c) {
                                    let end_at_4: i64 = MdCodeHighlight::endOfWord(text, i);
                                    let word: String = MdText::substr(&text, i, end_at_4);
                                    if MdCodeHighlight::isKeyword(&word, &lang.borrow().keywords.clone()) {
                                        MdCodeHighlight::flush(&out, text, plain_from, i);
                                        out.borrow_mut().tokens.push(MdToken::create(word.clone(), MdTokenKind::keyword()));
                                        plain_from = end_at_4;
                                    }
                                    i = end_at_4;
                                } else {
                                    i += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        MdCodeHighlight::flush(&out, text, plain_from, n);
        out
    }
    pub fn flush(out: &Rc<RefCell<MdHighlightedLine>>, text: &str, from: i64, to: i64) {
        if to <= from {
            return;
        }
        out.borrow_mut().tokens.push(MdToken::create(MdText::substr(&text, from, to), MdTokenKind::plain()));
    }
    /// The length of `what` when it starts at `i`, or 0. An EMPTY `what` never
    /// matches, which is how a language with no block comment — or no line
    /// comment — says so without needing a flag beside it.
    pub fn matchAt(text: &str, i: i64, what: &str) -> i64 {
        let m: i64 = MdText::str_len(&what) as i64;
        if m == 0 {
            return 0;
        }
        let n: i64 = MdText::str_len(&text) as i64;
        if i + m > n {
            return 0;
        }
        if MdText::substr(&text, i, i + m) == what {
            return m;
        }
        0
    }
    /// `indexOf` is a name the compiler resolves elsewhere — defining one here
    /// compiles and then fails at every call site — so this is `findSub`.
    pub fn findSub(text: &str, from: i64, what: &str) -> i64 {
        let m: i64 = MdText::str_len(&what) as i64;
        if m == 0 {
            return -1;
        }
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = from;
        while i + m <= n {
            if MdText::substr(&text, i, i + m) == what {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn isQuote(c: i64, quotes: &str) -> bool {
        let n: i64 = MdText::str_len(&quotes) as i64;
        let mut i: i64 = 0;
        while i < n {
            if MdText::char_at(&quotes, i) == c {
                return true;
            }
            i += 1;
        }
        false
    }
    /// Through the closing quote, or to the end of the line. A backslash escapes
    /// the next character, so `"a\"b"` is one string and not two.
    pub fn endOfString(text: &str, from: i64, quote: i64) -> i64 {
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = from + 1;
        while i < n {
            let c: i64 = MdText::char_at(&text, i);
            if c == 92 {
                i += 2;
            } else {
                if c == quote {
                    return i + 1;
                }
                i += 1;
            }
        }
        n
    }
    pub fn endOfNumber(text: &str, from: i64) -> i64 {
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = from;
        while i < n {
            let c: i64 = MdText::char_at(&text, i);
            let mut ok: bool = MdChar::isHexDigit(c);
            if c == 46 {
                ok = true;
            }
            if c == 120 {
                ok = true;
            }
            if c == 88 {
                ok = true;
            }
            if c == 95 {
                ok = true;
            }
            if false == ok {
                return i;
            }
            i += 1;
        }
        n
    }
    pub fn isWordStart(c: i64) -> bool {
        if MdChar::isLetter(c) {
            return true;
        }
        if c == 95 {
            return true;
        }
        if c == 35 {
            return true;
        }
        false
    }
    pub fn isWordChar(c: i64) -> bool {
        if MdChar::isAlnum(c) {
            return true;
        }
        if c == 95 {
            return true;
        }
        false
    }
    pub fn endOfWord(text: &str, from: i64) -> i64 {
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = from + 1;
        while i < n {
            let c: i64 = MdText::char_at(&text, i);
            if false == MdCodeHighlight::isWordChar(c) {
                return i;
            }
            i += 1;
        }
        n
    }
    pub fn isKeyword(word: &str, keywords: &str) -> bool {
        let probe: String = format!(" {} ", word);
        MdCodeHighlight::findSub(keywords, 0, &probe) >= 0
    }
}
