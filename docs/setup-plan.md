# ForgeTUI Setup Plan

Goal: one command should prepare a machine to run ForgeTUI with opencode and Ollama-backed coding models.

## Plan

1. Check required bootstrap tools: `curl` and `git`.
2. Install Rust/Cargo if missing using rustup.
3. Install Ollama if missing using the official Ollama install script.
4. Install opencode if missing using the official opencode install script.
5. Clone or update ForgeTUI under `~/.local/share/forgeTUI` when the installer is run outside a checkout.
6. Merge opencode provider configuration for the Ollama OpenAI-compatible endpoint.
7. Register known coding-capable Ollama cloud models in opencode:
   - `glm-4.7:cloud`
   - `glm-4.6:cloud`
   - `qwen3-coder:480b-cloud`
   - `gpt-oss:120b-cloud`
   - `minimax-m2:cloud`
   - `minimax-m2.1:cloud`
   - `kimi-k2.6:cloud`
   - `deepseek-v4-flash:cloud`
8. Build ForgeTUI in release mode.
9. Install the `forge` binary into `~/.local/bin`.
10. Tell the user to run `ollama signin` if they want Ollama Cloud access.
11. Launch with `forge`.

## Defaults

- Install directory: `~/.local/share/forgeTUI`
- Binary directory: `~/.local/bin`
- Ollama OpenAI-compatible endpoint: `http://ollama.lan:11434/v1`
- Default model: `ollama/glm-4.7:cloud`

## Overrides

The installer supports:

- `FORGETUI_HOME`
- `FORGETUI_BIN_DIR`
- `FORGETUI_REPO_URL`
- `OPENCODE_CONFIG`
- `OLLAMA_BASE_URL`
