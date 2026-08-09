default:
    @just --list

# Single source of truth — pre-commit and the CI workflow both call these recipes.
# Full quality gate: version consistency + format + lint + test.
ci: version-check fmt-check lint test

# Format in place (tab_spaces=2, cf. CLAUDE.md "Code style").
fmt:
    cargo fmt -- --config tab_spaces=2

# Verify formatting without modifying — fails if anything is unformatted.
fmt-check:
    cargo fmt --check -- --config tab_spaces=2

# Static lint (clippy). No warning tolerated.
lint:
    cargo clippy --all-targets -- -D warnings

# Unit tests.
test:
    cargo test

# Debug build.
build:
    cargo build

# Static musl binary via Nix — same artifact the release workflow attaches.
build-static:
    nix build

# CVE scan of the dependency tree.
audit:
    cargo audit

# This drift already happened once (Cargo.toml 0.15.0 vs default.nix 0.13.0).
# Fail if the version drifts between Cargo.toml, Cargo.lock and the Nix package.
version-check:
    #!/usr/bin/env bash
    set -euo pipefail
    toml=$(sed -n 's/^version = "\([0-9.]*\)"/\1/p' Cargo.toml | head -1)
    lock=$(awk '/^name = "rompom"$/{getline; gsub(/[^0-9.]/,""); print; exit}' Cargo.lock)
    nixv=$(sed -n 's/.*version = "\([0-9.]*\)";/\1/p' packages/rompom/default.nix | head -1)
    echo "Cargo.toml=${toml}  Cargo.lock=${lock}  packages/rompom/default.nix=${nixv}"
    if [ "$toml" != "$lock" ] || [ "$toml" != "$nixv" ]; then
        echo "version mismatch — run 'just release ${toml}' to realign" >&2
        exit 1
    fi

# Entries for v0.15.0 and earlier live frozen in CHANGELOG-legacy.md, never regenerated.
# Regenerate CHANGELOG.md from the Conventional Commits. Review the diff before committing.
changelog:
    @git-cliff --output CHANGELOG.md

# Preview the entry the next release would publish (what lands in the GitHub release body).
changelog-preview:
    @git-cliff --unreleased --strip header

# Per the hybrid git policy the tag is set by the maintainer, never by this recipe.
# Bump the version everywhere and regenerate the changelog. Usage: just release 0.16.0
release VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    v="{{ VERSION }}"
    [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "expected X.Y.Z, got '${v}'" >&2; exit 1; }
    sed -i "0,/^version = \"[0-9.]*\"/s//version = \"${v}\"/" Cargo.toml
    sed -i "s/version = \"[0-9.]*\";/version = \"${v}\";/" packages/rompom/default.nix
    # Retarget the rompom entry in Cargo.lock directly: `cargo metadata --offline` cannot
    # be used here (it tries to fetch the Windows-only crates the lockfile also pins).
    sed -i '/^name = "rompom"$/{n;s/^version = ".*"$/version = "'"${v}"'"/}' Cargo.lock
    just version-check
    # --tag: the tag does not exist yet, so tell git-cliff which version the pending
    # commits belong to. Without it the section would be titled "[Unreleased]" and would
    # not match what `git-cliff --latest` publishes in the release once the tag is pushed.
    git-cliff --tag "v${v}" --output CHANGELOG.md
    cat <<EOF

    Version bumped to ${v} and CHANGELOG.md regenerated.
    Review the diff, then:

      git add -A && git commit -m 'chore(release): v${v}'
      # merge onto master, then:
      git tag -a v${v} -m 'v${v}' && git push origin v${v}

    Pushing the tag triggers .github/workflows/release.yml.
    EOF
