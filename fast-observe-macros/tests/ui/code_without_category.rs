//! `#[code]` without `category` — the macro requires both (or neither).
#![feature(error_generic_member_access)]

use fast_observe::error;

error! {
    /// Errors of the test engine.
    #[derive(Debug)]
    pub enum CodeAlone {
        /// a coded variant missing its category
        #[error("code without category")]
        #[code = "E901"]
        CodeAlone,
    }
}

fn main() {}
