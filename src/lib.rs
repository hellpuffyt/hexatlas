//! hexatlas: inspect and diff binary files against reusable structure
//! templates, with a live hex + field-annotation view.
//!
//! A template describes a binary format once, in a small DSL (see
//! [`parser`]), and is then applied ([`eval`]) to any number of files to
//! decode a flat, ordered list of fields. Those fields drive an annotated
//! hex dump ([`hexview`]) and a structural diff between two files
//! ([`diff`]).
//!
//! ```
//! use hexatlas::eval::apply;
//! use hexatlas::parser::parse_template;
//!
//! let template = parse_template(
//!     "struct Header { magic: magic \"HX\"; version: u16le; } root Header;",
//! )
//! .unwrap();
//!
//! let bytes = [b'H', b'X', 0x01, 0x00];
//! let fields = apply(&template, &bytes).unwrap();
//!
//! assert_eq!(fields[0].path, "magic");
//! assert_eq!(fields[1].rendered, "1");
//! ```

pub mod ast;
pub mod diff;
pub mod eval;
pub mod hexview;
pub mod json;
pub mod lexer;
pub mod parser;
