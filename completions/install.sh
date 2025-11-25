#!/usr/bin/env bash
# Installation script for gflow completions

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHELL_TYPE="${1:-auto}"

print_usage() {
    cat <<EOF
Usage: $0 [SHELL]

Install gflow shell completions with dynamic job ID support.

SHELL can be: bash, zsh, fish, or auto (default)
  auto - automatically detect your shell and install completions

Examples:
  $0           # Auto-detect and install
  $0 bash      # Install bash completions
  $0 zsh       # Install zsh completions
  $0 fish      # Install fish completions

EOF
}

detect_shell() {
    if [ -n "$SHELL" ]; then
        basename "$SHELL"
    else
        echo "bash"
    fi
}

install_bash() {
    echo "Installing bash completions..."

    # Create completion directory
    mkdir -p "$HOME/.local/share/bash-completion/completions"

    # Generate basic completions
    for cmd in gflowd gbatch gjob gqueue gcancel ginfo; do
        echo "  Generating $cmd completion..."
        if command -v "$cmd" >/dev/null 2>&1; then
            "$cmd" completion bash > "$HOME/.local/share/bash-completion/completions/$cmd"
        else
            echo "    Warning: $cmd not found in PATH, skipping"
        fi
    done

    # Add dynamic completion source to bashrc if not already present
    BASHRC="$HOME/.bashrc"
    SOURCE_LINE="source \"$SCRIPT_DIR/gflow-dynamic.bash\""

    if ! grep -qF "gflow-dynamic.bash" "$BASHRC" 2>/dev/null; then
        echo "" >> "$BASHRC"
        echo "# gflow dynamic completions" >> "$BASHRC"
        echo "$SOURCE_LINE" >> "$BASHRC"
        echo "  Added dynamic completions to $BASHRC"
    else
        echo "  Dynamic completions already in $BASHRC"
    fi

    echo "✓ Bash completions installed!"
    echo "  Run: source ~/.bashrc"
}

install_zsh() {
    echo "Installing zsh completions..."

    # Create completion directory
    mkdir -p "$HOME/.zsh/completions"

    ZSHRC="$HOME/.zshrc"

    # Check if fpath is set
    if ! grep -qF ".zsh/completions" "$ZSHRC" 2>/dev/null; then
        echo "" >> "$ZSHRC"
        echo "# gflow completions" >> "$ZSHRC"
        echo "fpath=(~/.zsh/completions \$fpath)" >> "$ZSHRC"
        echo "  Added completion directory to fpath in $ZSHRC"
    fi

    # Add dynamic completion source
    SOURCE_LINE="source \"$SCRIPT_DIR/gflow-dynamic.zsh\""
    if ! grep -qF "gflow-dynamic.zsh" "$ZSHRC" 2>/dev/null; then
        echo "$SOURCE_LINE" >> "$ZSHRC"
        echo "  Added dynamic completions to $ZSHRC"
    else
        echo "  Dynamic completions already in $ZSHRC"
    fi

    # Check for compinit
    if ! grep -qF "compinit" "$ZSHRC" 2>/dev/null; then
        echo "" >> "$ZSHRC"
        echo "autoload -Uz compinit && compinit" >> "$ZSHRC"
        echo "  Added compinit to $ZSHRC"
    fi

    echo "✓ Zsh completions installed!"
    echo "  Run: source ~/.zshrc"
}

install_fish() {
    echo "Installing fish completions..."

    # Create completion directory
    mkdir -p "$HOME/.config/fish/completions"

    # Generate basic completions
    for cmd in gflowd gbatch gjob gqueue gcancel ginfo; do
        echo "  Generating $cmd completion..."
        if command -v "$cmd" >/dev/null 2>&1; then
            "$cmd" completion fish > "$HOME/.config/fish/completions/$cmd.fish"
        else
            echo "    Warning: $cmd not found in PATH, skipping"
        fi
    done

    # Copy dynamic completions
    cp "$SCRIPT_DIR/gflow-dynamic.fish" "$HOME/.config/fish/completions/"

    echo "✓ Fish completions installed!"
    echo "  Completions will be loaded automatically in new fish sessions"
}

# Main logic
case "$SHELL_TYPE" in
    -h|--help)
        print_usage
        exit 0
        ;;
    bash)
        install_bash
        ;;
    zsh)
        install_zsh
        ;;
    fish)
        install_fish
        ;;
    auto)
        DETECTED_SHELL=$(detect_shell)
        echo "Detected shell: $DETECTED_SHELL"
        case "$DETECTED_SHELL" in
            bash) install_bash ;;
            zsh) install_zsh ;;
            fish) install_fish ;;
            *)
                echo "Error: Unsupported shell: $DETECTED_SHELL"
                echo "Please run with explicit shell: $0 bash|zsh|fish"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Error: Unknown shell: $SHELL_TYPE"
        print_usage
        exit 1
        ;;
esac
