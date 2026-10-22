#!/usr/bin/env bash
# Remove Play Anything commands, managed runtimes, and installer PATH entries.
set -euo pipefail

INSTALL_HOME="${PLAY_ANYTHING_HOME:-$HOME/.local/share/play-anything}"
EXPLICIT_COMMAND_DIR="${PLAY_ANYTHING_COMMAND_DIR:-}"
TEMP_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIRECTORY"' EXIT

log() {
    printf '\033[1;36m[play-anything]\033[0m %s\n' "$*"
}

fail() {
    printf '\033[1;31m[play-anything]\033[0m %s\n' "$*" >&2
    exit 1
}

[[ -n "$INSTALL_HOME" && "$INSTALL_HOME" != '/' && "$INSTALL_HOME" != "$HOME" ]] || \
    fail "Refusing to remove unsafe installation path: $INSTALL_HOME"

remove_file() {
    local path="$1"
    local directory
    directory="$(dirname "$path")"
    if [[ -w "$directory" ]]; then
        rm -f "$path"
    elif command -v sudo >/dev/null 2>&1; then
        sudo rm -f "$path"
    else
        fail "Cannot remove $path; root access or sudo is required"
    fi
}

remove_commands() {
    local directory="$1"
    local command="$directory/play-anything"
    local alias="$directory/Play-Anything"

    if [[ -f "$command" ]] && \
        { grep -F '# Play Anything managed command' "$command" >/dev/null 2>&1 || \
          grep -F "PLAY_ANYTHING_ROOT=\"$INSTALL_HOME\"" "$command" >/dev/null 2>&1; }; then
        remove_file "$command"
        log "Removed $command"
    fi
    if [[ -L "$alias" && "$(readlink "$alias")" == 'play-anything' ]]; then
        remove_file "$alias"
        log "Removed $alias"
    elif [[ -f "$alias" ]] && \
        { grep -F '# Play Anything managed command' "$alias" >/dev/null 2>&1 || \
          grep -F "PLAY_ANYTHING_ROOT=\"$INSTALL_HOME\"" "$alias" >/dev/null 2>&1; }; then
        remove_file "$alias"
        log "Removed $alias"
    fi
}

clean_profile() {
    local profile="$1"
    local directory="$2"
    local path_line="export PATH=\"$directory:\$PATH\""
    local output="$TEMP_DIRECTORY/profile"
    [[ -f "$profile" && -w "$profile" ]] || return 0
    grep -F "$path_line" "$profile" >/dev/null 2>&1 || return 0

    awk -v target="$path_line" '
        $0 == target {
            if (has_previous && previous == "# Play Anything") {
                has_previous = 0
            } else if (has_previous) {
                print previous
            }
            previous = ""
            next
        }
        {
            if (has_previous) print previous
            previous = $0
            has_previous = 1
        }
        END {
            if (has_previous) print previous
        }
    ' "$profile" >"$output"
    cat "$output" >"$profile"
}

COMMAND_DIRS=()
if [[ -n "$EXPLICIT_COMMAND_DIR" ]]; then
    COMMAND_DIRS+=("$EXPLICIT_COMMAND_DIR")
else
    COMMAND_DIRS+=("$HOME/.local/bin" '/usr/local/bin')
    if [[ -n "${PREFIX:-}" && "${PREFIX:-}" == *com.termux* ]]; then
        COMMAND_DIRS+=("$PREFIX/bin")
    fi
fi

for directory in "${COMMAND_DIRS[@]}"; do
    remove_commands "$directory"
    for profile in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
        clean_profile "$profile" "$directory"
    done
done

if [[ -e "$INSTALL_HOME" ]]; then
    if [[ -w "$(dirname "$INSTALL_HOME")" ]]; then
        rm -rf "$INSTALL_HOME"
    elif command -v sudo >/dev/null 2>&1; then
        sudo rm -rf "$INSTALL_HOME"
    else
        fail "Cannot remove $INSTALL_HOME; root access or sudo is required"
    fi
    log "Removed $INSTALL_HOME"
fi

log 'Play Anything is uninstalled. Restart your terminal to refresh PATH and command lookup.'
log 'Shared media packages installed by Termux, Homebrew, or Linux package managers were left unchanged.'
