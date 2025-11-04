#!/usr/bin/env bash
# gflow installer script

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REPO="AndPuQing/gflow"
INSTALL_DIR="${GFLOW_INSTALL_DIR:-${HOME}/.cargo/bin}"
GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"
GITHUB_DOWNLOAD="https://github.com/${REPO}/releases/download"

# Functions
info() {
    echo -e "${GREEN}info:${NC} $1"
}

warn() {
    echo -e "${YELLOW}warn:${NC} $1"
}

error() {
    echo -e "${RED}error:${NC} $1"
    exit 1
}

check_cmd() {
    command -v "$1" > /dev/null 2>&1
}

# Detect platform
detect_platform() {
    local os arch

    # Detect OS
    case "$(uname -s)" in
        Linux*)
            os="linux"
            ;;
        Darwin*)
            os="darwin"
            ;;
        *)
            error "Unsupported operating system: $(uname -s)"
            ;;
    esac

    # Detect architecture
    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            arch="aarch64"
            ;;
        *)
            error "Unsupported architecture: $(uname -m)"
            ;;
    esac

    # For now, we only support x86_64-unknown-linux-musl based on releases
    if [ "$os" = "linux" ] && [ "$arch" = "x86_64" ]; then
        echo "x86_64-unknown-linux-musl"
    else
        error "Pre-built binaries are currently only available for x86_64 Linux. Please use 'cargo install gflow' instead."
    fi
}

# Check prerequisites
check_prerequisites() {
    info "Checking prerequisites..."

    if ! check_cmd curl && ! check_cmd wget; then
        error "curl or wget is required but not found"
    fi

    if ! check_cmd tar; then
        error "tar is required but not found"
    fi

    if ! check_cmd tmux; then
        warn "tmux is not installed. gflow requires tmux to function properly."
        warn "Please install tmux: sudo apt-get install tmux (Debian/Ubuntu) or brew install tmux (macOS)"
    fi
}

# Download file
download() {
    local url="$1"
    local output="$2"

    if check_cmd curl; then
        curl --proto '=https' --tlsv1.2 -fsSL "$url" -o "$output"
    elif check_cmd wget; then
        wget --https-only --secure-protocol=TLSv1_2 -q "$url" -O "$output"
    fi
}

# Get latest version
get_latest_version() {
    info "Fetching latest version..."

    local version
    if check_cmd curl; then
        version=$(curl -sSL "$GITHUB_API" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    elif check_cmd wget; then
        version=$(wget -qO- "$GITHUB_API" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    fi

    if [ -z "$version" ]; then
        error "Failed to fetch latest version"
    fi

    echo "$version"
}

# Install gflow
install_gflow() {
    local platform
    platform=$(detect_platform)

    local version
    version=$(get_latest_version)

    info "Installing gflow ${version} for ${platform}..."

    # Create temporary directory
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Download archive
    local archive_name="gflow_${version}_${platform}.tar.gz"
    local download_url="${GITHUB_DOWNLOAD}/${version}/${archive_name}"
    local archive_path="${tmp_dir}/${archive_name}"

    info "Downloading from ${download_url}..."
    if ! download "$download_url" "$archive_path"; then
        error "Failed to download gflow"
    fi

    # Extract archive
    info "Extracting archive..."
    tar -xzf "$archive_path" -C "$tmp_dir"

    # Create install directory if it doesn't exist
    mkdir -p "$INSTALL_DIR"

    # Install binaries
    info "Installing binaries to ${INSTALL_DIR}..."
    for binary in gbatch gcancel gflowd ginfo gjob gqueue; do
        if [ -f "${tmp_dir}/${binary}" ]; then
            install -m 755 "${tmp_dir}/${binary}" "${INSTALL_DIR}/${binary}"
            info "  ✓ ${binary}"
        else
            warn "  ✗ ${binary} not found in archive"
        fi
    done

    # Check if install directory is in PATH
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        warn "${INSTALL_DIR} is not in your PATH"
        warn "Add the following line to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo ""
        echo "    export PATH=\"\${HOME}/.cargo/bin:\$PATH\""
        echo ""
    fi

    info "gflow ${version} has been successfully installed!"
    echo ""
    echo "To get started, run:"
    echo "    gflowd up      # Start the scheduler daemon"
    echo "    gbatch --help  # Submit jobs"
    echo "    gqueue         # View job queue"
    echo ""
    echo "For more information, visit: https://github.com/${REPO}"
}

# Main
main() {
    echo "gflow installer"
    echo "==============="
    echo ""

    check_prerequisites
    install_gflow
}

# Run if not sourced
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
