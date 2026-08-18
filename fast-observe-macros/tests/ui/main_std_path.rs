//! Compile-pass: `#[fast_observe::main]` accepts the fully qualified
//! `std::result::Result` return path — same token-walk as main_alias.rs.
//! No `.stderr` file — pass cases.

#[derive(Debug)]
struct Boom;

impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "boom")
    }
}
impl std::error::Error for Boom {}

#[fast_observe::main]
fn main() -> std::result::Result<(), Boom> {
    let _ = 42;
    Ok(())
}
