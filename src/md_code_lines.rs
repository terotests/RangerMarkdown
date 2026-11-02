// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::MdText;

/// ============================================================================
/// MdCodeLines.rgr — what each line of a code block is, before it is drawn.
/// ============================================================================
///
///   ```diff js {.numbers lines=12-14|20}
///   @@ -10,6 +10,7 @@
///    const a = 1
///   -const b = 2
///   +const b = 3
///   ```
///
/// A `diff` block (the second word is the language its code is coloured as)
/// marks each line added (+), removed (-), a hunk header (@@) or a file header
/// (diff --git, index, --- a/, +++ b/ before the first hunk). `.numbers` (or
/// `numbers=N`, the first number) puts line numbers in a gutter; in a diff they
/// follow the hunk headers' new-file side, and a removed line has none.
/// `lines=` names lines to highlight by the numbers shown (1 = first line when
/// there are none): `12-14,20` together, `12-14|20` as steps one after the
/// other, which `{.build}` reveals one per build step.
///
/// In a diff, lines= fades only the context: the added and removed lines are
/// the change and keep their colours.
///
/// A run of removed lines followed by a run of added lines is a change: the
/// i-th removed line is paired with the i-th added one, and where the two
/// still read alike (they share at least half of the longer one, counted
/// from both ends) the part between their common start and end, widened to
/// whole words, is what changed in the line: changeFrom..changeTo on each.
///
/// The layout draws what this says; it reads characters only.
/// ============================================================================
#[derive(Clone)]
pub struct MdCodeLine {
    /// the line as shown, a diff's marker kept
    pub text: String,
    /// the diff marker ("+", "-", " ") or ""; code is the rest
    pub marker: String,
    pub code: String,
    /// 0 code, 1 added, 2 removed, 3 hunk header, 4 file header
    pub kind: i64,
    /// the number in the gutter, -1 for none
    pub number: i64,
    /// 0 not highlighted, k the highlight step (1 = first) it belongs to
    pub mark: i64,
    /// on a paired added/removed line: the bytes of code [changeFrom,
    /// changeTo) that differ from its pair; -1 when nothing is marked
    pub changeFrom: i64,
    pub changeTo: i64,
    /// set by the layout: where in the drawn text (code, or text for a
    /// header) each row after the first begins when the line is wider than
    /// the block, and how far those rows are indented
    pub wraps: Vec<i64>,
    pub wrapIndent: f64,
    /// where `text` starts in the block's literal (its source through the
    /// node's map): an editor's caret stops on each character of the line
    pub litAt: i64,
}

impl MdCodeLine {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            marker: String::new(),
            code: String::new(),
            kind: 0,
            number: -1,
            mark: 0,
            changeFrom: -1,
            changeTo: -1,
            wraps: Vec::new(),
            wrapIndent: 0.0,
            litAt: -1,
        }
    }
}

/// ============================================================================
/// MdCodeWrap — where a code line too wide for its block wraps.
/// ============================================================================
///
/// A listing keeps its line breaks, and a line wider than the block goes on
/// under itself rather than past the block's (and the slide's) edge: after a
/// space or a , ; ( [ { = where there is one in the row, between two
/// characters where there is not. The rows after the first are indented as
/// far as the line is, so the code still reads as nested. The layout measures,
/// this decides.
/// ============================================================================
#[derive(Clone)]
pub struct MdCodeWrap {
}

