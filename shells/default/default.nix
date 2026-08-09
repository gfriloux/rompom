{
  pkgs,
  mkShell,
  ...
}:
mkShell {
  packages = with pkgs; [
    alejandra
    deadnix
    statix
    pre-commit
    cargo
    rustc
    clippy
    rustfmt
    rust-analyzer
    just
    git-cliff # changelog generation (`just changelog`)
    cargo-audit # CVE scan (`just audit`)
  ];

  shellHook = ''
    echo "[rompom] Ready. Gates: just ci — changelog: just changelog"

    # .pre-commit-config.yaml is committed (it calls the Justfile recipes), so there is
    # nothing to generate here — only the local git hook needs installing.
    if [ -d .git ] && [ ! -f .git/hooks/pre-commit ]; then
      echo "Installing pre-commit hook..."
      pre-commit install -f --install-hooks
    fi
  '';
}
