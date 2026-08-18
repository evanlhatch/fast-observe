//! Compile-pass: `#[fast_observe::main]` wraps a `Result`-returning fn so
//! the `Err` arm routes through `Fault::exit_with_report` (report to stderr
//! + sysexits exit code). No `.stderr` file — pass cases.

#[derive(Debug)]
struct Boom;

impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "boom")
    }
}
impl std::error::Error for Boom {}

#[fast_observe::main]
fn main() -> Result<(), Boom> {
    let _ = 42;
    Ok(())
}