impl MdCodeWrap {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The line as the pieces a row may end between: one character each, a
    /// character's UTF-8 continuation bytes (or UTF-16 low surrogate) kept
    /// with it so a cut never splits one.
    pub fn units(text: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let n: i64 = MdText::str_len(&text) as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut j: i64 = i + 1;
            let mut going: bool = true;
            while going && j < n {
                let c: i64 = MdText::char_at(&text, j);
                if c >= 128 && c < 192 || c >= 56320 && c < 57344 {
                    j += 1;
                } else {
                    going = false;
                }
            }
            out.push(MdText::substr(&text, i, j));
            i = j;
        }
        out
    }
    pub fn isSpace(u: &str) -> bool {
        u == " " || u == char::from_u32(9 as u32).unwrap_or('\0').to_string()
    }
    /// a row may end after this piece
    pub fn breaksAfter(u: &str) -> bool {
        if MdCodeWrap::isSpace(u) {
            return true;
        }
        MdText::index_of(&",;([{=".to_string(), u) >= 0
    }
    /// how far the rows after the first are indented: the line's own leading
    /// whitespace, never more than half the row
    pub fn indentOf(units: &[String], widths: &[f64], limit: f64) -> f64 {
        let mut w: f64 = 0.0;
        let mut i: i64 = 0;
        let mut going: bool = true;
        while going && i < (units.len() as i64) {
            if MdCodeWrap::isSpace(&units[i as usize].clone()) {
                w += widths[i as usize];
                i += 1;
            } else {
                going = false;
            }
        }
        if w > limit * 0.5 {
            return 0.0;
        }
        w
    }
    /// The piece each row begins with (the first row at 0): first is the
    /// first row's width, rest the others'. Spaces at the end of a row hang
    /// past it, as they do in any text; a piece wider than a row has one to
    /// itself.
    pub fn rows(units: &[String], widths: &[f64], first: f64, rest: f64) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        out.push(0);
        let n: i64 = units.len() as i64;
        let mut start: i64 = 0;
        let mut x: f64 = 0.0;
        let mut limit: f64 = first;
        let mut last_break: i64 = -1;
        let mut i: i64 = 0;
        while i < n {
            let u: String = units[i as usize].clone();
            let w: f64 = widths[i as usize];
            if !MdCodeWrap::isSpace(&u) && (x + w > limit + 0.01 && i > start) {
                let mut cut: i64 = i;
                if last_break >= start {
                    cut = last_break + 1;
                }
                out.push(cut);
                start = cut;
                limit = rest;
                last_break = -1;
                x = 0.0;
                let mut k: i64 = cut;
                while k < i {
                    x += widths[k as usize];
                    k += 1;
                }
            } else {
                x += w;
                if MdCodeWrap::breaksAfter(&u) {
                    last_break = i;
                }
                i += 1;
            }
        }
        out
    }
    /// rows (piece indexes) as offsets into the text the pieces were cut
    /// from, the first row's 0 left out
    pub fn offsets(units: &[String], rows: &[i64]) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        let mut at: i64 = 0;
        let mut r: i64 = 1;
        let mut i: i64 = 0;
        while i < (units.len() as i64) && r < (rows.len() as i64) {
            if i == rows[r as usize] {
                out.push(at);
                r += 1;
            }
            at += (MdText::str_len(&units[i as usize].clone()) as i64);
            i += 1;
        }
        out
    }
}

#[derive(Clone)]
pub struct MdCodeLines {
    pub lines: Vec<Rc<RefCell<MdCodeLine>>>,
    pub diff: bool,
    /// the language the code is coloured as ("" plain)
    pub lang: String,
    pub numbers: bool,
    /// highlight steps (`lines=` groups split by |); 0 when nothing is marked
    pub steps: i64,
}

