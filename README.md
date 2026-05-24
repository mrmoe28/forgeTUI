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

Default model backend:

```text
opencode run -m ollama/glm-4.7:cloud
```

Prototype controls:

- type a request and press `Enter`: run it through `opencode`
- `/help`: show available commands
- `/agent <task>` or `Ctrl-A`: spawn a real opencode subagent in an isolated Git worktree
- `/run` or `Ctrl-R`: run the selected task
- `/sidebar` or `Ctrl-B`: show or hide the side panels
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
