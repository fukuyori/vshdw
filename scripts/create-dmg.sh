#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/create-dmg.sh
  scripts/create-dmg.sh --no-notarize

By default this builds, signs, notarizes, and staples: running with no options
produces a distributable, notarized dmg. Use the --no-* flags to skip steps.

Options:
  --binary PATH      executable to package. Defaults to target/release/vshdw.
  --output PATH      dmg to create. Defaults to dist/vshdw-<version>-macos_arm.dmg.
  --volume NAME      mounted volume name. Defaults to vshdw.
  --build            run cargo build --release before packaging. Enabled by default.
  --no-build         package the existing executable without rebuilding.
  --sign             sign the executable and dmg. Enabled by default.
  --no-sign          package without signing.
  --identity ID      codesign identity used when signing. Defaults to CODESIGN_IDENTITY.
  --team-id ID       team ID used to auto-detect Developer ID. Defaults to CODESIGN_TEAM_ID or Q6GG27UYG5.
  --force            replace an existing signature and output dmg. Enabled by default.
  --no-force         keep an existing signature and refuse to overwrite the output dmg.
  --notarize         submit the dmg to Apple notary service. Enabled by default.
  --no-notarize      skip notarization (and stapling).
  --profile NAME     notarytool keychain profile. Defaults to NOTARY_PROFILE or notarytool.
  --staple           staple the notary ticket to the dmg. Implied by --notarize.
  --no-staple        do not staple after notarization.
  -h, --help         show this help.

Examples:
  scripts/create-dmg.sh
  scripts/create-dmg.sh --no-notarize
  scripts/create-dmg.sh --no-build --no-sign --no-notarize
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(sed -nE 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$repo_root/Cargo.toml" | head -n 1)"
binary="$repo_root/target/release/vshdw"
output="$repo_root/dist/vshdw-${version}-macos_arm.dmg"
volume_name="vshdw"
identity="${CODESIGN_IDENTITY:-}"
team_id="${CODESIGN_TEAM_ID:-Q6GG27UYG5}"
profile="${NOTARY_PROFILE:-notarytool}"
build=true
sign=true
force=true
notarize=true
staple=false
staple_set=false

while [[ $# -gt 0 ]]; do
  case "$1" in
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
    --volume)
      [[ $# -ge 2 ]] || { echo "error: --volume requires a value" >&2; exit 2; }
      volume_name="$2"
      shift 2
      ;;
    --build)
      build=true
      shift
      ;;
    --no-build)
      build=false
      shift
      ;;
    --sign)
      sign=true
      shift
      ;;
    --no-sign)
      sign=false
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
    --force)
      force=true
      shift
      ;;
    --no-force)
      force=false
      shift
      ;;
    --notarize)
      notarize=true
      shift
      ;;
    --no-notarize)
      notarize=false
      shift
      ;;
    --profile)
      [[ $# -ge 2 ]] || { echo "error: --profile requires a value" >&2; exit 2; }
      profile="$2"
      shift 2
      ;;
    --staple)
      staple=true
      staple_set=true
      shift
      ;;
    --no-staple)
      staple=false
      staple_set=true
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
  echo "error: dmg creation must run on macOS" >&2
  exit 1
fi

for tool in hdiutil ditto codesign; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "error: $tool was not found" >&2
    exit 1
  }
done

if [[ "$notarize" == true ]]; then
  command -v xcrun >/dev/null 2>&1 || {
    echo "error: xcrun was not found" >&2
    exit 1
  }
  if [[ "$staple_set" == false ]]; then
    staple=true
  fi
fi

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

if [[ "$notarize" == true ]]; then
  if grep -q '^Signature=adhoc$' <<<"$signature_info"; then
    echo "error: executable is ad-hoc signed; notarization requires a Developer ID Application certificate" >&2
    echo "hint: rerun with --sign --identity \"Developer ID Application: ...\"" >&2
    exit 1
  fi

  if ! grep -q '^Authority=Developer ID Application:' <<<"$signature_info"; then
    echo "error: executable is not signed with a Developer ID Application certificate" >&2
    exit 1
  fi

  if ! grep -q '^Timestamp=' <<<"$signature_info"; then
    echo "error: executable signature does not include a secure timestamp" >&2
    exit 1
  fi

  if ! grep -Eq 'flags=.*\(.*runtime.*\)|^Runtime Version=' <<<"$signature_info"; then
    echo "error: executable signature does not have the hardened runtime enabled" >&2
    exit 1
  fi
fi

mkdir -p "$(dirname "$output")"

if [[ -e "$output" ]]; then
  if [[ "$force" == true ]]; then
    rm -f "$output"
  else
    echo "error: output already exists: $output" >&2
    echo "hint: pass --force to replace it" >&2
    exit 1
  fi
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

stage_dir="$tmp_dir/stage"
mkdir -p "$stage_dir"

ditto "$binary" "$stage_dir/$(basename "$binary")"

ln -s /Applications "$stage_dir/Applications"

cat > "$stage_dir/README.txt" <<'README'
vshdw is a command-line tool.

Drag vshdw onto the Applications folder in this window to install it, then run
it from Terminal:

  /Applications/vshdw --help

For convenience on the command line, copy vshdw to a directory on your PATH
instead, such as /usr/local/bin or ~/.local/bin, and run:

  vshdw --help
README

hdiutil create \
  -volname "$volume_name" \
  -srcfolder "$stage_dir" \
  -ov \
  -format UDZO \
  "$output"

hdiutil verify "$output"

if [[ "$sign" == true ]]; then
  dmg_identity="$identity"
  if [[ -z "$dmg_identity" ]]; then
    dmg_identity="$(
      security find-identity -v -p codesigning 2>/dev/null \
        | sed -nE 's/^[[:space:]]*[0-9]+\)[[:space:]]+[A-F0-9]+[[:space:]]+"(Developer ID Application: .* \('"$team_id"'\))"$/\1/p' \
        | head -n 1
    )"
  fi
  if [[ -n "$dmg_identity" && "$dmg_identity" != "-" ]]; then
    dmg_sign_args=(--sign "$dmg_identity" --timestamp)
    if [[ "$force" == true ]]; then
      dmg_sign_args+=(--force)
    fi
    codesign "${dmg_sign_args[@]}" "$output"
    codesign --verify --strict --verbose=2 "$output"
  fi
fi

if [[ "$notarize" == true ]]; then
  xcrun notarytool submit "$output" \
    --keychain-profile "$profile" \
    --wait

  if [[ "$staple" == true ]]; then
    xcrun stapler staple "$output"
    xcrun stapler validate "$output"
  fi
fi

echo "created dmg: $output"
