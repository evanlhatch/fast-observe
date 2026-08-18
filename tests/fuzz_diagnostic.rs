//! Fuzz target: diagnostic rendering with hostile spans.
//!
//! Property (unconditional): `render_diagnostic` NEVER panics, always
//! produces non-empty output containing the diagnostic code, and renders
//! deterministically (same input, byte-identical output) — for arbitrary
//! code/message/advice strings (unicode, control chars), arbitrary severity,
//! and arbitrary spans (start/end any `usize`: start>end, empty,
//! `usize::MAX`) against small registered in-memory sources plus one source
//! name that resolves nowhere (filesystem read fails → fallback path).
//!
//! Reversed spans cannot panic: `build_report` clamps every label range to
//! `(min, max)` before constructing ariadne `Label`s (ariadne asserts
//! `start <= end`).

use bolero::generator::TypeGenerator;
use camino::Utf8PathBuf;
use fast_observe::diagnostic::register_source;
use fast_observe::{Diagnostic, LabelSpan, Severity, SourceSpan, render_diagnostic};

/// (name, contents) pairs registered in-memory before fuzzing. Small on
/// purpose: hostile offsets are large, the sources are not.
const SOURCES: &[(&str, &str)] = &[
    ("fuzz-small.txt", "abc def ghi\n"),
    ("fuzz-empty.txt", ""),
    ("fuzz-unicode.txt", "héllo wörld ☃\nsecond line\n"),
    ("fuzz-nonl.txt", "no trailing newline"),
];

/// Never registered and (almost surely) not on disk — exercises the
/// ariadne-cache-miss → render-failure fallback path.
const MISSING: &str = "fuzz-definitely-not-registered-☃.txt";

/// All file names a label can point at.
const FILES: &[&str] = &[
    "fuzz-small.txt",
    "fuzz-empty.txt",
    "fuzz-unicode.txt",
    "fuzz-nonl.txt",
    MISSING,
];

#[derive(Debug, Clone, TypeGenerator)]
struct FuzzLabel {
    file: u8,
    start: usize,
    end: usize,
    message: Option<String>,
    primary: bool,
}

#[derive(Debug, Clone, TypeGenerator)]
struct FuzzDiagnostic {
    code: String,
    severity: u8,
    message: String,
    advice: Option<String>,
    labels: Vec<FuzzLabel>,
}

impl FuzzDiagnostic {
    fn build(&self) -> Diagnostic {
        let severity = match self.severity % 3 {
            0 => Severity::Error,
            1 => Severity::Warning,
            _ => Severity::Info,
        };
        Diagnostic {
            code: self.code.clone(),
            severity,
            message: self.message.clone(),
            labels: self
                .labels
                .iter()
                .map(|l| LabelSpan {
                    span: SourceSpan {
                        file: Utf8PathBuf::from(FILES[usize::from(l.file) % FILES.len()]),
                        start: l.start,
                        end: l.end,
                    },
                    message: l.message.clone(),
                    primary: l.primary,
                })
                .collect(),
            advice: self.advice.clone(),
        }
    }
}

fn register_sources() {
    for (name, contents) in SOURCES {
        register_source(*name, *contents);
    }
}

/// Check the render invariants shared by the fuzz target and the probe.
fn assert_render_invariants(diag: &Diagnostic) {
    let out = render_diagnostic(diag);
    assert!(!out.is_empty(), "rendered output must be non-empty");
    assert!(
        out.contains(diag.code.as_str()),
        "rendered output must contain the code {:?}: {out:?}",
        diag.code,
    );
    let again = render_diagnostic(diag);
    assert_eq!(out, again, "rendering must be deterministic");
}

#[test]
fn fuzz_render_diagnostic_hostile_spans() {
    register_sources();
    bolero::check!()
        .with_type::<FuzzDiagnostic>()
        .for_each(|input: &FuzzDiagnostic| {
            assert_render_invariants(&input.build());
        });
}

/// Hand-picked hostile spans — a readable triage list complementing the
/// random search above. Every case, reversed spans included, must render
/// without panicking.
#[test]
fn probe_hostile_spans() {
    register_sources();
    let cases: &[(&str, usize, usize)] = &[
        ("fuzz-small.txt", 0, 3),                   // sane baseline
        ("fuzz-small.txt", 5, 2),                   // start > end
        ("fuzz-small.txt", 0, usize::MAX),          // end at MAX
        ("fuzz-small.txt", usize::MAX, usize::MAX), // both MAX
        ("fuzz-small.txt", usize::MAX, 0),          // reversed extreme
        ("fuzz-empty.txt", 0, 1),                   // past end of empty source
        ("fuzz-unicode.txt", 1, 2),                 // mid-char boundary
        ("fuzz-nonl.txt", 100, 200),                // far past EOF
        (MISSING, 0, 3),                            // unresolvable source
    ];
    for (file, start, end) in cases {
        let diag = Diagnostic {
            code: "E000".to_string(),
            severity: Severity::Error,
            message: "probe".to_string(),
            labels: vec![LabelSpan {
                span: SourceSpan {
                    file: Utf8PathBuf::from(*file),
                    start: *start,
                    end: *end,
                },
                message: None,
                primary: true,
            }],
            advice: None,
        };
        assert_render_invariants(&diag);
    }
}
