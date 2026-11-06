//! RangerMarkdown: Markdown parsed, laid out and drawn, as one crate. The
//! .rgr code imports this file and uses its types by name.

mod md_anchor;
mod md_attrs;
mod md_block;
mod md_book;
mod md_cell_tones;
mod md_char;
mod md_code_highlight;
mod md_code_lines;
mod md_columns;
mod md_container;
mod md_css;
mod md_css_limits;
mod md_css_support;
mod md_embed;
mod md_embed_kinds;
mod md_entities;
mod md_front_matter;
mod md_gallery;
mod md_html;
mod md_html_md;
mod md_inline;
mod md_layout;
mod md_node;
mod md_slide_layout;
mod md_slide_meta;
mod md_style;
mod md_svg;
mod md_table_split;
mod md_to_evg;
mod md_typography;
mod md_url;
mod rgr;

// the crate's types, for a crate that depends on it (`use rangermarkdown::MdLayout`)
pub use md_anchor::{MdAnchor};
pub use md_attrs::{MdAttrs};
pub use md_block::{MdBlock};
pub use md_book::{MdBook};
pub use md_cell_tones::{MdCellTones};
pub use md_char::{MdChar};
pub use md_code_highlight::{MdTokenKind, MdToken, MdHighlightedLine, MdLanguage, MdCodeHighlight};
pub use md_code_lines::{MdCodeLine, MdCodeWrap, MdCodeLines};
pub use md_columns::{MdColumnsPlan, MdColumns};
pub use md_container::{MdContainer};
pub use md_css::{MdCss};
pub use md_css_limits::{MdCssLimits};
pub use md_css_support::{MdCssNote, MdCssSupport};
pub use md_embed::{MdEmbedEntry, MdFigureStyle, MdEmbedCache};
pub use md_embed_kinds::{MdEmbedKinds};
pub use md_entities::{MdEntities};
pub use md_front_matter::{MdMeta, MdFrontMatter};
pub use md_gallery::{MdGalleryItem, MdCellLook, MdGalleryLook, MdGalleryCell, MdGallery};
pub use md_html::{MdHtmlTag, MdHtmlCell, MdHtmlRow, MdHtmlTable, MdHtml};
pub use md_html_md::{MdHtmlItem, MdHtmlList, MdHtmlStyle, MdHtmlMd};
pub use md_inline::{MdDelim, MdBracket, MdInline};
pub use md_layout::{MdRun, MdSeg, MdLine, MdSlotPic, MdBox, MdContinuations, MdLayout};
pub use md_node::{MdNode, MdNotes, MdLinkRef};
pub use md_slide_layout::{MdSlideLayout};
pub use md_slide_meta::{MdSlideMeta};
pub use md_style::{MdStyle, MdTypo};
pub use md_svg::{MdSvgUse, MdSvg};
pub use md_table_split::{MdTablePlan, MdTableSplit};
pub use md_to_evg::{MdToEvg};
pub use md_typography::{MdInlineOptions, MdTypoCell, MdTypoGroup, MdQuoteItem, MdQuoteOpen, MdTypography};
pub use md_url::{MdUrl};

// the .rgr types too, for the crates whose .rgr code uses this crate
// (-rust-extern names them through `use rangermarkdown::*`)
#[cfg(not(ranger))]
pub use rgr::*;