impl MdCodeLines {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            diff: false,
            lang: String::new(),
            numbers: false,
            steps: 0,
        }
    }
    pub fn kindCode() -> i64 {
        0
    }
    pub fn kindAdded() -> i64 {
        1
    }
    pub fn kindRemoved() -> i64 {
        2
    }
    pub fn kindHunk() -> i64 {
        3
    }
    pub fn kindFile() -> i64 {
        4
    }
    /// literal: the block's text; info: its info string (without attributes);
    /// numbers: "" off, "1" from 1, "N" from N; marks: the lines= value
    pub fn read(literal: &str, info: &str, numbers: &str, marks: &str) -> Rc<RefCell<MdCodeLines>> {
        let mut out: Rc<RefCell<MdCodeLines>> = Rc::new(RefCell::new(MdCodeLines::new()));
        let mut words: Vec<String> = MdCodeLines::words(info);
        if !words.is_empty() && words[0].clone().to_lowercase() == "diff" {
            out.borrow_mut().diff = true;
            if (words.len() as i64) > 1 {
                out.borrow_mut().lang = words[1].clone();
            }
        } else {
            if !words.is_empty() {
                out.borrow_mut().lang = words[0].clone();
            }
        }
        let mut raw: Vec<String> = literal.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut count: i64 = raw.len() as i64;
        // a fenced block ends in a newline, which is not a line of code
        if count > 0 && (MdText::str_len(&raw[(count - 1) as usize].clone()) as i64) == 0 {
            count -= 1;
        }
        let mut start: i64 = 1;
        if !numbers.is_empty() {
            out.borrow_mut().numbers = true;
            let sv: i64 = MdCodeLines::intOf(numbers, -1);
            if sv >= 0 {
                start = sv;
            }
        }
        let mut hunks: bool = false;
        if out.borrow().diff {
            let mut h: i64 = 0;
            while h < count {
                if raw[h as usize].clone().starts_with("@@".to_string().as_str()) {
                    hunks = true;
                }
                h += 1;
            }
        }
        let mut next: i64 = start;
        let mut in_hunk: bool = false;
        let mut line_at: i64 = 0;
        let mut i: i64 = 0;
        while i < count {
            let mut t: String = raw[i as usize].clone();
            let at: i64 = line_at;
            line_at = (line_at + (MdText::str_len(&t) as i64)) + 1;
            if !t.is_empty() {
                if MdText::char_at(&t, (MdText::str_len(&t) as i64) - 1) == 13 {
                    t = MdText::substr(&t, 0, (MdText::str_len(&t) as i64) - 1);
                }
            }
            // git's "\ No newline at end of file" says something about the
            // file, not a line of it: not shown
            if out.borrow().diff && t.starts_with("\\ No newline".to_string().as_str()) {
                i += 1;
                continue;
            }
            let mut l: Rc<RefCell<MdCodeLine>> = Rc::new(RefCell::new(MdCodeLine::new()));
            l.borrow_mut().text = t.clone();
            l.borrow_mut().code = t.clone();
            l.borrow_mut().litAt = at;
            if out.borrow().diff {
                if t.starts_with("@@".to_string().as_str()) {
                    l.borrow_mut().kind = 3;
                    in_hunk = true;
                    let ns: i64 = MdCodeLines::hunkStart(&t);
                    if ns >= 0 {
                        next = ns;
                    }
                } else {
                    if hunks && !in_hunk {
                        l.borrow_mut().kind = 4;
                    } else {
                        let mut c: i64 = -1;
                        if !t.is_empty() {
                            c = MdText::char_at(&t, 0);
                        }
                        if c == 43 || (c == 45 || c == 32) {
                            l.borrow_mut().marker = MdText::substr(&t, 0, 1);
                            l.borrow_mut().code = MdText::substr(&t, 1, (MdText::str_len(&t) as i64));
                        }
                        if c == 43 {
                            l.borrow_mut().kind = 1;
                        }
                        if c == 45 {
                            l.borrow_mut().kind = 2;
                        }
                    }
                }
            }
            if l.borrow().kind == 0 || l.borrow().kind == 1 {
                l.borrow_mut().number = next;
                next += 1;
            }
            out.borrow_mut().lines.push(l.clone());
            i += 1;
        }
        // without a gutter, lines= counts the block's lines from 1
        if !out.borrow().numbers {
            let mut k: i64 = 0;
            while k < (out.borrow().lines.len() as i64) {
                let mut lk: Rc<RefCell<MdCodeLine>> = out.borrow().lines[k as usize].clone();
                lk.borrow_mut().number = k + 1;
                k += 1;
            }
        }
        if out.borrow().diff {
            out.borrow().pairChanges();
        }
        out.borrow_mut().mark(marks);
        if !out.borrow().numbers {
            let __it_ln = out.borrow().lines.clone();
            for ln in __it_ln.iter().cloned() {
                ln.borrow_mut().number = -1;
            }
        }
        out
    }
    /// a byte that continues a character (UTF-8 continuation, UTF-16 low
    /// surrogate): a span never begins or ends before one
    pub fn inChar(t: &str, at: i64) -> bool {
        if at <= 0 || at >= (MdText::str_len(&t) as i64) {
            return false;
        }
        let c: i64 = MdText::char_at(&t, at);
        c >= 128 && c < 192 || c >= 56320 && c < 57344
    }
    pub fn isWord(c: i64) -> bool {
        if (c >= 48 && c <= 57) {
            return true;
        }
        if (c >= 65 && c <= 90) {
            return true;
        }
        if (c >= 97 && c <= 122) {
            return true;
        }
        c == 95 || c >= 128
    }
    /// what differs between a removed line and the added line it became
    pub fn changeOf(old: &Rc<RefCell<MdCodeLine>>, now: &Rc<RefCell<MdCodeLine>>) {
        let a: String = old.borrow().code.clone();
        let b: String = now.borrow().code.clone();
        let la: i64 = MdText::str_len(&a) as i64;
        let lb: i64 = MdText::str_len(&b) as i64;
        let mut most: i64 = la;
        if lb < most {
            most = lb;
        }
        let mut pre: i64 = 0;
        while pre < most && MdText::char_at(&a, pre) == MdText::char_at(&b, pre) {
            pre += 1;
        }
        let mut suf: i64 = 0;
        while suf < most - pre && MdText::char_at(&a, (la - 1) - suf) == MdText::char_at(&b, (lb - 1) - suf) {
            suf += 1;
        }
        // the same line, or one with nothing in common worth pointing at
        if pre == la && pre == lb {
            return;
        }
        let mut longer: i64 = la;
        if lb > longer {
            longer = lb;
        }
        if (pre + suf) * 2 < longer {
            return;
        }
        while MdCodeLines::inChar(&a, pre) {
            pre -= 1;
        }
        while suf > 0 && (MdCodeLines::inChar(&a, la - suf) || MdCodeLines::inChar(&b, lb - suf)) {
            suf -= 1;
        }
        // whole words: a renamed identifier is marked whole, not its last
        // letters
        while pre > 0 && (MdCodeLines::isWord(MdText::char_at(&a, pre - 1)) && (pre < la && MdCodeLines::isWord(MdText::char_at(&a, pre)) || pre < lb && MdCodeLines::isWord(MdText::char_at(&b, pre)))) {
            pre -= 1;
        }
        while suf > 0 && (MdCodeLines::isWord(MdText::char_at(&a, la - suf)) && (la - suf > pre && MdCodeLines::isWord(MdText::char_at(&a, (la - suf) - 1)) || lb - suf > pre && MdCodeLines::isWord(MdText::char_at(&b, (lb - suf) - 1)))) {
            suf -= 1;
        }
        if la - suf > pre {
            old.borrow_mut().changeFrom = pre;
            old.borrow_mut().changeTo = la - suf;
        }
        if lb - suf > pre {
            now.borrow_mut().changeFrom = pre;
            now.borrow_mut().changeTo = lb - suf;
        }
    }
    /// the new-file start of "@@ -10,6 +12,7 @@", -1 when it does not say
    pub fn hunkStart(t: &str) -> i64 {
        let at: i64 = MdText::index_of(&t, " +");
        if at < 0 {
            return -1;
        }
        let i: i64 = at + 2;
        let mut j: i64 = i;
        while j < (MdText::str_len(&t) as i64) && (MdText::char_at(&t, j) >= 48 && MdText::char_at(&t, j) <= 57) {
            j += 1;
        }
        if j == i {
            return -1;
        }
        MdCodeLines::intOf(&MdText::substr(&t, i, j), -1)
    }
    pub fn intOf(s: &str, fallback: i64) -> i64 {
        let t: String = s.trim().to_string();
        if t.is_empty() {
            return fallback;
        }
        let mut v: i64 = 0;
        let mut i: i64 = 0;
        while i < (MdText::str_len(&t) as i64) {
            let c: i64 = MdText::char_at(&t, i);
            if c < 48 || c > 57 {
                return fallback;
            }
            v = v * 10 + (c - 48);
            i += 1;
        }
        v
    }
    pub fn words(info: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cur: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&info) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&info, i);
            if c == 32 || c == 9 {
                if !cur.is_empty() {
                    out.push(cur.clone());
                }
                cur = String::new();
            } else {
                if c == 123 {
                    // attributes start
                    break;
                }
                cur = format!("{}{}", cur, MdText::substr(&info, i, i + 1));
            }
            i += 1;
        }
        if !cur.is_empty() {
            out.push(cur.clone());
        }
        out
    }
    /// each run of removed lines and the run of added lines right after it,
    /// paired line by line
    pub fn pairChanges(&self) {
        let n: i64 = self.lines.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let mut li: Rc<RefCell<MdCodeLine>> = self.lines[i as usize].clone();
            if li.borrow().kind != 2 {
                i += 1;
                continue;
            }
            let r0: i64 = i;
            while i < n && self.kindAt(i) == 2 {
                i += 1;
            }
            let a0: i64 = i;
            while i < n && self.kindAt(i) == 1 {
                i += 1;
            }
            let mut k: i64 = 0;
            while r0 + k < a0 && a0 + k < i {
                MdCodeLines::changeOf(&self.lines[(r0 + k) as usize].clone(), &self.lines[(a0 + k) as usize].clone());
                k += 1;
            }
        }
    }
    pub fn kindAt(&self, i: i64) -> i64 {
        let mut l: Rc<RefCell<MdCodeLine>> = self.lines[i as usize].clone();
        return l.borrow().kind;
    }
    /// lines= : "12-14,20" one step, "12-14|20" two
    pub fn mark(&mut self, spec: &str) {
        let s: String = spec.trim().to_string();
        if s.is_empty() {
            return;
        }
        let mut groups: Vec<String> = s.split("|").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut step: i64 = 0;
        for g in groups.iter().cloned() {
            let mut any: bool = false;
            let mut parts: Vec<String> = g.split(",").map(|s| s.to_string()).collect::<Vec<String>>();
            for p in parts.iter().cloned() {
                let r: String = p.trim().to_string();
                if r.is_empty() {
                    continue;
                }
                let mut from: i64 = -1;
                let mut to: i64 = -1;
                let dash: i64 = MdText::index_of(&r, "-");
                if dash > 0 {
                    from = MdCodeLines::intOf(&MdText::substr(&r, 0, dash), -1);
                    to = MdCodeLines::intOf(&MdText::substr(&r, dash + 1, (MdText::str_len(&r) as i64)), -1);
                } else {
                    from = MdCodeLines::intOf(&r, -1);
                    to = from;
                }
                if from < 0 || to < from {
                    continue;
                }
                if !any {
                    step += 1;
                    any = true;
                }
                let __it_l = self.lines.clone();
                for l in __it_l.iter().cloned() {
                    if (l.borrow().number >= from && l.borrow().number <= to) && l.borrow().mark == 0 {
                        l.borrow_mut().mark = step;
                    }
                }
            }
        }
        self.steps = step;
    }
}
