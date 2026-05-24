# Architecture

## Core Modules

- Workspace manager: opens a project, tracks root paths, and discovers Git state.
- Pane manager: runs shells, editors, logs, tests, and long-running commands.
- Search manager: fuzzy-selects files, commands, branches, tasks, and symbols.
- Task runner: starts builds, tests, linters, dev servers, and watch commands.
- Agent manager: spawns subagents, streams output, tracks ownership, and collects diffs.
- Diff reviewer: previews and applies or rejects proposed changes.

## Agent Flow

1. User creates a task from the agent panel or command palette.
2. The app assigns a workspace isolation strategy.
3. The agent runs with an explicit scope and optional file ownership.
4. Logs stream into the agent panel.
5. The app detects changed files.
6. User reviews the diff.
7. Accepted changes are merged into the main workspace.

## Initial Implementation Choice

Rust with `ratatui` and `crossterm` is the strongest default for a durable local TUI.
Go with Bubble Tea is also viable if fast iteration is preferred.

