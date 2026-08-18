//! Compile-pass: `#[fast_observe::main]` accepts the `fast_observe::Result`
//! alias as the return type — the return-type check token-walks for a final
//! `Result` segment instead of matching a bare `Result<` prefix.
//! No `.stderr` file — pass cases.

#[fast_observe::main]
fn main() -> fast_observe::Result<()> {
    let _ = 42;
    Ok(())
}
