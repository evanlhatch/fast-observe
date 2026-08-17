//! A variant without `#[error("...")]` — the Display template is required
//! on every variant.
#![feature(error_generic_member_access)]

use fast_observe::error;

error! {
    /// Errors of the test engine.
    #[derive(Debug)]
    pub enum NoTemplate {
        /// no display template at all
        Bare,
    }
}

fn main() {}
