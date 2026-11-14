#!/usr/bin/env bash
# whelk -- a Wayland shell written in C, GTK4 and GNU Guile Scheme
#
# SPDX-License-Identifier: MIT
# Copyright © Josep Bigorra <jjbigorra@gmail.com>
#
# One release has to agree in six places.  Keep that agreement checkable, and
# keep the tag, tarball, checksum and GitHub release in one reproducible path.
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

check       Verify that every manifest and NEWS.org agree on VERSION.
set-version Update the manifests to VERSION; add its NEWS.org section first.
verify      Run the host tests, Guix build, and release smoke install.
publish     Verify, create and push the tag, build a tarball/checksum, and
            create the GitHub release.  WHELK_RELEASE_NOTES may point at a
            Markdown file; otherwise GitHub generates the notes.

The worktree must be clean before check/publish.  publish pushes the current
branch and the new tag, so it is intentionally a separate explicit command.
EOF
}

die() {
  printf 'release: %s\n' "$*" >&2
  exit 1
}

meson_version() {
  sed -n "s/^  version: '\([^']*\)',/\1/p" meson.build | head -n 1
}

nix_version() {
  sed -n 's/^  version = "\([^"]*\)";/\1/p' default.nix | head -n 1
}

arch_version() {
  sed -n 's/^pkgver=//p' packaging/arch/PKGBUILD | head -n 1
}

debian_version() {
  head -n 1 packaging/debian/changelog |
    sed -n 's/^whelk (\([^)]*\)-1).*/\1/p'
}

spec_version() {
  sed -n 's/^Version: *//p' packaging/opensuse/whelk.spec | head -n 1
}

check_version() {
  local expected="$1"

  [[ "$expected" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]] ||
    die "VERSION must look like 0.21.0 (got '$expected')"

  local name
  local value
  for name in meson_version nix_version arch_version debian_version \
              spec_version; do
    value=$("$name")
    [[ "$value" == "$expected" ]] ||
      die "$name is '$value', expected '$expected'"
  done

  grep -q "^\*\* v${expected} (" NEWS.org ||
    die "NEWS.org has no '** v${expected} (YYYY-MM-DD)' section"
}

current_version() {
  local version
  version=$(meson_version)
  [[ -n "$version" ]] || die "could not read the version from meson.build"
  printf '%s\n' "$version"
}

check_clean() {
  [[ -z "$(git status --porcelain)" ]] ||
    die "the worktree is not clean; commit the release prep first"
}

check_tag_available() {
  local version="$1"

  ! git rev-parse -q --verify "refs/tags/v${version}" >/dev/null ||
    die "tag v${version} already exists"
}

check_release_available() {
  local version="$1"

  if command -v gh >/dev/null 2>&1; then
    if gh release view "v${version}" >/dev/null 2>&1; then
      die "GitHub release v${version} already exists"
    fi
  fi
}

update_version() {
  local version="$1"

  sed -i "s/^  version: '[^']*',/  version: '${version}',/" meson.build
  sed -i "s/^  version = \"[^\"]*\";/  version = \"${version}\";/" default.nix
  sed -i "s/^pkgver=.*/pkgver=${version}/" packaging/arch/PKGBUILD
  sed -i "1s/^whelk ([^-)]*-[0-9][0-9]*)/whelk (${version}-1)/" \
    packaging/debian/changelog
  sed -i "s/^Version:.*/Version:        ${version}/" \
    packaging/opensuse/whelk.spec
}

verify() {
  local version
  version=$(current_version)
  check_version "$version"

  command -v meson >/dev/null 2>&1 || die "meson is not installed"
  command -v ninja >/dev/null 2>&1 || die "ninja is not installed"

  if [[ ! -d build-host ]]; then
    meson setup build-host
  fi
  ninja -C build-host
  meson test -C build-host --print-errorlogs

  if command -v guix >/dev/null 2>&1; then
    guix build -f guix.scm
  else
    printf 'release: guix not found; skipping the Guix build\n' >&2
  fi

  if command -v nix-build >/dev/null 2>&1; then
    nix-build -E 'with import <nixpkgs> {}; callPackage ./default.nix {}' \
      --no-out-link
  else
    printf 'release: nix-build not found; skipping the Nix build\n' >&2
  fi

  scripts/release-smoke.sh --source
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
  local tarball="dist/whelk-${version}.tar.gz"
  git archive --format=tar.gz --prefix="whelk-${version}/" \
    -o "$tarball" HEAD
  sha256sum "$tarball" >"${tarball}.sha256"

  git tag -a "v${version}" -m "whelk ${version}"
  git push origin "$(git branch --show-current)"
  git push origin "v${version}"

  if [[ -n "${WHELK_RELEASE_NOTES:-}" ]]; then
    [[ -f "$WHELK_RELEASE_NOTES" ]] ||
      die "WHELK_RELEASE_NOTES does not exist: $WHELK_RELEASE_NOTES"
    gh release create "v${version}" --title "whelk ${version}" \
      --notes-file "$WHELK_RELEASE_NOTES" \
      "$tarball" "${tarball}.sha256"
  else
    gh release create "v${version}" --title "whelk ${version}" \
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
    version=$2
    grep -q "^\*\* v${version} (" NEWS.org ||
      die "add the '** v${version} (YYYY-MM-DD)' NEWS.org section first"
    update_version "$version"
    check_version "$version"
    printf 'release: manifests updated to %s; review and commit them\n' "$version"
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
