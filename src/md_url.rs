// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;

use crate::md_char::MdChar;
use crate::rgr::MdText;

/// ============================================================================
/// MdUrl.rgr — which addresses a link or a picture may keep
/// ============================================================================
///
/// A slide is drawn, not run, but its links travel: into the PDF as link
/// annotations, into Word as hyperlinks, into the web page export as <a>.
/// A `javascript:` or `vbscript:` address (or a `data:` one that is not a
/// picture) runs script in whatever opens it, so every writer asks here and
/// keeps the text without the address.
/// ============================================================================
#[derive(Clone)]
pub struct MdUrl {
}

impl MdUrl {
    pub fn new() -> Self {
        Self {
        }
    }
    /// Would following this address run script? `javascript:` and `vbscript:`
    /// always; `data:` unless a picture is asked for and it is one. Browsers
    /// ignore case and leading blanks and controls, so this does.
    pub fn runsScript(dest: &str, picture: bool) -> bool {
        let mut d: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = MdText::str_len(&dest) as i64;
        while i < n {
            let c: i64 = MdText::char_at(&dest, i);
            if c > 32 {
                d = format!("{}{}", d, MdText::substr(&dest, i, i + 1));
            }
            i += 1;
        }
        d = MdChar::lower(&d);
        if MdText::index_of(&d, "javascript:") == 0 || MdText::index_of(&d, "vbscript:") == 0 {
            return true;
        }
        if MdText::index_of(&d, "data:") == 0 {
            // a picture's own script does not run inside <img>
            return !(picture && MdText::index_of(&d, "data:image/") == 0);
        }
        false
    }
    /// A link's address as a writer may keep it: "" for one that runs script.
    pub fn safeLink(dest: &str) -> String {
        if MdUrl::runsScript(dest, false) {
            return String::new();
        }
        dest.to_string()
    }
}
