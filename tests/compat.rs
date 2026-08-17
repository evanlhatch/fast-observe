//! Boundary conversions between foreign error types and `Fault`
//! (DESIGN.md §5.9). Explicit in both directions — never implicit `From`.
//! Each compat feature is tested independently: one module per feature.
#![allow(clippy::unwrap_used, reason = "test")]

#[cfg(feature = "anyhow-boundary")]
mod anyhow_tests {
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
}

#[cfg(feature = "compat-eyre")]
mod eyre_tests {
    use fast_observe::Fault;
    use fast_observe::compat::eyre_boundary::{EyreError, from_eyre, into_eyre};

    #[test]
    fn eyre_to_fault_displays_message() {
        let result: eyre::Result<()> = Err(eyre::eyre!("boundary failure"));
        let fault: Fault<EyreError> = result.map_err(from_eyre).unwrap_err();
        assert_eq!(fault.to_string(), "boundary failure");
        // eyre's cause chain survives via alternate Display on the wrapper.
        let chained = eyre::Report::new(std::io::Error::other("outer")).wrap_err("ctx");
        let fault = from_eyre(chained);
        let alt = format!("{:#}", fault.inner());
        assert!(alt.contains("ctx"), "chain lost: {alt}");
        assert!(alt.contains("outer"), "chain lost: {alt}");
    }

    #[test]
    fn fault_to_eyre_keeps_message_and_debug() {
        let fault = Fault::from(std::io::Error::other("typed failure"));
        let err = into_eyre(fault);
        assert_eq!(err.to_string(), "typed failure");
        let debug = format!("{err:?}");
        assert!(debug.contains("typed failure"), "tree lost: {debug}");
    }

    #[test]
    fn round_trip_displays_match() {
        let before = eyre::eyre!("round trip").to_string();
        let fault = from_eyre(eyre::eyre!("round trip"));
        assert_eq!(fault.to_string(), before);
        let back = into_eyre(fault);
        assert_eq!(back.to_string(), before);
    }
}

#[cfg(feature = "compat-error-stack")]
mod error_stack_tests {
    use fast_observe::Fault;
    use fast_observe::compat::error_stack_boundary::{
        ErrorStackReport, from_error_stack, into_error_stack,
    };

    #[test]
    fn report_to_fault_preserves_context_type_and_frames() {
        let report = error_stack::Report::new(std::io::Error::other("stack failure"))
            .attach_printable("extra detail");
        let fault: Fault<ErrorStackReport<std::io::Error>> = from_error_stack(report);
        assert_eq!(fault.to_string(), "stack failure");
        // The frame stack survived inside the wrapper: >= 2 frames
        // (context + printable attachment; error-stack also adds a
        // `Location` attachment frame at capture).
        let inner = fault.inner();
        assert!(inner.frames().count() >= 2, "frames lost");
        // Typed context still reachable.
        let ctx = inner.current_context();
        assert_eq!(ctx.to_string(), "stack failure");
        // Debug renders the attachment.
        let debug = format!("{inner:?}");
        assert!(debug.contains("extra detail"), "attachment lost: {debug}");
    }

    #[test]
    fn fault_to_report_keeps_message_and_context() {
        let fault = Fault::from(std::io::Error::other("typed failure"));
        let report = into_error_stack(fault);
        assert_eq!(report.to_string(), "typed failure");
        // The Fault itself is the current context; Deref reaches the
        // original typed error.
        let ctx: &Fault<std::io::Error> = report.current_context();
        assert_eq!(ctx.kind(), std::io::ErrorKind::Other);
        // Fault's source chain was walked into frames by Report::new;
        // the full tree renders via Debug.
        let debug = format!("{report:?}");
        assert!(debug.contains("typed failure"), "tree lost: {debug}");
    }

    #[test]
    fn round_trip_displays_match() {
        let before = error_stack::Report::new(std::io::Error::other("round trip")).to_string();
        let fault = from_error_stack(error_stack::Report::new(std::io::Error::other(
            "round trip",
        )));
        assert_eq!(fault.to_string(), before);
        let back = into_error_stack(Fault::from(std::io::Error::other("round trip")));
        assert_eq!(back.to_string(), before);
    }
}

// tokio boundary (feature `int-tokio`): TYPE-CHECK ONLY. `JoinError` cannot
// be constructed without a tokio runtime, and `int-tokio` does not enable
// tokio's `rt` feature (dev-dependencies cannot add features), so no
// runtime test exists yet — see src/tokio_ext.rs module docs. This module
// also requires lib.rs to export `tokio_ext` (integration step).
#[cfg(feature = "int-tokio")]
mod tokio_tests {
    use fast_observe::tokio_ext::{JoinTaskError, ObserveJoinExt};

    #[allow(dead_code, reason = "compile-only until tokio/rt is enabled")]
    fn observe_join_typecheck(
        j: Result<(), tokio::task::JoinError>,
    ) -> fast_observe::Result<(), JoinTaskError> {
        j.observe_join("t")
    }
}
