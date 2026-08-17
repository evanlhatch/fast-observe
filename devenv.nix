{ pkgs, config, inputs, lib, ... }:
{
  imports = [
    ./devenv/lang/rust.nix
  ];

  # ── System packages ─────────────────────────────────────────────
  packages = with pkgs; [
    jujutsu
    just
    ripgrep
    difftastic
  ];

  enterShell = ''
    echo "fast-observe devenv ready"
  '';
}
