#!/usr/bin/env sh
set -e

REPO="DavidNzube101/beacon"
INSTALL_DIR="/usr/local/bin"

# Get binary name from first argument (default: beacon)
BINARY="${1:-beacon}"
# Get version from second argument (default: latest)
VERSION_ARG="${2:-latest}"

get_latest_version() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep '"tag_name"' \
        | sed 's/.*"tag_name": "\(.*\)".*/\1/'
}

# Resolve the actual version to download
if [ "$VERSION_ARG" = "latest" ]; then
    VERSION=$(get_latest_version)
else
    # Ensure version starts with 'v'
    case "$VERSION_ARG" in
        v*) VERSION="$VERSION_ARG" ;;
        *)  VERSION="v$VERSION_ARG" ;;
    esac
fi

get_target() {
    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)  echo "beacon-linux-x86_64" ;;
                aarch64) echo "beacon-linux-arm64" ;;
                *)       echo "Unsupported architecture: $ARCH" >&2 && exit 1 ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64) echo "beacon-macos-x86_64" ;;
                arm64)  echo "beacon-macos-arm64" ;;
                *)      echo "Unsupported architecture: $ARCH" >&2 && exit 1 ;;
            esac
            ;;
        *)
            echo "Unsupported OS: $OS" >&2
            exit 1
            ;;
    esac
}

# --- Rigorous Conflict Detection ---
# We use a perfect combination of checks to identify our binary
is_our_binary() {
    path="$1"
    [ -x "$path" ] || return 1
    
    # 1. Check version format (must start with "beacon X.Y.Z")
    "$path" --version 2>&1 | grep -q "^beacon [0-9]" || return 1
    
    # 2. Check for the unique project tagline
    help_text=$("$path" --help 2>&1)
    echo "$help_text" | grep -q "agent-ready" || return 1
    
    # 3. Verify the core command set exists
    echo "$help_text" | grep -q "generate" || return 1
    echo "$help_text" | grep -q "validate" || return 1
    echo "$help_text" | grep -q "serve" || return 1
    
    return 0
}

if [ -e "$INSTALL_DIR/$BINARY" ]; then
    if is_our_binary "$INSTALL_DIR/$BINARY"; then
        echo "Beacon installation detected at $INSTALL_DIR/$BINARY. Proceeding with upgrade to $VERSION..."
    else
        echo "Error: A file named '$BINARY' already exists in $INSTALL_DIR and does not appear to be Beacon."
        echo "To install Beacon with a different name, pass it as an argument:"
        echo "  curl -fsSL https://raw.githubusercontent.com/DavidNzube101/beacon/master/install.sh | sh -s -- your-custom-name"
        echo ""
        echo "Example: curl ... | sh -s -- beacon-ai $VERSION"
        exit 1
    fi
fi

TARGET=$(get_target)
URL="https://github.com/$REPO/releases/download/$VERSION/$TARGET"

echo "Installing Beacon $VERSION as '$BINARY'..."
echo "Downloading $TARGET..."

TMP_BIN="/tmp/beacon_$(date +%s)"
if ! curl -fsSL "$URL" -o "$TMP_BIN"; then
    echo "Error: Could not download Beacon version $VERSION. Please ensure the version exists." >&2
    exit 1
fi
chmod +x "$TMP_BIN"

if [ -w "$INSTALL_DIR" ]; then
    mv -f "$TMP_BIN" "$INSTALL_DIR/$BINARY"
else
    echo "Sudo privileges are required to install to $INSTALL_DIR"
    sudo mv -f "$TMP_BIN" "$INSTALL_DIR/$BINARY"
fi

echo "Successfully installed to $INSTALL_DIR/$BINARY"
echo "Run: $BINARY --help"
