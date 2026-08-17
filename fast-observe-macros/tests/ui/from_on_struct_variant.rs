//! `#[from]` on a struct variant — unsupported; mark a field `#[source]`.
#![feature(error_generic_member_access)]

use fast_observe::error;

error! {
    /// Errors of the test engine.
    #[derive(Debug)]
    pub enum BadFrom {
        /// struct variant wrongly marked #[from]
        #[error("wrapped: {inner:?}")]
        #[from]
        Wrapped {
            /// would-be source field
            inner: std::io::Error,
        },
    }
}

fn main() {}
