//! The .rgr classes the crate's modules use, imported once.

#[allow(dead_code, unused, non_snake_case)]
mod rgr {
    ranger::import_rgr!("../../../lib/evg/EVGColor");
    ranger::import_rgr!("../../../lib/evg/EVGDisplayList");
    ranger::import_rgr!("../../../lib/evg/EVGElement");
    ranger::import_rgr!("../../../lib/evg/EVGLayout");
    ranger::import_rgr!("../../../lib/evg/EVGTextEngine");
    ranger::import_rgr!("../../../lib/evg/EVGTextMeasurer");
    ranger::import_rgr!("../../../lib/evg/EVGUnit");
    ranger::import_rgr!("../../css/CssCore");
    ranger::import_rgr!("../../vfs/src/Vfs");
    ranger::import_rgr!("../../vfs/src/VfsCore");
    ranger::import_rgr!("MdEmoji");
    ranger::import_rgr!("MdMath");
    ranger::import_rgr!("MdText");
}
pub use rgr::{CssCascade, CssComputed, CssNode, CssRule, CssSelector, CssSelectorPart, CssSheet, CssValue, EVGColor, EVGDisplayList, EVGDrawCmd, EVGElement, EVGLayout, EVGTextEngine, EVGTextMeasurer, EVGTextMetrics, EVGUnit, MdEmoji, MdMath, MdText, Vfs, VfsStat};
