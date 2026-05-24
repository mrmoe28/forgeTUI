#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${FORGETUI_REPO_URL:-https://github.com/mrmoe28/forgeTUI.git}"
INSTALL_DIR="${FORGETUI_HOME:-$HOME/.local/share/forgeTUI}"
BIN_DIR="${FORGETUI_BIN_DIR:-$HOME/.local/bin}"
OPENCODE_CONFIG="${OPENCODE_CONFIG:-$HOME/.config/opencode/opencode.json}"
OLLAMA_BASE_URL="${OLLAMA_BASE_URL:-http://ollama.lan:11434/v1}"

log() {
  printf '\033[1;36m==>\033[0m %s\n' "$*" >&2
}

warn() {
  printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2
}

have() {
  command -v "$1" >/dev/null 2>&1
}

ensure_path_hint() {
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) warn "$BIN_DIR is not on PATH. Add this to your shell profile: export PATH=\"$BIN_DIR:\$PATH\"" ;;
  esac
}

install_rust_if_missing() {
  if have cargo; then
    return
  fi

  log "Installing Rust toolchain with rustup"
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
}

install_ollama_if_missing() {
  if have ollama; then
    return
  fi

  log "Installing Ollama"
  curl -fsSL https://ollama.com/install.sh | sh
}

install_opencode_if_missing() {
  if have opencode; then
    return
  fi

  log "Installing opencode"
  curl -fsSL https://opencode.ai/install | bash
}

checkout_repo() {
  if [ -f Cargo.toml ] && grep -q 'name = "forge-tui"' Cargo.toml; then
    pwd
    return
  fi

  if [ -d "$INSTALL_DIR/.git" ]; then
    log "Updating ForgeTUI checkout"
    git -C "$INSTALL_DIR" pull --ff-only
  else
    log "Cloning ForgeTUI into $INSTALL_DIR"
    mkdir -p "$(dirname "$INSTALL_DIR")"
    git clone "$REPO_URL" "$INSTALL_DIR"
  fi

  printf '%s\n' "$INSTALL_DIR"
}

configure_opencode() {
  log "Configuring opencode Ollama provider"
  mkdir -p "$(dirname "$OPENCODE_CONFIG")"

  if ! have python3; then
    warn "python3 not found; skipping opencode config merge"
    return
  fi

  OPENCODE_CONFIG="$OPENCODE_CONFIG" OLLAMA_BASE_URL="$OLLAMA_BASE_URL" python3 <<'PY'
import json
import os
from pathlib import Path

path = Path(os.environ["OPENCODE_CONFIG"])
base_url = os.environ["OLLAMA_BASE_URL"]

if path.exists():
    try:
        data = json.loads(path.read_text())
    except json.JSONDecodeError:
        backup = path.with_suffix(path.suffix + ".bak")
        path.rename(backup)
        data = {}
else:
    data = {}

provider = data.setdefault("provider", {})
ollama = provider.setdefault("ollama", {})
ollama["npm"] = ollama.get("npm", "@ai-sdk/openai-compatible")
ollama["name"] = ollama.get("name", "Ollama LAN")
options = ollama.setdefault("options", {})
options["baseURL"] = options.get("baseURL", base_url)
models = ollama.setdefault("models", {})

for model, name in {
    "glm-4.7:cloud": "GLM 4.7 Cloud",
    "glm-4.6:cloud": "GLM 4.6 Cloud",
    "qwen3-coder:480b-cloud": "Qwen3 Coder 480B Cloud",
    "gpt-oss:120b-cloud": "GPT OSS 120B Cloud",
    "minimax-m2:cloud": "MiniMax M2 Cloud",
    "minimax-m2.1:cloud": "MiniMax M2.1 Cloud",
    "kimi-k2.6:cloud": "Kimi K2.6 Cloud",
    "deepseek-v4-flash:cloud": "DeepSeek V4 Flash Cloud",
    "qwen2.5-coder:32b": "Qwen 2.5 Coder 32B",
    "qwen2.5-coder:7b": "Qwen 2.5 Coder 7B",
    "qwen2.5-coder:7b-claude": "Qwen 2.5 Coder 7B Claude",
    "hermes3:claude": "Hermes 3 Claude",
    "hermes3:8b": "Hermes 3 8B",
    "mistral-small:24b": "Mistral Small 24B",
    "qwen2.5:14b": "Qwen 2.5 14B",
    "llama3.1:8b": "Llama 3.1 8B",
}.items():
    models.setdefault(model, {"name": name})

data.setdefault("model", "ollama/glm-4.7:cloud")
data.setdefault("small_model", "ollama/qwen2.5-coder:7b")

path.write_text(json.dumps(data, indent=2) + "\n")
PY
}

install_forge() {
  local repo_dir="$1"
  log "Building ForgeTUI"
  cargo build --release --manifest-path "$repo_dir/Cargo.toml"

  mkdir -p "$BIN_DIR"
  cp "$repo_dir/target/release/forge" "$BIN_DIR/forge"
  chmod +x "$BIN_DIR/forge"
  log "Installed forge to $BIN_DIR/forge"
}

main() {
  if ! have curl; then
    printf 'curl is required for setup.\n' >&2
    exit 1
  fi
  if ! have git; then
    printf 'git is required for setup.\n' >&2
    exit 1
  fi

  install_rust_if_missing
  install_ollama_if_missing
  install_opencode_if_missing
  repo_dir="$(checkout_repo)"
  configure_opencode
  install_forge "$repo_dir"
  ensure_path_hint

  log "Setup complete"
  printf 'Run ForgeTUI with: forge\n'
  printf 'If using Ollama Cloud models, make sure you are signed in with: ollama signin\n'
}

main "$@"
