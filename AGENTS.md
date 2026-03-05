# GPX Agent Guide

This file defines the working contract for coding agents and contributors in this repository.
Keep it short, executable, and stable.

Quick links:

- User docs: `README.md`
- Full specification: `docs/SPEC.md`

## 1. Document Boundaries (Source of Truth)

- `AGENTS.md` (this file): contributor/agent execution rules and change policy.
- `README.md`: user-facing install/usage/config docs.
- `docs/SPEC.md`: full product and architecture specification (detailed behavior, rationale, trade-offs).

Do not duplicate long spec content in multiple files.

## 2. Product Snapshot

- Binary: `gpx`
- Scope: Unix (macOS/Linux)
- Core goal: switch Git/SSH identities by active profile with minimal direct edits to main config files.
- Preferred mechanism: stable include entry + generated include targets.
- Runtime model: no daemon, hook-driven and short-lived commands.

## 3. Non-Negotiable Behavior

### 3.1 Config and Resolution

- Prefer `$XDG_CONFIG_HOME/gpx/config.toml`; support `$XDG_CONFIG_HOME/gpx/config` (INI-compatible).
- If both config files exist, `config.toml` wins and `doctor/status --verbose` must report this.
- Each rule must have at least one `match.*` condition.
- If no rule matches, fallback to `core.defaultProfile`; if unset, return actionable error.
- Rule strategy:
  - `first-match`: declaration order.
  - `highest-score`: `priority` first, then specificity; unresolved ties are errors.

### 3.2 Git/SSH Injection

- Git should use a stable global include entry pointing to active include.
- Profile switching should update generated include targets, not repeatedly rewrite root configs.
- SSH default mode should use fixed include refresh; dynamic `Match exec` is optional (`ssh.dynamicMatch`).

### 3.3 Hooks and Run Mode

- Shell hook and Git hook must be independently installable and usable together.
- `run` mode must be zero-persistence (environment override + `exec`), with no config residue.

### 3.4 Submodule/Worktree

- Submodules are evaluated independently by their own context.
- Worktree handling must respect `extensions.worktreeConfig`; fallback behavior is controlled by `worktree.allowSharedFallback`.

## 4. Command Surface

```text
gpx init
gpx doctor
gpx status [--verbose]
gpx list [profiles|rules] [--json]
gpx check [--cwd <path>] [--json]
gpx apply [--cwd <path>] [--profile <name>] [--dry-run]
gpx hook install [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx hook uninstall [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx run [--profile <name>] -- <git args...>
gpx -- <git args...>
gpx ssh-eval --profile <name> [--cwd <path>]   # internal
```

## 5. Output Contract

All human-readable command output must follow one format:

- Title: `<Title> report` (bold)
- Item: `- <Label>: <Value>`
- Note: `- <Message>`

Status color conventions:

- Success/info: `OK` / `ENABLED` / `CREATED` / `UPDATED` / `INSTALLED` (green/cyan)
- Warning/default/missing: `WARN` / `UNSET` / `<none>` / `MISSING` (yellow)
- Failure: `FAIL` / `INVALID` (red)

Machine-readable output (`--json`) must remain pure JSON.

## 6. Engineering Constraints

- Rust formatting is controlled by `rustfmt.toml` (`tab_spaces = 2`).
- Before merge, pass:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
- Prefer `anyhow` + `Context` and typed domain errors via `thiserror`.
- Avoid runtime `unwrap/expect` in production paths.
- Centralize user-facing output via shared helpers (`src/output.rs`).

## 7. Change Policy (Required)

When changing config schema/semantics (add/remove/rename/change behavior):

1. Update `README.md` `Config Reference` (`Required` / `Default` / `Allowed`).
2. Update `docs/SPEC.md` to keep full spec accurate.
3. Keep this file focused; only update here if execution rules changed.

A PR is incomplete if `README.md` and `docs/SPEC.md` drift on config behavior.

## 8. Suggested Placement for Information

- Put concise, execution-critical rules in `AGENTS.md`.
- Put user onboarding and everyday usage in `README.md`.
- Put detailed design, rationale, and long-form behavior spec in `docs/SPEC.md`.
