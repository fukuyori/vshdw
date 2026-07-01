#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/notarize-executable.sh
  scripts/notarize-executable.sh --sign --identity "Developer ID Application: Name (TEAMID)"

Before first use, store notary credentials once:
  xcrun notarytool store-credentials "notarytool" \
    --apple-id "fukuyori.n@me.com" \
    --team-id "Q6GG27UYG5"

Options:
  --profile NAME     notarytool keychain profile. Defaults to NOTARY_PROFILE or notarytool.
  --binary PATH      executable to notarize. Defaults to target/release/vshdw.
  --output PATH      archive to submit. Defaults to dist/vshdw-macos-notary.zip.
  --sign             sign the executable before archiving.
  --identity ID      codesign identity used with --sign. Defaults to CODESIGN_IDENTITY.
  --team-id ID       team ID used to auto-detect Developer ID. Defaults to CODESIGN_TEAM_ID or Q6GG27UYG5.
  --build            build release executable before signing/notarizing.
  --force            replace an existing signature when used with --sign.
  --keep-archive     keep the generated zip after successful submission.
  -h, --help         show this help.

Notes:
  notarytool accepts a zip archive, but stapler cannot staple a ticket to a plain
  Mach-O executable or zip. For offline Gatekeeper validation, distribute a
  signed pkg or dmg and staple that container.
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_root/target/release/vshdw"
output="$repo_root/dist/vshdw-macos-notary.zip"
profile="${NOTARY_PROFILE:-notarytool}"
sign=false
identity="${CODESIGN_IDENTITY:-}"
team_id="${CODESIGN_TEAM_ID:-Q6GG27UYG5}"
build=false
force=false
keep_archive=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      [[ $# -ge 2 ]] || { echo "error: --profile requires a value" >&2; exit 2; }
      profile="$2"
      shift 2
      ;;
    --binary)
      [[ $# -ge 2 ]] || { echo "error: --binary requires a value" >&2; exit 2; }
      binary="$2"
      shift 2
      ;;
    --output)
      [[ $# -ge 2 ]] || { echo "error: --output requires a value" >&2; exit 2; }
      output="$2"
      shift 2
      ;;
    --sign)
      sign=true
      shift
      ;;
    --identity)
      [[ $# -ge 2 ]] || { echo "error: --identity requires a value" >&2; exit 2; }
      identity="$2"
      shift 2
      ;;
    --team-id)
      [[ $# -ge 2 ]] || { echo "error: --team-id requires a value" >&2; exit 2; }
      team_id="$2"
      shift 2
      ;;
    --build)
      build=true
      shift
      ;;
    --force)
      force=true
      shift
      ;;
    --keep-archive)
      keep_archive=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: notarization must run on macOS with Xcode command line tools" >&2
  exit 1
fi

command -v xcrun >/dev/null 2>&1 || {
  echo "error: xcrun was not found" >&2
  exit 1
}

if [[ "$build" == true ]]; then
  (cd "$repo_root" && cargo build --release)
fi

if [[ ! -f "$binary" ]]; then
  echo "error: executable not found: $binary" >&2
  echo "hint: run with --build or pass --binary PATH" >&2
  exit 1
fi

if [[ ! -x "$binary" ]]; then
  echo "error: path is not executable: $binary" >&2
  exit 1
fi

if [[ "$sign" == true ]]; then
  sign_args=(--binary "$binary" --team-id "$team_id")
  if [[ -n "$identity" ]]; then
    sign_args+=(--identity "$identity")
  fi
  if [[ "$force" == true ]]; then
    sign_args+=(--force)
  fi
  "$repo_root/scripts/sign-executable.sh" "${sign_args[@]}"
fi

codesign --verify --strict --verbose=2 "$binary"
signature_info="$(codesign --display --verbose=4 "$binary" 2>&1)"

if grep -q '^Signature=adhoc$' <<<"$signature_info"; then
  echo "error: executable is ad-hoc signed; notarization requires a Developer ID Application certificate" >&2
  echo "hint: rerun with --sign --identity \"Developer ID Application: ...\"" >&2
  exit 1
fi

if ! grep -q '^Authority=Developer ID Application:' <<<"$signature_info"; then
  echo "error: executable is not signed with a Developer ID Application certificate" >&2
  echo "hint: list available identities with: security find-identity -v -p codesigning" >&2
  exit 1
fi

if ! grep -q '^Timestamp=' <<<"$signature_info"; then
  echo "error: executable signature does not include a secure timestamp" >&2
  echo "hint: sign without --no-timestamp" >&2
  exit 1
fi

if ! grep -Eq 'flags=.*\(.*runtime.*\)|^Runtime Version=' <<<"$signature_info"; then
  echo "error: executable signature does not have the hardened runtime enabled" >&2
  echo "hint: sign without --no-runtime" >&2
  exit 1
fi

mkdir -p "$(dirname "$output")"
rm -f "$output"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

stage_dir="$tmp_dir/vshdw-notary"
mkdir -p "$stage_dir"
cp "$binary" "$stage_dir/$(basename "$binary")"
(cd "$stage_dir" && ditto -c -k --keepParent "$(basename "$binary")" "$output")

xcrun notarytool submit "$output" \
  --keychain-profile "$profile" \
  --wait

if [[ "$keep_archive" == true ]]; then
  echo "notarized archive: $output"
else
  rm -f "$output"
fi
