// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;

use crate::md_char::MdChar;

/// ============================================================================
/// MdAnchor.rgr — a heading's anchor: the name a link or a selector uses
/// ============================================================================
///
/// `## Mihin raha menee?` is `#mihin-raha-menee`: the GitHub rule every
/// Markdown reader knows from in-page links — lower case, spaces become
/// hyphens, punctuation goes, letters of any script stay. The same name is
/// how a stylesheet reaches one section (`#mihin-raha-menee chart { … }`), so
/// a selector reads like the link to what it styles.
/// ============================================================================
#[derive(Clone)]
pub struct MdAnchor {
}

impl MdAnchor {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn slug(text: &str) -> String {
        let t: String = MdChar::lower(&text.trim().to_string());
        let mut out: String = String::new();
        let mut dash: bool = false;
        // by code point, the same on every target: on Go a `charAt` is a
        // byte, and "Ä" two of them
        let mut cs: Vec<i64> = t.chars().map(|c| c as i64).collect::<Vec<i64>>();
        let mut i: i64 = 0;
        let n: i64 = cs.len() as i64;
        while i < n {
            let mut c: i64 = cs[i as usize];
            // Latin-1 capitals (À..Þ) the ASCII lower-casing leaves alone
            if ((c >= 192 && c <= 222)) && c != 215 {
                c += 32;
            }
            let ch: String = MdChar::fromCode(c);
            let ascii: bool = ((c >= 97 && c <= 122) || (c >= 48 && c <= 57)) || c == 95;
            // letters of other scripts stay; Latin-1 signs and the general
            // punctuation block (dashes, quotes) go
            let other: bool = ((c >= 192 && c != 215) && c != 247) && (c < 8192 || c > 8303);
            if ascii || other {
                if dash && !out.is_empty() {
                    out = format!("{}-", out);
                }
                dash = false;
                out = format!("{}{}", out, ch);
            } else {
                if (c == 32 || c == 45) || c == 9 {
                    dash = true;
                }
            }
            i += 1;
        }
        out
    }
}
