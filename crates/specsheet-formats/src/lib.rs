//! Parsers for SCS Software game file formats.
//!
//! Every parser takes bytes (never touches archives or the disk) and treats
//! its input as untrusted: malformed data yields an error, never a panic.

mod binary;
pub mod bsii;
pub mod mat;
pub mod mod_manifest;
pub mod paths;
pub mod pmd;
pub mod pmg;
pub mod scsc;
pub mod sii;
pub mod token;
pub mod ui_markup;

pub use binary::FormatError;
