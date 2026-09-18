#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/scripts/build_linux_appimage.sh"
VERIFY="$ROOT/scripts/verify_sha256.sh"
# shellcheck disable=SC1091
source "$ROOT/scripts/appimagetool_pin.sh"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

PINNED_TOOL="$TMP_DIR/pinned-appimagetool"
curl --connect-timeout 30 --max-time 300 -fsSL \
  "$APPIMAGETOOL_PINNED_URL" \
  -o "$PINNED_TOOL"
"$VERIFY" "$PINNED_TOOL" "$APPIMAGETOOL_PINNED_SHA256"

STUB_BIN="$TMP_DIR/bin"
mkdir -p "$STUB_BIN"

cat > "$STUB_BIN/cargo" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

cat > "$STUB_BIN/install" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
destination="${!#}"
mkdir -p "$(dirname "$destination")"
: > "$destination"
chmod 0755 "$destination"
EOF

cat > "$STUB_BIN/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
output=""
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "-o" ]]; then
    output="$2"
    shift 2
  else
    shift
  fi
done
[[ -n "$output" ]]
printf 'called\n' > "$APPIMAGETOOL_CURL_MARKER"
cp "$APPIMAGETOOL_FIXTURE" "$output"
EOF

chmod 0755 "$STUB_BIN/cargo" "$STUB_BIN/install" "$STUB_BIN/curl"

TRUSTED_TOOL="$TMP_DIR/trusted-tool"
cat > "$TRUSTED_TOOL" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'ran\n' > "$APPIMAGETOOL_RUN_MARKER"
compression=""
positionals=()
while (( $# )); do
  case "$1" in
    --comp)
      [[ $# -ge 2 ]] || exit 2
      compression="$2"
      shift 2
      ;;
    --) shift; positionals+=("$@"); break ;;
    -*) echo "Unexpected appimagetool option: $1" >&2; exit 2 ;;
    *) positionals+=("$1"); shift ;;
  esac
