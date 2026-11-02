// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;

use crate::rgr::{MdText};

/// ============================================================================
/// MdChar.rgr — the character tests the specification is written in terms of.
/// ============================================================================
///
/// CommonMark says "Unicode whitespace" and "Unicode punctuation" and means it:
/// the flanking rules that decide whether a `*` opens or closes emphasis read
/// the characters on both sides, and getting them wrong costs examples in the
/// section that has the most of them.
///
/// `charAt` gives a UTF-16 code unit on the targets that have one and a byte on
/// the ones that do not, so everything here works on code points the parser has
/// already decoded, and the multi-byte tests are the ranges rather than a table.
/// ============================================================================
#[derive(Clone)]
pub struct MdChar {
}

impl MdChar {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn tab() -> i64 {
        9
    }
    pub fn newline() -> i64 {
        10
    }
    pub fn space() -> i64 {
        32
    }
    pub fn bang() -> i64 {
        33
    }
    pub fn quote() -> i64 {
        34
    }
    pub fn hash() -> i64 {
        35
    }
    pub fn amp() -> i64 {
        38
    }
    pub fn apos() -> i64 {
        39
    }
    pub fn parenL() -> i64 {
        40
    }
    pub fn parenR() -> i64 {
        41
    }
    pub fn star() -> i64 {
        42
    }
    pub fn plus() -> i64 {
        43
    }
    pub fn dash() -> i64 {
        45
    }
    pub fn dot() -> i64 {
        46
    }
    pub fn slash() -> i64 {
        47
    }
    pub fn colon() -> i64 {
        58
    }
    pub fn semi() -> i64 {
        59
    }
    pub fn lt() -> i64 {
        60
    }
    pub fn eq() -> i64 {
        61
    }
    pub fn gt() -> i64 {
        62
    }
    pub fn bracketL() -> i64 {
        91
    }
    pub fn backslash() -> i64 {
        92
    }
    pub fn bracketR() -> i64 {
        93
    }
    pub fn underscore() -> i64 {
        95
    }
    pub fn backtick() -> i64 {
        96
    }
    pub fn pipe() -> i64 {
        124
    }
    pub fn tilde() -> i64 {
        126
    }
    pub fn isDigit(c: i64) -> bool {
        if c < 48 {
            return false;
        }
        if c > 57 {
            return false;
        }
        true
    }
    pub fn isSpaceOrTab(c: i64) -> bool {
        if c == 32 {
            return true;
        }
        if c == 9 {
            return true;
        }
        false
    }
    /// Unicode whitespace, as the specification defines it: the Zs category
    /// plus tab, line feed, form feed and carriage return.
    pub fn isUnicodeWhitespace(c: i64) -> bool {
        if c == 32 {
            return true;
        }
        if c == 9 {
            return true;
        }
        if c == 10 {
            return true;
        }
        if c == 12 {
            return true;
        }
        if c == 13 {
            return true;
        }
        if c == 160 {
            return true;
        }
        if c == 5760 {
            return true;
        }
        if (c >= 8192 && c <= 8202) {
            return true;
        }
        if c == 8239 {
            return true;
        }
        if c == 8287 {
            return true;
        }
        if c == 12288 {
            return true;
        }
        false
    }
    /// ASCII punctuation, which is the set backslash escapes accept.
    pub fn isAsciiPunct(c: i64) -> bool {
        if (c >= 33 && c <= 47) {
            return true;
        }
        if (c >= 58 && c <= 64) {
            return true;
        }
        if (c >= 91 && c <= 96) {
            return true;
        }
        if (c >= 123 && c <= 126) {
            return true;
        }
        false
    }
    /// Unicode punctuation: the ASCII set plus the ranges the P* and S*
    /// categories occupy densely enough to matter for flanking. A parser that
    /// only tests ASCII here reads `"*foo*"` correctly and `»foo«` wrongly.
    pub fn isUnicodePunct(c: i64) -> bool {
        if MdChar::isAsciiPunct(c) {
            return true;
        }
        if (c >= 161 && c <= 191) {
            return true;
        }
        if (c >= 8208 && c <= 8286) {
            return true;
        }
        if (c >= 8317 && c <= 8318) {
            return true;
        }
        if (c >= 8333 && c <= 8334) {
            return true;
        }
        if (c >= 8968 && c <= 8971) {
            return true;
        }
        if (c >= 9001 && c <= 9002) {
            return true;
        }
        if (c >= 10088 && c <= 10101) {
            return true;
        }
        if (c >= 11776 && c <= 11858) {
            return true;
        }
        if (c >= 12296 && c <= 12305) {
            return true;
        }
        if (c >= 12308 && c <= 12319) {
            return true;
        }
        if (c >= 65040 && c <= 65049) {
            return true;
        }
        if (c >= 65072 && c <= 65106) {
            return true;
        }
        if (c >= 65108 && c <= 65121) {
            return true;
        }
        // Symbols. CommonMark 0.31 defines "Unicode punctuation character" as
        // the P categories AND the S ones, so `*€*` is not emphasis for the
        // same reason `*(*` is not. Leaving the symbol blocks out reads as a
        // currency bug and is a flanking bug.
        if c == 215 {
            return true;
        }
        if c == 247 {
            return true;
        }
        if (c >= 8352 && c <= 8399) {
            return true;
        }
        if (c >= 8448 && c <= 8527) {
            return true;
        }
        if (c >= 8592 && c <= 9254) {
            return true;
        }
        if (c >= 9280 && c <= 9290) {
            return true;
        }
        if (c >= 9312 && c <= 10175) {
            return true;
        }
        if (c >= 10176 && c <= 11123) {
            return true;
        }
        if (c >= 11126 && c <= 11157) {
            return true;
        }
        if (c >= 11159 && c <= 11263) {
            return true;
        }
        if (c >= 12032 && c <= 12245) {
            return true;
        }
        if (c >= 12272 && c <= 12292) {
            return true;
        }
        if (c >= 12688 && c <= 12703) {
            return true;
        }
        if (c >= 12800 && c <= 13311) {
            return true;
        }
        if (c >= 63744 && c <= 64255) {
            return false;
        }
        if (c >= 65281 && c <= 65376) {
            return true;
        }
        if (c >= 65504 && c <= 65510) {
            return true;
        }
        if (c >= 65512 && c <= 65518) {
            return true;
        }
        if (c >= 65529 && c <= 65533) {
            return true;
        }
        if (c >= 127744 && c <= 129782) {
            return true;
        }
        false
    }
    /// Simple case folding, for the alphabets that have one. Link reference
    /// labels are matched case-insensitively, and the specification means
    /// Unicode case folding by that — `[ΑΓΩ]` and `[αγω]` are the same label.
    /// ASCII, Latin-1, Greek and Cyrillic are one addition each and cover
    /// every label anybody writes; the rest fold to themselves, which is
    /// stated here rather than left to be discovered.
    pub fn foldCase(c: i64) -> i64 {
        if (c >= 65 && c <= 90) {
            return c + 32;
        }
        if (c >= 192 && c <= 222) {
            if c == 215 {
                return c;
            }
            return c + 32;
        }
        if (c >= 913 && c <= 929) {
            return c + 32;
        }
        if (c >= 931 && c <= 939) {
            return c + 32;
        }
        if (c >= 1040 && c <= 1071) {
            return c + 32;
        }
        if (c >= 1024 && c <= 1039) {
            return c + 80;
        }
        c
    }
    pub fn isLetter(c: i64) -> bool {
        if (c >= 65 && c <= 90) {
            return true;
        }
        if (c >= 97 && c <= 122) {
            return true;
        }
        false
    }
    pub fn isAlnum(c: i64) -> bool {
        if MdChar::isLetter(c) {
            return true;
        }
        MdChar::isDigit(c)
    }
    pub fn isHexDigit(c: i64) -> bool {
        if MdChar::isDigit(c) {
            return true;
        }
        if (c >= 65 && c <= 70) {
            return true;
        }
        if (c >= 97 && c <= 102) {
            return true;
        }
        false
    }
    /// ASCII A-Z to a-z. The runs between capitals are copied whole and
    /// joined once: adding one character at a time copies the string so far
    /// on the Go target, which made a long line take minutes.
    pub fn lower(s: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut from: i64 = 0;
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&s) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&s, i);
            if (c >= 65 && c <= 90) {
                if i > from {
                    parts.push(MdText::substr(&s, from, i));
                }
                parts.push(MdChar::fromCode(c + 32));
                from = i + 1;
            }
            i += 1;
        }
        if from == 0 {
            return s.to_string();
        }
        if n > from {
            parts.push(MdText::substr(&s, from, n));
        }
        parts.join(&*String::new())
    }
    /// A code point back to a string. `strfromcode` writes UTF-8 directly on
    /// the targets whose strings are bytes and one UTF-16 unit on the ones whose
    /// strings are units, so above the BMP the pair has to be built by hand —
    /// the same split EVGCodepoint.toStr makes, for the same reason.
    pub fn fromCode(c: i64) -> String {
        if c < 65536 {
            return MdText::from_code(c);
        }
        if MdChar::stringIsBytes() {
            return MdText::from_code(c);
        }
        let rel: i64 = c - 65536;
        let hi: i64 = 55296 + (rel / 1024);
        let lo: i64 = 56320 + rel % 1024;
        format!("{}{}", MdText::from_code(hi), MdText::from_code(lo))
    }
    /// Whether this target stores a string as bytes (C++, Go) or as units
    /// (JavaScript, Java, C#). Asked of the language rather than of a build
    /// flag, so it cannot be got wrong by a compiler switch. The letter is
    /// written as itself: the Go target wrote "\u00e4" as "", which made
    /// this false there and fromCode split an emoji into two surrogates.
    pub fn stringIsBytes() -> bool {
        let n: i64 = MdText::str_len(&"ä".to_string()) as i64;
        n > 1
    }
}
