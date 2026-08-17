{ pkgs, config, lib, ... }:
{
  languages.rust = {
    enable = true;
    channel = "nightly";
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
      "rust-src"
      "miri"
      "rustc-dev"
      "llvm-tools-preview"
    ];

    # Build flags (dev). NOTE: no -fuse-ld flag — the linker is managed by
    # devenv's native languages.rust.wild.enable option, not via RUSTFLAGS.
    rustflags = lib.concatStringsSep " " [
      "-C lto=off"
      "-C codegen-units=256"
      "-C opt-level=1"
      "-Zshare-generics=y"
      "-Zthreads=8"
      "-Zpolonius=next"
      "-Zinline-mir"
      "-Zub-checks"
      "-C target-cpu=native"
    ];

    # Use `wild` as the linker (fast, drop-in). Managed by devenv itself —
    # no mold, no -fuse-ld in RUSTFLAGS.
    wild.enable = true;
  };

  # ── Rust infrastructure packages ──────────────────────────────────
  packages = with pkgs; [
    cargo-sweep
    cargo-nextest
    cargo-llvm-cov
    cargo-expand
    cargo-edit
    cargo-outdated
    cargo-machete
    cargo-deny
    cargo-hack
    cargo-bloat
    cargo-bolero
    cargo-watch
    bacon
    cargo-mutants
  ];

  # ── Environment ────────────────────────────────────────────────
  # RUSTFLAGS is derived by devenv from languages.rust.rustflags (+ wild linker).
  # RUSTFLAGS_FIX: flag set for cargo fix / clippy --fix (no -Zthreads — the
  # re-entrant subprocess lock model conflicts with parallel frontend threads).
  # No -fuse-ld flag: the linker is wild, managed via languages.rust.wild.enable.
  env.RUSTFLAGS_FIX = lib.mkForce (lib.concatStringsSep " " [
    "-C lto=off"
    "-C codegen-units=256"
    "-C opt-level=1"
    "-Zshare-generics=y"
    "-C target-cpu=native"
  ]);

  # ── Utility scripts ───────────────────────────────────────────────
  scripts = {
    cargo-fix.exec = ''
      RUSTFLAGS="$RUSTFLAGS_FIX" CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="${config.env.DEVENV_ROOT}/target-fix" cargo fix --allow-dirty "$@"
    '';
    cargo-clippy-fix.exec = ''
      RUSTFLAGS="$RUSTFLAGS_FIX" CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="${config.env.DEVENV_ROOT}/target-fix" cargo clippy --fix --allow-dirty "$@"
    '';
  };

  # ── Pre-commit hooks ──────────────────────────────────────────────
  pre-commit.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
    cargo-deny = {
      enable = true;
      entry = "${pkgs.cargo-deny}/bin/cargo-deny check";
    };
  };

  enterShell = ''
    export CARGO_BUILD_JOBS=$(($(nproc) - 1))
    mkdir -p "${config.env.DEVENV_STATE}/coverage/profiles"
    mkdir -p "${config.env.DEVENV_STATE}/bolero-corpus"

    echo "🦀 fast-observe Rust dev environment ready"
  '';
}
