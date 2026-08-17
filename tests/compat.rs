//! Boundary conversions between anyhow errors and `Fault` (feature
//! `anyhow-boundary`, DESIGN.md §5.9). Explicit in both directions — never
//! implicit `From`.
#![cfg(feature = "anyhow-boundary")]
#![allow(clippy::unwrap_used, reason = "test")]

use fast_observe::Fault;
use fast_observe::compat::anyhow_boundary::{AnyhowError, from_anyhow, into_anyhow};

#[test]
fn anyhow_to_fault_displays_message() {
    let result: anyhow::Result<()> = Err(anyhow::anyhow!("boundary failure"));
    let fault: Fault<AnyhowError> = result.map_err(from_anyhow).unwrap_err();
    assert_eq!(fault.to_string(), "boundary failure");
    // anyhow's cause chain survives via alternate Display on the wrapper.
    let chained = anyhow::anyhow!("outer").context("ctx");
    let fault = from_anyhow(chained);
    let alt = format!("{:#}", fault.inner());
    assert!(alt.contains("ctx"), "chain lost: {alt}");
    assert!(alt.contains("outer"), "chain lost: {alt}");
}

#[test]
fn fault_to_anyhow_keeps_message_and_debug() {
    // Typed error: `Fault<SimpleError>` has no `Error` impl (BoxError is
    // unsized), so `into_anyhow` takes a typed `Fault<E>` only.
    let fault = Fault::from(std::io::Error::other("typed failure"));
    let err = into_anyhow(fault);
    assert_eq!(err.to_string(), "typed failure");
    let debug = format!("{err:?}");
    assert!(debug.contains("typed failure"), "tree lost: {debug}");
}

#[test]
fn round_trip_displays_match() {
    let before = anyhow::anyhow!("round trip").to_string();
    let fault = from_anyhow(anyhow::anyhow!("round trip"));
    assert_eq!(fault.to_string(), before);
    let back = into_anyhow(fault);
    assert_eq!(back.to_string(), before);
}
