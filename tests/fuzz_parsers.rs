//! Fuzz target: config parser robustness.
//!
//! Two pure targets (no process state touched):
//!
//! - `config::Backends::from_env_value` on arbitrary and token-biased
//!   strings. Invariants: never panics; when the comma-split trimmed
//!   lowercase tokens contain "off" AND any other non-empty, non-"off"
//!   token the result must be `None`; when every token is in the known
//!   set {off,instant,fastrace,web,puffin,tracy,superluminal,tracing}
//!   and the off-rule holds, the result must be `Some`.
//! - `deploy::DeploymentConfig` with arbitrary Strings in `level`,
//!   `layout`, `backends`, `traces` → `.apply(deploy::observe())`.
//!   Invariants: never panics; on `Err(errors)` every `ConfigError` has
//!   non-empty `field` and `reason`. `.init()` is NEVER called — it
//!   installs the global logger.

use bolero::generator::TypeGenerator;
use fast_observe::config::Backends;
use fast_observe::deploy::{DeploymentConfig, observe};

/// The known backend tokens (`Backends::from_env_value`'s grammar).
const KNOWN: &[&str] = &[
    "off",
    "instant",
    "fastrace",
    "web",
    "puffin",
    "tracy",
    "superluminal",
    "tracing",
];

/// One comma-separated token — known-name-biased or free-form.
#[derive(Debug, Clone, TypeGenerator)]
enum Tok {
    /// A known token; high selector bits add padding/case mangling.
    Known(u8),
    /// Raw arbitrary token text.
    Free(String),
}

/// An `OBSERVE_PROFILE`-style value.
#[derive(Debug, Clone, TypeGenerator)]
enum EnvValue {
    /// Raw arbitrary string.
    Raw(String),
    /// Comma-joined structured tokens — biased toward parseable values.
    Tokens(Vec<Tok>),
}

impl EnvValue {
    fn build(&self) -> String {
        match self {
            Self::Raw(s) => s.clone(),
            Self::Tokens(toks) => toks.iter().map(tok_text).collect::<Vec<_>>().join(","),
        }
    }
}

fn tok_text(tok: &Tok) -> String {
    match tok {
        Tok::Free(s) => s.clone(),
        Tok::Known(sel) => {
            let bits = *sel;
            let name = KNOWN[usize::from(bits) % KNOWN.len()];
            let mut out = String::new();
            if bits & 0x10 != 0 {
                out.push(' ');
            }
            if bits & 0x20 != 0 {
                out.push_str(&name.to_uppercase());
            } else {
                out.push_str(name);
            }
            if bits & 0x40 != 0 {
                out.push(' ');
            }
            out
        }
    }
}

/// `from_env_value` never panics and follows the off-rule / known-set
/// contract.
fn check_from_env_value(input: &str) {
    let parsed = Backends::from_env_value(input);
    let tokens: Vec<String> = input
        .split(',')
        .map(|t| t.trim().to_ascii_lowercase())
        .collect();
    let has_off = tokens.iter().any(|t| t == "off");
    // "off,off" is still just OFF — only a non-"off" token clashes.
    let has_other = tokens.iter().any(|t| !t.is_empty() && t != "off");
    if has_off && has_other {
        assert!(
            parsed.is_none(),
            "`off` combined with another token must be None: {input:?}"
        );
    }
    let all_known = tokens.iter().all(|t| KNOWN.contains(&t.as_str()));
    if all_known && !(has_off && has_other) {
        assert!(
            parsed.is_some(),
            "known tokens without the off-clash must parse: {input:?}"
        );
    }
}

/// Arbitrary strings for the four string-parsed config fields.
#[derive(Debug, Clone, TypeGenerator)]
struct ConfigFuzz {
    level: String,
    layout: String,
    backends: String,
    traces: String,
}

/// `DeploymentConfig::apply` never panics; every returned error is
/// well-formed (non-empty `field` and `reason`).
fn check_apply(input: &ConfigFuzz) {
    let config = DeploymentConfig {
        level: Some(input.level.clone()),
        layout: Some(input.layout.clone()),
        backends: Some(input.backends.clone()),
        traces: Some(input.traces.clone()),
        ..DeploymentConfig::default()
    };
    // NEVER `.init()` the result — that installs the global logger.
    if let Err(errors) = config.apply(observe()) {
        for error in &errors {
            assert!(
                !error.field.is_empty(),
                "ConfigError.field must be non-empty: {error:?}"
            );
            assert!(
                !error.reason.is_empty(),
                "ConfigError.reason must be non-empty: {error:?}"
            );
        }
    }
}

#[test]
fn fuzz_backends_from_env_value() {
    bolero::check!()
        .with_type::<EnvValue>()
        .for_each(|v: &EnvValue| check_from_env_value(&v.build()));
}

#[test]
fn fuzz_deployment_config_apply() {
    bolero::check!()
        .with_type::<ConfigFuzz>()
        .for_each(|c: &ConfigFuzz| check_apply(c));
}
