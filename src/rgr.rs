//! The .rgr classes the crate's modules use, imported once. cargo builds
//! MdRgr.rgr as one module (build.rs) and takes EVG's types from the
//! rangerevg crate; Ranger reads the EVG files below.

#[allow(dead_code, unused, non_snake_case, clippy::all)]
mod rgr {
    ranger::import_rgr!("MdRgr");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGBox");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGColor");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGDisplayList");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGElement");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGLayout");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGTextEngine");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGTextMeasurer");
    #[cfg(any())]
    ranger::import_rgr!("../../../lib/evg/EVGUnit");
    #[cfg(any())]
    ranger::import_rgr!("../../css/CssCore");
    #[cfg(any())]
    ranger::import_rgr!("../../vfs/src/Vfs");
    #[cfg(any())]
    ranger::import_rgr!("../../vfs/src/VfsCore");
    #[cfg(any())]
    ranger::import_rgr!("MdEmoji");
    #[cfg(any())]
    ranger::import_rgr!("MdMath");
    #[cfg(any())]
    ranger::import_rgr!("MdText");
}
#[cfg(not(ranger))]
pub use rangerevg::{EVGBox, EVGColor, EVGDisplayList, EVGDrawCmd, EVGElement, EVGLayout, EVGTextEngine, EVGTextMeasurer, EVGTextMetrics, EVGUnit};
pub use rgr::{CssCascade, CssComputed, CssNode, CssRule, CssSelector, CssSelectorPart, CssSheet, CssValue, MdEmoji, MdMath, MdText, Vfs, VfsStat};
