//! `#[code = "nope"]` — codes must match `^[A-Z]+[0-9]+$` (e.g. "E100").
#![feature(error_generic_member_access)]

use fast_observe::error;

error! {
    /// Errors of the test engine.
    #[derive(Debug)]
    pub enum BadCode {
        /// a variant with a malformed code
        #[error("invalid code format")]
        #[code = "nope", category = Content]
        BadCode,
    }
}

fn main() {}
