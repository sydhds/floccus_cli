//! A crate to handle the XMLBookmarkExchangeLanguage format (Xbel) used by Floccus

pub mod xbel_format;

pub use xbel_format::{Xbel, XbelError, XbelItem, XbelPath};
pub use xbel_format::{XbelItemOrEnd, XbelNestingIterator};
