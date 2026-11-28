#!/usr/bin/env bash
# bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
#
# One release has to agree in Cargo.toml and default.nix.  Keep that agreement
# checkable, and keep the tag, tarball, checksum and GitHub release in one
# reproducible path.
set -euo pipefail
unset CDPATH

cd "$(dirname "${BASH_SOURCE[0]}")/.."
root=$PWD

usage() {
  cat <<'EOF'
Usage:
  scripts/release.sh check [VERSION]
  scripts/release.sh set-version VERSION
  scripts/release.sh verify
  scripts/release.sh publish

check       Verify that Cargo.toml and default.nix agree on VERSION.
set-version Update both manifests to VERSION.
verify      Run the Rust tests, the install smoke test, and the Nix and Guix
            packages when those tools are installed.
publish     Verify, create and push the tag, build a tarball/checksum, and
            create the GitHub release.  BIBELOT_RELEASE_NOTES may point at a
            Markdown file; otherwise GitHub generates the notes.

The worktree must be clean before check/publish.  publish pushes the current
branch and the new tag, so it is intentionally a separate explicit command.
EOF
}

die() {
  printf 'release: %s\n' "$*" >&2
  exit 1
}

cargo_version() {
  sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1
}

nix_version() {
  sed -n 's/^  version = "\([^"]*\)";/\1/p' default.nix | head -n 1
}

check_version() {
  local expected=$1
  local actual

  [[ "$expected" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]] ||
    die "VERSION must look like 0.1.0 (got '$expected')"

  actual=$(cargo_version)
  [[ "$actual" == "$expected" ]] ||
    die "Cargo.toml is '$actual', expected '$expected'"

  actual=$(nix_version)
  [[ "$actual" == "$expected" ]] ||
    die "default.nix is '$actual', expected '$expected'"
}

current_version() {
  local version
  version=$(cargo_version)
  [[ -n "$version" ]] || die "could not read the version from Cargo.toml"
  printf '%s\n' "$version"
}

check_clean() {
  [[ -z "$(git status --porcelain)" ]] ||
    die "the worktree is not clean; commit the release prep first"
}

check_tag_available() {
  local version=$1

  ! git rev-parse -q --verify "refs/tags/v${version}" >/dev/null ||
    die "tag v${version} already exists"
}

check_release_available() {
  local version=$1

  if command -v gh >/dev/null 2>&1; then
    if gh release view "v${version}" >/dev/null 2>&1; then
      die "GitHub release v${version} already exists"
    fi
  fi
}

update_version() {
  local version=$1

  sed -i "0,/^version = \"[^\"]*\"/s//version = \"${version}\"/" Cargo.toml
  sed -i "s/^  version = \"[^\"]*\";/  version = \"${version}\";/" default.nix
}

verify() {
  local version
  version=$(current_version)
  check_version "$version"

  command -v cargo >/dev/null 2>&1 || die "cargo is not installed"

  cargo fmt --all --check
  cargo clippy --all-targets --locked -- -D warnings
  cargo test --locked

  local tmp
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/bibelot-smoke.XXXXXX")
  trap 'rm -rf "$tmp"' RETURN

  scripts/install.sh --prefix=/usr --destdir="$tmp/dest" --no-build
  [[ -f "$tmp/dest/usr/share/guile/site/3.0/bibelot/theme.scm" ]] ||
    die "the staged Scheme module is missing"

  if command -v nix-build >/dev/null 2>&1; then
    nix-build -E 'with import <nixpkgs> {}; callPackage ./default.nix {}' \
      --no-out-link
  else
    printf 'release: nix-build not found; skipping the Nix build\n' >&2
  fi

  if command -v guix >/dev/null 2>&1; then
    guix time-machine --channels=channels.scm -- build -f guix.scm
  else
    printf 'release: guix not found; skipping the Guix build\n' >&2
  fi
}

publish() {
  local version
  version=$(current_version)

  check_clean
  check_version "$version"
  check_tag_available "$version"
  check_release_available "$version"
  verify

  mkdir -p dist
  local tarball="dist/bibelot-${version}.tar.gz"
  git archive --format=tar.gz --prefix="bibelot-${version}/" \
    -o "$tarball" HEAD
  sha256sum "$tarball" >"${tarball}.sha256"

  git tag -a "v${version}" -m "bibelot ${version}"
  git push origin "$(git branch --show-current)"
  git push origin "v${version}"

  if [[ -n "${BIBELOT_RELEASE_NOTES:-}" ]]; then
    [[ -f "$BIBELOT_RELEASE_NOTES" ]] ||
      die "BIBELOT_RELEASE_NOTES does not exist: $BIBELOT_RELEASE_NOTES"
    gh release create "v${version}" --title "bibelot ${version}" \
      --notes-file "$BIBELOT_RELEASE_NOTES" \
      "$tarball" "${tarball}.sha256"
  else
    gh release create "v${version}" --title "bibelot ${version}" \
      --generate-notes "$tarball" "${tarball}.sha256"
  fi
}

command=${1:-}
case "$command" in
  check)
    version=${2:-$(current_version)}
    check_version "$version"
    printf 'release: all manifests agree on %s\n' "$version"
    ;;
  set-version)
    [[ $# -eq 2 ]] || {
      usage
      exit 2
    }
    update_version "$2"
    check_version "$2"
    printf 'release: manifests updated to %s; review and commit them\n' "$2"
    ;;
  verify)
    verify
    ;;
  publish)
    publish
    ;;
  *)
    usage
    exit 2
    ;;
esac
