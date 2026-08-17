//! `category` without `#[code]` — rejected: uncoded variants take neither.
#![feature(error_generic_member_access)]

use fast_observe::error;

error! {
    /// Errors of the test engine.
    #[derive(Debug)]
    pub enum CategoryAlone {
        /// a categorized variant missing its code
        #[error("category without code")]
        #[category = Content]
        CategoryAlone,
    }
}

fn main() {}
