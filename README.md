# ForgeTUI

A unified terminal workspace for complicated coding work.

Binary name: `forge`

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

Prototype controls:

- type a request and press `Enter`: add it to the conversation
- `/help`: show available commands
- `/agent` or `Ctrl-A`: spawn a placeholder subagent
- `/run` or `Ctrl-R`: mark the selected task as started
- `/sidebar` or `Ctrl-B`: show or hide the side panels
- `Up` / `Down`: move task selection
- `Tab` / `Shift-Tab`: move agent selection
- `Esc` or `Ctrl-C`: quit

## Safety Model

Subagents should work in isolated branches, worktrees, temp folders, or containers.
All file changes should return through an explicit diff review flow.
