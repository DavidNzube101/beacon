#!/usr/bin/env sh
set -e

REPO="DavidNzube101/beacon"
BINARY="beacon"
INSTALL_DIR="/usr/local/bin"

get_latest_version() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep '"tag_name"' \
        | sed 's/.*"tag_name": "\(.*\)".*/\1/'
}

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

VERSION=$(get_latest_version)
TARGET=$(get_target)
URL="https://github.com/$REPO/releases/download/$VERSION/$TARGET"

echo "Installing Beacon $VERSION..."
echo "Downloading $TARGET..."

curl -fsSL "$URL" -o "/tmp/$BINARY"
chmod +x "/tmp/$BINARY"

# --- New logic starts here ---

# Function to perform the move, using sudo if needed
move_binary() {
    local src="$1"
    local dest="$2"
    local dest_binary_name=$(basename "$dest")
    if [ -w "$(dirname "$dest")" ]; then
        mv "$src" "$dest"
    else
        echo "Sudo privileges are required to install to $INSTALL_DIR"
        sudo mv "$src" "$dest"
    fi
    echo "Beacon installed to $dest"
    echo "Run: $dest_binary_name --help"
}

# Check if the binary already exists
if [ -e "$INSTALL_DIR/$BINARY" ]; then
    echo "A file named '$BINARY' already exists in $INSTALL_DIR."
    read -rp "What would you like to do? [(o)verwrite, (r)ename, (c)ancel]: " choice
    
    case "$choice" in
        o|O)
            echo "Overwriting existing binary..."
            move_binary "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"
            ;;
        r|R)
            read -rp "Enter a new name for the binary: " new_name
            if [ -z "$new_name" ]; then
                echo "No name entered. Aborting." >&2
                exit 1
            fi
            if [ -e "$INSTALL_DIR/$new_name" ]; then
                echo "File '$new_name' also exists. Aborting." >&2
                exit 1
            fi
            echo "Installing with new name: $new_name"
            move_binary "/tmp/$BINARY" "$INSTALL_DIR/$new_name"
            ;;
        *)
            echo "Installation cancelled."
            exit 0
            ;;
    esac
else
    # No conflict, proceed with standard installation
    move_binary "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"
fi
