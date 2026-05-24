# ForgeTUI

A unified terminal workspace for complicated coding work.

Binary name: `forge`

## Install

Run this on a new machine:

```sh
curl -fsSL https://raw.githubusercontent.com/mrmoe28/forgeTUI/main/scripts/install.sh | bash
```

Then start the TUI:

```sh
forge
```

The installer:

- installs Rust/Cargo if missing
- installs Ollama if missing
- installs opencode if missing
- clones or updates ForgeTUI
- configures opencode for the Ollama OpenAI-compatible endpoint
- registers known Ollama cloud coding models
- builds ForgeTUI in release mode
- installs `forge` into `~/.local/bin`

If you plan to use Ollama Cloud models, sign in once:

```sh
ollama signin
```

Setup details are in [docs/setup-plan.md](docs/setup-plan.md).

Planned capabilities:

- Multiplexed terminal panes
- Project file search and command palette
- Editor integration
- Build, test, and log watchers
- Debugger/process-monitor views
- Subagent spawning and supervision
- Diff review before applying agent changes

## Architecture Direction

The first version should orchestrate existing tools instead of replacing them:

- `tmux`-style pane/session management
- `fzf`-style fuzzy project navigation
- editor launch support for `vim`, `nvim`, `nano`, or `$EDITOR`
- task runners for tests, builds, logs, and shell commands
- agent adapters for local CLIs such as `codex`, `claude`, `opencode`, `openclaude`, and `ollama` workflows

## Current Prototype

Run locally with:

```sh
cargo run
```

Default model backend:

```text
opencode run -m ollama/glm-4.7:cloud
```

Configured Ollama cloud models:

- `ollama/glm-4.7:cloud`
- `ollama/glm-4.6:cloud`
- `ollama/qwen3-coder:480b-cloud`
- `ollama/gpt-oss:120b-cloud`
- `ollama/minimax-m2:cloud`
- `ollama/minimax-m2.1:cloud`
- `ollama/kimi-k2.6:cloud`
- `ollama/deepseek-v4-flash:cloud`

ForgeTUI uses opencode for configured models. If an `ollama/...` model is not configured in opencode, ForgeTUI falls back to `ollama run`.

Prototype controls:

- type a request and press `Enter`: run it through `opencode`
- `/help`: show available commands
- `/agent <task>` or `Ctrl-A`: spawn a real opencode subagent in an isolated Git worktree
- `/run` or `Ctrl-R`: run the selected task
- `/sidebar` or `Ctrl-B`: show or hide the side panels, hidden by default
- `/model [provider/model]`: show or switch the active model
- `/models`: list known local models
- `/cancel` or `Ctrl-X`: cancel the latest running job
- `/diff`: show pending post-run diff
- `/approve`: accept pending changes
- `/reject`: reject pending changes when the workspace was clean before the run
- `/test`: run `cargo check`
- `/autotest`: toggle automatic `cargo check` after edited main-workspace runs
- `/history`: show recent job/session history
- `Up` / `Down`: move task selection
- `Tab` / `Shift-Tab`: move agent selection
- `Esc` or `Ctrl-C`: quit

Normal prompts run in the background and stream command output into the conversation.
After a main-workspace opencode run, ForgeTUI snapshots Git status/diff before and after, tracks changed files through `git status --short`, and exposes the result through `/diff`, `/approve`, and `/reject`.

## Safety Model

Subagents should work in isolated branches, worktrees, temp folders, or containers.
All file changes should return through an explicit diff review flow.