done
[[ "$compression" == "xz" ]]
[[ ${#positionals[@]} -eq 2 ]]
[[ -d "${positionals[0]}" ]]
: > "${positionals[1]}"
EOF
chmod 0755 "$TRUSTED_TOOL"
TRUSTED_SHA256="$(hash_file "$TRUSTED_TOOL")"

CORRUPT_TOOL="$TMP_DIR/corrupt-tool"
printf 'corrupt\n' > "$CORRUPT_TOOL"

run_build() {
  local scenario="$1"
  local fixture="$2"
  local tool="$scenario/appimagetool"
  local output="$scenario/entropy.AppImage"

  mkdir -p "$scenario"
  PATH="$STUB_BIN:$PATH" \
    APPDIR="$scenario/Entropy.AppDir" \
    APPIMAGETOOL="$tool" \
    APPIMAGETOOL_URL="https://example.invalid/appimagetool" \
    APPIMAGETOOL_SHA256="$TRUSTED_SHA256" \
    APPIMAGETOOL_FIXTURE="$fixture" \
    APPIMAGETOOL_CURL_MARKER="$scenario/curl-called" \
    APPIMAGETOOL_RUN_MARKER="$scenario/tool-ran" \
    "$BUILD" vtest "$output"
}

FRESH_VALID="$TMP_DIR/fresh-valid"
run_build "$FRESH_VALID" "$TRUSTED_TOOL"
[[ -x "$FRESH_VALID/appimagetool" ]]
[[ -f "$FRESH_VALID/curl-called" ]]
[[ -f "$FRESH_VALID/tool-ran" ]]
[[ -f "$FRESH_VALID/entropy.AppImage" ]]

FRESH_CORRUPT="$TMP_DIR/fresh-corrupt"
mkdir -p "$FRESH_CORRUPT"
if run_build "$FRESH_CORRUPT" "$CORRUPT_TOOL" 2> "$FRESH_CORRUPT/stderr"; then
  echo "Expected a corrupt download to fail" >&2
  exit 1
fi
[[ "$(<"$FRESH_CORRUPT/stderr")" == *"SHA-256 mismatch"* ]]
[[ ! -e "$FRESH_CORRUPT/appimagetool" ]]
[[ ! -e "$FRESH_CORRUPT/tool-ran" ]]
if compgen -G "$FRESH_CORRUPT/appimagetool.download.*" >/dev/null; then
  echo "Corrupt download temporary file was not cleaned up" >&2
  exit 1
fi

CACHED_VALID="$TMP_DIR/cached-valid"
mkdir -p "$CACHED_VALID"
cp "$TRUSTED_TOOL" "$CACHED_VALID/appimagetool"
run_build "$CACHED_VALID" "$CORRUPT_TOOL"
[[ ! -e "$CACHED_VALID/curl-called" ]]
[[ -f "$CACHED_VALID/tool-ran" ]]
[[ -f "$CACHED_VALID/entropy.AppImage" ]]

CACHED_CORRUPT="$TMP_DIR/cached-corrupt"
mkdir -p "$CACHED_CORRUPT"
cp "$CORRUPT_TOOL" "$CACHED_CORRUPT/appimagetool"
chmod 0755 "$CACHED_CORRUPT/appimagetool"
if run_build "$CACHED_CORRUPT" "$TRUSTED_TOOL" 2> "$CACHED_CORRUPT/stderr"; then
  echo "Expected a corrupt cached tool to fail" >&2
  exit 1
fi
[[ "$(<"$CACHED_CORRUPT/stderr")" == *"SHA-256 mismatch"* ]]
[[ ! -e "$CACHED_CORRUPT/curl-called" ]]
[[ ! -e "$CACHED_CORRUPT/tool-ran" ]]

# Пути, по которым сборка делает rm -rf, приезжают снаружи: всё, что после
# канонизации уходит из сборочных каталогов, должно отвергаться до сборки.
# Собственный TMPDIR у сценариев для того, чтобы «разрешённый временный
# каталог» и «чужой каталог» различались внутри одного /tmp.
SANDBOX="$TMP_DIR/paths"
ALLOWED="$SANDBOX/allowed"
OUTSIDE="$SANDBOX/outside"
mkdir -p "$ALLOWED" "$OUTSIDE/appdir"
: > "$OUTSIDE/keep"
: > "$OUTSIDE/appdir/keep"
ln -s "$OUTSIDE" "$ALLOWED/escape"

reject_path() {
  local label="$1" appdir="$2" out="$3" tool="${4:-$ALLOWED/appimagetool}"
  local stderr="$SANDBOX/stderr"

  if PATH="$STUB_BIN:$PATH" \
    TMPDIR="$ALLOWED" \
    DIST="" \
    APPDIR="$appdir" \
    APPIMAGETOOL="$tool" \
    APPIMAGETOOL_URL="https://example.invalid/appimagetool" \
    APPIMAGETOOL_SHA256="$TRUSTED_SHA256" \
    APPIMAGETOOL_FIXTURE="$TRUSTED_TOOL" \
    APPIMAGETOOL_CURL_MARKER="$SANDBOX/curl-called" \
    APPIMAGETOOL_RUN_MARKER="$SANDBOX/tool-ran" \
    "$BUILD" vtest "$out" 2> "$stderr"; then
    echo "Expected $label to be rejected" >&2
    exit 1
  fi
  if ! grep -q "outside" "$stderr"; then
    cat "$stderr" >&2
    echo "Expected a path validation error for $label" >&2
    exit 1
  fi
  if [[ ! -f "$OUTSIDE/keep" || ! -f "$OUTSIDE/appdir/keep" ]]; then
    echo "$label removed files outside the build directories" >&2
    exit 1
  fi
}

VALID_APPDIR="$ALLOWED/Entropy.AppDir"
VALID_OUT="$ALLOWED/entropy.AppImage"
reject_path "an absolute APPDIR outside the build directories" "$OUTSIDE/appdir" "$VALID_OUT"
reject_path "a parent-traversing APPDIR" "$ALLOWED/../outside/appdir" "$VALID_OUT"
reject_path "an APPDIR escaping through a symlink" "$ALLOWED/escape/appdir" "$VALID_OUT"
reject_path "the repository root as APPDIR" "$ROOT" "$VALID_OUT"
reject_path "the build root itself as APPDIR" "$ROOT/target" "$VALID_OUT"
reject_path "an output outside the build directories" "$VALID_APPDIR" "$OUTSIDE/entropy.AppImage"
reject_path "the repository root as output" "$VALID_APPDIR" "$ROOT"
reject_path "a tool download outside the build directories" "$VALID_APPDIR" "$VALID_OUT" "$OUTSIDE/appimagetool"
[[ ! -e "$OUTSIDE/appimagetool" ]]

echo "AppImage tool integration tests passed"
