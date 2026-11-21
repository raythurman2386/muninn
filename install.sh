#!/bin/sh
# install.sh - POSIX sh installer for Muninn release tarballs.
#
# Supported: Linux x86_64/aarch64 and macOS aarch64.
# Never runs sudo and never writes to ~/.config/muninn, app state or a vault.
#
# MUNINN_INSTALL_BASE_URL is a test-only override for the download root
# (default: https://github.com/raythurman2386/muninn/releases/download).

set -eu

REPO="raythurman2386/muninn"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
DOWNLOAD_BASE="${MUNINN_INSTALL_BASE_URL:-https://github.com/${REPO}/releases/download}"

TAG=""
PREFIX=""
UNINSTALL=false
REQUIRE_SIGNATURE=false
NO_VERIFY=false
OS_NAME=""
PLATFORM=""
TMP=""

die() { printf 'install.sh: error: %s\n' "$*" >&2; exit 1; }
warn() { printf 'install.sh: warning: %s\n' "$*" >&2; }

usage() {
  cat <<EOF
Usage: sh install.sh [options]

Install a prebuilt Muninn release into a prefix (default: \$HOME/.local).

Options:
  --version TAG       Install a specific release tag (default: latest stable).
  --prefix DIR        Install into DIR (default: \$HOME/.local).
  --uninstall         Remove only the files listed in the install receipt under DIR.
  --require-signature Fail unless cosign or gh is present and verification succeeds.
  --no-verify         Skip checksum and signature verification (explicit opt-out).
  -h, --help          Show this help and exit.

Examples:
  sh install.sh
  sh install.sh --version v0.9.0 --prefix /tmp/muninn
  sh install.sh --prefix /tmp/muninn --uninstall

The script never runs sudo. If the prefix is not writable it prints the
command to re-run yourself. Uninstall never touches ~/.config/muninn,
sync state or a vault.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || die "--version requires a TAG"
      TAG=$2
      shift 2
      ;;
    --version=*)
      TAG=${1#--version=}
      shift
      ;;
    --prefix)
      [ "$#" -ge 2 ] || die "--prefix requires a DIR"
      PREFIX=$2
      shift 2
      ;;
    --prefix=*)
      PREFIX=${1#--prefix=}
      shift
      ;;
    --uninstall)
      UNINSTALL=true
      shift
      ;;
    --require-signature)
      REQUIRE_SIGNATURE=true
      shift
      ;;
    --no-verify)
      NO_VERIFY=true
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      die "unknown option '$1' (try --help)"
      ;;
    *)
      die "unexpected argument '$1' (try --help)"
      ;;
  esac
done

if [ "$NO_VERIFY" = true ] && [ "$REQUIRE_SIGNATURE" = true ]; then
  die "--no-verify and --require-signature cannot be combined"
fi

if [ -z "$PREFIX" ]; then
  if [ -z "${HOME:-}" ]; then
    die "HOME is not set; pass --prefix explicitly"
  fi
  PREFIX="${HOME}/.local"
