// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;

use crate::rgr::MdText;

/// ============================================================================
/// MdEmbedKinds.rgr — which fence words name a drawing, and in which notation.
/// ============================================================================
///
/// ````markdown
/// ```mermaid      ```plantuml     ```dot        ```d2      ```vega-lite
/// ````
///
/// Two places need this answer and they must not disagree. The PARSER needs it
/// to decide whether a fenced block is code or a slot — that decision changes
/// the node's kind, and a parser that says "code" while the renderer says
/// "diagram" leaves a hole nobody fills. The RENDERER needs it to pick which
/// reader the fence's text goes to.
///
/// So the table lives here, on its own, with NO imports: the parser must not
/// grow a dependency on a graph library to find out that `dot` is a drawing,
/// and the renderer must not carry a second copy of the list.
///
///   fence word                  notation
///   ─────────────────────────   ────────────
///   mermaid                     mermaid
///   plantuml, puml              plantuml
///   dot, graphviz               graphviz
///   d2                          d2
///   vega, vega-lite, vegalite   vega
///   app                         app
///   process, swot, timeline,    figure
///   stats, cards
///
/// The notation is what `MdDiagram` dispatches on, not the fence word: two
/// spellings of the same reader should not be two branches downstream.
///
/// A word earns a row here only once something can DRAW it. A fence listed
/// here that nothing reads is worse than one left as code: it turns a
/// syntax-highlighted block into an empty slot with an apology in it. `d2` sat
/// out of this table for exactly that reason until RangerFlow grew a D2 reader
/// and `D2Render` put a door on it.
/// ============================================================================
#[derive(Clone)]
pub struct MdEmbedKinds {
}

