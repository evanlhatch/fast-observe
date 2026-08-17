//! Two variants sharing one code — registry codes must be unique per enum.
#![feature(error_generic_member_access)]

use fast_observe::error;

error! {
    /// Errors of the test engine.
    #[derive(Debug)]
    pub enum DupCodes {
        /// first claimant of E903
        #[error("first")]
        #[code = "E903", category = Content]
        First,

        /// second claimant of E903 — the duplicate
        #[error("second")]
        #[code = "E903", category = Transient]
        Second,
    }
}

fn main() {}