fi
case "$PREFIX" in
  /*) ;;
  *) PREFIX="${PWD}/${PREFIX}" ;;
esac

SCRIPT_PATH=$0
case "$SCRIPT_PATH" in
  /*) ;;
  *) SCRIPT_PATH="${PWD}/${SCRIPT_PATH}" ;;
esac

TMP=$(mktemp -d "${TMPDIR:-/tmp}/muninn-install.XXXXXX") || die "cannot create a temporary directory"
cleanup() {
  if [ -n "$TMP" ] && [ -d "$TMP" ]; then
    rm -rf "$TMP"
  fi
}
trap cleanup EXIT HUP INT TERM

# Print a shell-quoted argument (POSIX sh has no arrays, so build commands as strings).
shquote() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

check_prefix_writable() {
  probe=$PREFIX
  while [ ! -e "$probe" ] && [ "$probe" != "/" ]; do
    probe=$(dirname "$probe")
  done
  [ -d "$probe" ] && [ -w "$probe" ]
}

sudo_command() {
  cmd="sudo sh $(shquote "$SCRIPT_PATH") --prefix $(shquote "$PREFIX")"
  if [ -n "$TAG" ]; then
    cmd="$cmd --version $(shquote "$TAG")"
  fi
  if [ "$UNINSTALL" = true ]; then
    cmd="$cmd --uninstall"
  fi
  if [ "$REQUIRE_SIGNATURE" = true ]; then
    cmd="$cmd --require-signature"
  fi
  if [ "$NO_VERIFY" = true ]; then
    cmd="$cmd --no-verify"
  fi
  printf '%s' "$cmd"
}

add_parent_dirs() {
  dir=$(dirname "$1")
  while [ "$dir" != "$PREFIX" ] && [ "$dir" != "/" ] && [ "$dir" != "." ]; do
    printf '%s\n' "$dir" >> "$TMP/uninstall-dirs"
    dir=$(dirname "$dir")
  done
}

do_uninstall() {
  receipt="$PREFIX/share/muninn/install-receipt"
  [ -f "$receipt" ] || die "no install receipt at $receipt; nothing to uninstall"

  : > "$TMP/uninstall-dirs"
  file=""
  while IFS= read -r file || [ -n "$file" ]; do
    [ -n "$file" ] || continue
    case "$file" in
      "$PREFIX"/*) ;;
      *)
        warn "skipping receipt entry outside the prefix: $file"
        continue
        ;;
    esac
    if [ -f "$file" ] || [ -L "$file" ]; then
      rm -f "$file" && printf 'removed %s\n' "$file"
    fi
    add_parent_dirs "$file"
  done < "$receipt"
  rm -f "$receipt"
  printf 'removed %s\n' "$receipt"
  add_parent_dirs "$receipt"

  # rmdir is safe: it only removes directories that are empty at this point.
  sort -u -r "$TMP/uninstall-dirs" > "$TMP/uninstall-dirs.sorted"
  while IFS= read -r dir; do
    rmdir "$dir" 2>/dev/null || :
  done < "$TMP/uninstall-dirs.sorted"

  printf 'Muninn uninstalled from %s\n' "$PREFIX"
  printf 'Your vault, ~/.config/muninn and app state were left untouched.\n'
}

detect_platform() {
  OS_NAME=$(uname -s)
  arch=$(uname -m)
  case "$OS_NAME" in
    Linux)
      case "$arch" in
        x86_64 | amd64) PLATFORM=linux_x86_64 ;;
        aarch64 | arm64) PLATFORM=linux_aarch64 ;;
        *)
          die "unsupported Linux architecture '$arch' (expected x86_64 or aarch64)"
          ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        arm64 | aarch64) PLATFORM=macos_aarch64 ;;
        *)
          die "unsupported macOS architecture '$arch' (only aarch64 is supported)"
          ;;
      esac
      ;;
    *)
      die "unsupported operating system '$OS_NAME' (expected Linux or Darwin)"
      ;;
  esac
}

resolve_tag() {
  if [ -n "$TAG" ]; then
    return 0
  fi
  response=$(curl -fsSL -H 'Accept: application/vnd.github+json' "$API_URL") ||
    die "could not query the GitHub releases API at $API_URL (check the network connection)"
  TAG=$(printf '%s\n' "$response" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$TAG" ] || die "could not read tag_name from the latest-release response"
  printf 'Latest release: %s\n' "$TAG"
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1; exit}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1; exit}'
  else
    die "no SHA-256 utility found (install sha256sum or shasum)"
  fi
}

verify_checksum() {
  archive=$1
  name=$(basename "$archive")
  actual=$(sha256_of "$archive")
  [ -n "$actual" ] || die "could not compute a SHA-256 for $name"
  # Match the exact archive line in checksums.txt (hash + basename, allowing
  # sha256sum's '*' binary marker and either hash tool's whitespace).
  if awk -v h="$actual" -v n="$name" '
    {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      split(line, fields, /[[:space:]]+/)
      if (fields[1] == h) {
        fname = fields[2]
        sub(/^\*/, "", fname)
        if (fname == n) { found = 1; exit }
      }
    }
    END { exit(found ? 0 : 1) }' "$CHECKSUMS"; then
    return 0
  fi
  return 1
}

verify_signature() {
  verifier=""
  if command -v cosign >/dev/null 2>&1; then
    verifier=cosign
  elif command -v gh >/dev/null 2>&1; then
    verifier=gh
  fi

  if [ "$REQUIRE_SIGNATURE" = true ] && [ -z "$verifier" ]; then
    die "--require-signature was requested, but neither cosign nor gh is installed (install one, or use --no-verify)"
  fi

  case "$verifier" in
    cosign)
      bundle="$TMP/checksums.txt.sigstore.json"
      curl -fsSL -o "$bundle" "$DOWNLOAD_BASE/$TAG/checksums.txt.sigstore.json" ||
        die "failed to download checksums.txt.sigstore.json from $DOWNLOAD_BASE/$TAG/"
      cosign verify-blob \
        --bundle "$bundle" \
        --certificate-identity-regexp '^https://github.com/raythurman2386/muninn/.github/workflows/release.yml@refs/tags/' \
        --certificate-oidc-issuer https://token.actions.githubusercontent.com \
        "$CHECKSUMS" || die "cosign could not verify checksums.txt"
      printf 'Sigstore signature verified with cosign.\n'
      ;;
    gh)
      gh attestation verify "$ARCHIVE" --repo "$REPO" ||
        die "gh attestation verify failed for $ARCHIVE_NAME"
      printf 'GitHub attestation verified with gh.\n'
      ;;
    *)
      warn "signature was NOT checked: no cosign or gh was found. The checksum was verified; install cosign or gh for Sigstore verification, or pass --require-signature to demand a verifier."
      ;;
  esac
}