impl MdEmbedKinds {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The notation a fence word belongs to, or "" when it names a language
    /// rather than a drawing.
    pub fn notationOf(word: &str) -> String {
        if word == "mermaid" {
            return "mermaid".to_string();
        }
        if word == "plantuml" {
            return "plantuml".to_string();
        }
        if word == "puml" {
            return "plantuml".to_string();
        }
        if word == "dot" {
            return "graphviz".to_string();
        }
        if word == "graphviz" {
            return "graphviz".to_string();
        }
        if word == "d2" {
            return "d2".to_string();
        }
        // A statechart: an XState machine config (JSON, or the object in
        // `createMachine({ … })`), drawn by RangerFlow's `domains/xstate`.
        if word == "xstate" {
            return "xstate".to_string();
        }
        // A chart, not a diagram — the one notation here that is not a graph.
        // Both spellings reach the same reader: `VlCompile.compileSpec` takes
        // Vega-Lite to Vega and passes Vega through, so a writer does not have
        // to know which of the two they wrote.
        if word == "vega" {
            return "vega".to_string();
        }
        if word == "vega-lite" {
            return "vega".to_string();
        }
        if word == "vegalite" {
            return "vega".to_string();
        }
        // A slide layout computed from a list (MdFigure): steps, a SWOT
        // grid, a timeline.
        if word == "process" {
            return "figure".to_string();
        }
        if word == "swot" {
            return "figure".to_string();
        }
        if word == "stats" {
            return "figure".to_string();
        }
        if word == "timeline" {
            return "figure".to_string();
        }
        if word == "cards" {
            return "figure".to_string();
        }
        // A program the host runs (MdApp): the box it paints into, and a
        // plate with its name where nothing runs it.
        if word == "app" {
            return "app".to_string();
        }
        String::new()
    }
    /// The notation's own name, for a reader: "Mermaid", "PlantUML",
    /// "Graphviz", "D2" — and "Drawing" for a word this table does not know,
    /// which is what a group that came from nowhere in particular is called.
    /// One table, because the deck names its groups from it and the Word
    /// document names its drawings from it, and a reader who moves between
    /// the two should meet the same name.
    pub fn notationWord(word: &str) -> String {
        let n: String = MdEmbedKinds::notationOf(word);
        if n == "mermaid" {
            return "Mermaid".to_string();
        }
        if n == "plantuml" {
            return "PlantUML".to_string();
        }
        if n == "graphviz" {
            return "Graphviz".to_string();
        }
        if n == "d2" {
            return "D2".to_string();
        }
        if n == "xstate" {
            return "XState".to_string();
        }
        if n == "vega" {
            return "Vega".to_string();
        }
        if n == "figure" {
            return "Figure".to_string();
        }
        if n == "app" {
            return "App".to_string();
        }
        "Drawing".to_string()
    }
    pub fn isDrawing(word: &str) -> bool {
        let n: String = MdEmbedKinds::notationOf(word);
        !n.is_empty()
    }
    /// A diagram drawn narrower than the column it lands in, because the
    /// document said so:
    ///
    /// ````markdown
    /// ```mermaid
    /// flowchart LR
    ///   A --> B
    /// ```
    /// {width=360}
    /// ````
    ///
    /// Both the RENDERER and the LAYOUT have to arrive at the same number. The
    /// embed cache is keyed by the width a diagram was prepared at, so a
    /// disagreement of one point is a diagram prepared at one width and asked
    /// for at another — a slot with an apology in it rather than a drawing. One
    /// function, two callers, the same reason `notationOf` lives here.
    ///
    /// Never WIDER than the column. A diagram that overflowed its column would
    /// draw across the margin, and `{width=2000}` is a typo far more often than
    /// it is a wish. Anything that is not a plain number is the column width,
    /// because refusing to draw over a unit nobody parsed helps no one.
    pub fn widthOf(want0: &str, avail: f64) -> f64 {
        let mut want: String = want0.to_string();
        let mut n: i64 = MdText::str_len(&want) as i64;
        if n == 0 {
            return avail;
        }
        // `60%`: a share of the room
        let mut share: bool = false;
        if MdText::char_at(&want, n - 1) == 37 {
            share = true;
            want = MdText::substr(&want, 0, n - 1);
            n -= 1;
            if n == 0 {
                return avail;
            }
        }
        let mut v: i64 = 0;
        let mut digits: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = MdText::char_at(&want, i);
            if c < 48 {
                return avail;
            }
            if c > 57 {
                return avail;
            }
            v = v * 10 + (c - 48);
            digits += 1;
            if digits > 5 {
                return avail;
            }
            i += 1;
        }
        if digits == 0 {
            return avail;
        }
        let mut out: f64 = v as f64;
        if share {
            out = (avail * out) / 100.0;
        }
        if out <= 0.0 {
            return avail;
        }
        if out > avail {
            return avail;
        }
        out
    }
    /// The first whitespace-delimited word of an info string — `mermaid` out of
    /// "mermaid theme=dark".
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
    /// Whether a chart's text states a height of its own: `"height": 300`.
    /// `"height": "container"` is no height (the chart takes its room, as
    /// one that says nothing does); read as one, a chart written for a web
    /// page was never drawn again for the room a slide had under its title.
    pub fn statesHeight(text: &str) -> bool {
        let mut at: i64 = MdText::index_of(&text, "\"height\"");
        while at >= 0 {
            let mut i: i64 = at + 8;
            let n: i64 = MdText::str_len(&text) as i64;
            // past the colon and the white space round it
            let mut going: bool = true;
            while going && i < n {
                let c: i64 = MdText::char_at(&text, i);
                if c == 58 || (c == 32 || (c == 9 || (c == 10 || c == 13))) {
                    i += 1;
                } else {
                    going = false;
                }
            }
            if i < n && MdText::char_at(&text, i) != 34 {
                return true;
            }
            let rest: i64 = MdText::index_of(&MdText::substr(&text, at + 8, n), "\"height\"");
            if rest < 0 {
                return false;
            }
            at = (at + 8) + rest;
        }
        false
    }
}
