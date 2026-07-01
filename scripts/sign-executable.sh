#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/sign-executable.sh --identity "Developer ID Application: Name (TEAMID)"
  CODESIGN_IDENTITY="Developer ID Application: Name (TEAMID)" scripts/sign-executable.sh

Options:
  --identity ID       codesign identity. Defaults to CODESIGN_IDENTITY.
  --team-id ID        team ID used to auto-detect Developer ID. Defaults to CODESIGN_TEAM_ID or Q6GG27UYG5.
  --binary PATH      executable to sign. Defaults to target/release/vshdw.
  --build            run cargo build --release before signing.
  --adhoc            use ad-hoc signing when no identity is available.
  --runtime          enable the hardened runtime. Default when using a real identity.
  --no-runtime       disable the hardened runtime.
  --timestamp        request a trusted timestamp. Default when using a real identity.
  --no-timestamp     disable trusted timestamp.
  --force            replace an existing signature.
  --verify           verify the signature after signing. Enabled by default.
  --no-verify        skip signature verification.
  -h, --help         show this help.

Examples:
  scripts/sign-executable.sh --build --identity "Developer ID Application: Example Inc. (ABCDE12345)"
  scripts/sign-executable.sh --binary ./dist/vshdw --adhoc
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_root/target/release/vshdw"
identity="${CODESIGN_IDENTITY:-}"
team_id="${CODESIGN_TEAM_ID:-Q6GG27UYG5}"
build=false
adhoc=false
runtime=true
timestamp=""
force=false
verify=true

while [[ $# -gt 0 ]]; do
  case "$1" in
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
    --binary)
      [[ $# -ge 2 ]] || { echo "error: --binary requires a value" >&2; exit 2; }
      binary="$2"
      shift 2
      ;;
    --build)
      build=true
      shift
      ;;
    --adhoc)
      adhoc=true
      shift
      ;;
    --runtime)
      runtime=true
      shift
      ;;
    --no-runtime)
      runtime=false
      shift
      ;;
    --timestamp)
      timestamp="--timestamp"
      shift
      ;;
    --no-timestamp)
      timestamp="--timestamp=none"
      shift
      ;;
    --force)
      force=true
      shift
      ;;
    --verify)
      verify=true
      shift
      ;;
    --no-verify)
      verify=false
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
  echo "error: this script signs macOS executables with codesign and must run on macOS" >&2
  exit 1
fi

command -v codesign >/dev/null 2>&1 || {
  echo "error: codesign was not found" >&2
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

if [[ -z "$identity" ]]; then
  detected_identity="$(
    security find-identity -v -p codesigning 2>/dev/null \
      | sed -nE 's/^[[:space:]]*[0-9]+\)[[:space:]]+[A-F0-9]+[[:space:]]+"(Developer ID Application: .* \('"$team_id"'\))"$/\1/p' \
      | head -n 1
  )"
  if [[ -n "$detected_identity" ]]; then
    identity="$detected_identity"
  elif [[ "$adhoc" == true ]]; then
    identity="-"
  else
    echo "error: no signing identity provided" >&2
    echo "hint: install a Developer ID Application certificate for team $team_id, set CODESIGN_IDENTITY, pass --identity, or use --adhoc" >&2
    echo "hint: list available identities with: security find-identity -v -p codesigning" >&2
    exit 1
  fi
fi

codesign_args=(--sign "$identity")

if [[ "$force" == true ]]; then
  codesign_args+=(--force)
fi

if [[ "$identity" != "-" && "$runtime" == true ]]; then
  codesign_args+=(--options runtime)
fi

if [[ -n "$timestamp" ]]; then
  codesign_args+=("$timestamp")
elif [[ "$identity" != "-" ]]; then
  codesign_args+=(--timestamp)
fi

codesign_args+=("$binary")

codesign "${codesign_args[@]}"

if [[ "$verify" == true ]]; then
  codesign --verify --strict --verbose=2 "$binary"
fi

codesign --display --verbose=2 "$binary"