# --- uninstall ---------------------------------------------------------------
if [ "$UNINSTALL" = true ]; then
  if ! check_prefix_writable; then
    die "prefix $PREFIX is not writable; re-run the uninstall yourself: $(sudo_command)"
  fi
  do_uninstall
  exit 0
fi

# --- install -----------------------------------------------------------------
command -v curl >/dev/null 2>&1 || die "curl is required to download a release"
command -v tar >/dev/null 2>&1 || die "tar is required to unpack a release"

detect_platform
resolve_tag

if ! check_prefix_writable; then
  die "prefix $PREFIX is not writable; re-run with elevated privileges if appropriate: $(sudo_command)"
fi

ARCHIVE_NAME="muninn_${TAG}_${PLATFORM}.tar.gz"
ARCHIVE="$TMP/$ARCHIVE_NAME"
CHECKSUMS="$TMP/checksums.txt"

printf 'Downloading %s ...\n' "$ARCHIVE_NAME"
curl -fsSL -o "$CHECKSUMS" "$DOWNLOAD_BASE/$TAG/checksums.txt" ||
  die "failed to download checksums.txt from $DOWNLOAD_BASE/$TAG/"
curl -fsSL -o "$ARCHIVE" "$DOWNLOAD_BASE/$TAG/$ARCHIVE_NAME" ||
  die "failed to download $ARCHIVE_NAME from $DOWNLOAD_BASE/$TAG/"

if [ "$NO_VERIFY" = true ]; then
  warn "verification skipped (--no-verify): neither the checksum nor the signature was checked"
else
  verify_checksum "$ARCHIVE" ||
    die "SHA-256 mismatch for $ARCHIVE_NAME; refusing to install (see $DOWNLOAD_BASE/$TAG/checksums.txt)"
  printf 'SHA-256 checksum verified.\n'
  verify_signature
fi

EXTRACT="$TMP/extract"
mkdir -p "$EXTRACT"
tar -xzf "$ARCHIVE" -C "$EXTRACT" || die "failed to extract $ARCHIVE_NAME"

BIN_SRC=$(find "$EXTRACT" -type f -name muninn | head -n 1)
[ -n "$BIN_SRC" ] || die "$ARCHIVE_NAME does not contain a 'muninn' binary"

RECEIPT_NEW="$TMP/receipt.new"
: > "$RECEIPT_NEW"

mkdir -p "$PREFIX/bin"
cp "$BIN_SRC" "$PREFIX/bin/muninn"
chmod 755 "$PREFIX/bin/muninn"
printf '%s\n' "$PREFIX/bin/muninn" >> "$RECEIPT_NEW"

if [ "$OS_NAME" = "Linux" ]; then
  desktop_src=$(find "$EXTRACT" -type f -name muninn.desktop | head -n 1)
  if [ -n "$desktop_src" ]; then
    mkdir -p "$PREFIX/share/applications"
    cp "$desktop_src" "$PREFIX/share/applications/muninn.desktop"
    printf '%s\n' "$PREFIX/share/applications/muninn.desktop" >> "$RECEIPT_NEW"
  fi

  png_src=$(find "$EXTRACT" -type f -name logo.png | head -n 1)
  if [ -n "$png_src" ]; then
    mkdir -p "$PREFIX/share/icons/hicolor/512x512/apps"
    cp "$png_src" "$PREFIX/share/icons/hicolor/512x512/apps/muninn.png"
    printf '%s\n' "$PREFIX/share/icons/hicolor/512x512/apps/muninn.png" >> "$RECEIPT_NEW"
  fi

  svg_src=$(find "$EXTRACT" -type f -name logo.svg | head -n 1)
  if [ -n "$svg_src" ]; then
    mkdir -p "$PREFIX/share/icons/hicolor/scalable/apps"
    cp "$svg_src" "$PREFIX/share/icons/hicolor/scalable/apps/muninn.svg"
    printf '%s\n' "$PREFIX/share/icons/hicolor/scalable/apps/muninn.svg" >> "$RECEIPT_NEW"
  fi
fi

mkdir -p "$PREFIX/share/muninn"
cp "$RECEIPT_NEW" "$PREFIX/share/muninn/install-receipt"
chmod 644 "$PREFIX/share/muninn/install-receipt"

printf 'Installed Muninn %s into %s\n' "$TAG" "$PREFIX"
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) printf 'Add %s/bin to your PATH to run muninn from anywhere.\n' "$PREFIX" ;;
esac
