# gpx

`gpx` (Git Profile Extension) automatically switches Git and SSH identities across different repositories/directories.

## Scope

- Unix platforms (macOS / Linux)
- Users who need to switch identities between personal/corporate and multi-account repositories

## Install

- Install from crates.io:

  ```bash
  cargo install gpx-cli
  gpx --help
  ```

- Build from source:

  ```bash
  cargo build --release
  ./target/release/gpx --help
  ```

## Quick Start

- Initialize the include entry:

  ```bash
  gpx init
  ```

- Write `config.toml` in the config directory (see the example below).

- Check which rules apply to the current directory:

  ```bash
  gpx check
  ```

- Apply the current profile:

  ```bash
  gpx apply
  ```

- Check status and diagnostics:

  ```bash
  gpx status --verbose
  gpx doctor
  ```

## Config File Paths

- Config: `$XDG_CONFIG_HOME/gpx/config.toml` (default: `~/.config/gpx/config.toml`)
- Compatible `.gitconfig`-style config with lower priority than `.toml`: `$XDG_CONFIG_HOME/gpx/config`
- Git config cache: `$XDG_CACHE_HOME/gpx/git/...`
- SSH config cache: `$XDG_CACHE_HOME/gpx/ssh/gpx_ssh.conf`
- State: `$XDG_STATE_HOME/gpx/state.toml`

## Config Example

```toml
[core]
defaultProfile = "personal"
ruleMode = "first-match" # first-match | highest-score
mode = "global-active"   # global-active | repo-local

[profile.work.user]
name = "Alice Chen"
email = "alice@corp.com"

[profile.work.ssh]
key = "~/.ssh/id_ed25519_work"
identitiesOnly = true

[profile.personal.user]
name = "Alice"
email = "me@example.com"

[profile.personal.ssh]
key = "~/.ssh/id_ed25519_personal"

[hook]
shell = true
git = true
fixPolicy = "abort-once" # continue | abort-once

[run]
allowProfileOverride = true

[worktree]
allowSharedFallback = false

[ssh]
dynamicMatch = false

[rule.corp-path]
profile = "work"
priority = 200
"match.path" = "~/code/company/**"

[rule.corp-remote]
profile = "work"
priority = 180
"match.remoteHost" = "github.com"
"match.remoteOrg" = "corp-org"
```

## Config Reference

- `core.defaultProfile`
  - Required: Recommended
  - Default: `<none>`
  - Allowed: existing profile name, e.g. `personal`, `work`
  - Notes: Used as fallback when no rule matches in `check`, `apply`, and `run`. If unset and no rule matches, resolution fails with an error instead of silently choosing an identity.
- `core.ruleMode`
  - Required: No
  - Default: `first-match`
  - Allowed: `first-match`, `highest-score`
  - Notes: `first-match` uses declaration order only. `highest-score` compares `priority` first and then matcher specificity; unresolved ties are treated as conflicts and require config changes.
- `core.mode`
  - Required: No
  - Default: `global-active`
  - Allowed: `global-active`, `repo-local`
  - Notes: Controls write target for `gpx apply`. `global-active` updates one global active include shared by all repos; `repo-local` writes include settings into the current repo/worktree context.
- `profile.<name>.user.name`
  - Required: No
  - Default: `<none>`
  - Allowed: any non-empty string, e.g. `Alice Chen`
  - Notes: Compiled into the profile include as Git `user.name`. Useful when separate profiles require explicit author identity per repo.
- `profile.<name>.user.email`
  - Required: No
  - Default: `<none>`
  - Allowed: email string, e.g. `alice@corp.com`
  - Notes: Compiled into the profile include as Git `user.email`. This is usually the primary identity field teams validate in commits.
- `profile.<name>.user.signingkey`
  - Required: No
  - Default: `<none>`
  - Allowed: key id/fingerprint string
  - Notes: Compiled as Git `user.signingkey` for commit/tag signing workflows. Leave unset if signing is not required for that profile.
- `profile.<name>.gpg.format`
  - Required: No
  - Default: `<none>`
  - Allowed: string, e.g. `openpgp`
  - Notes: Optional `gpg.*` extension field written into the profile include. Configure this only when your signing backend differs across profiles.
- `profile.<name>.ssh.key`
  - Required: No
  - Default: `<none>`
  - Allowed: SSH private key path, e.g. `~/.ssh/id_ed25519_work`
  - Notes: `~` is expanded before use. The key path is used for generated SSH include files and for temporary SSH command injection in `run` mode.
- `profile.<name>.ssh.identitiesOnly`
  - Required: No
  - Default: `false`
  - Allowed: `true`, `false`
  - Notes: When `true`, emits `IdentitiesOnly yes` so SSH avoids offering unrelated agent keys, which helps prevent wrong-account auth on multi-key machines.
- `hook.shell`
  - Required: No
  - Default: `false`
  - Allowed: `true`, `false`
  - Notes: Enables shell-hook installation intent in config. It does not install hooks automatically; run `gpx hook install --shell ...` to actually install.
- `hook.git`
  - Required: No
  - Default: `false`
  - Allowed: `true`, `false`
  - Notes: Enables git-hook installation intent in config. Actual hook files are created by `gpx hook install --git`.
- `hook.fixPolicy`
  - Required: No
  - Default: `continue`
  - Allowed: `continue`, `abort-once`
  - Notes: Applies to hook-triggered auto-fix flow (`--hook-mode`). `continue` keeps current Git action running after identity correction; `abort-once` stops once so the next command runs with corrected identity from the start.
- `run.allowProfileOverride`
  - Required: No
  - Default: `false`
  - Allowed: `true`, `false`
  - Notes: Governs whether `gpx run --profile <name> -- ...` can force a profile. When `false`, `run` must use rule/default resolution and explicit override is rejected.
- `worktree.allowSharedFallback`
  - Required: No
  - Default: `false`
  - Allowed: `true`, `false`
  - Notes: Only relevant in `repo-local` mode for linked worktrees. If `extensions.worktreeConfig` is disabled, `true` allows fallback to shared `--local` include (affecting all worktrees); `false` blocks write and asks for explicit fix.
- `ssh.dynamicMatch`
  - Required: No
  - Default: `false`
  - Allowed: `true`, `false`
  - Notes: When enabled, generates SSH `Match exec "gpx ssh-eval ..."` blocks for directory-aware identity selection. Default is `false` to keep SSH behavior predictable and avoid extra per-connection command execution overhead.
- `rule.<name>.profile`
  - Required: Yes (per rule)
  - Default: N/A
  - Allowed: existing profile name
  - Notes: Target profile selected when this rule matches. Validation fails if the referenced profile does not exist.
- `rule.<name>.priority`
  - Required: No
  - Default: `0`
  - Allowed: integer, e.g. `200`, `180`, `-10`
  - Notes: Used only by `highest-score` mode. Larger values win before specificity checks; negative values are valid for low-priority fallback rules.
- `rule.<name>.match.path`
  - Required: Conditionally
  - Default: `<none>`
  - Allowed: glob path string, e.g. `~/code/company/**`
  - Notes: Matches against normalized absolute current working directory. This matcher works even outside a Git repo, so it is useful for early directory-based profile decisions.
- `rule.<name>.match.remoteHost`
  - Required: Conditionally
  - Default: `<none>`
  - Allowed: host string, e.g. `github.com`
  - Notes: Compared against host parsed from repository remotes (for example `github.com`, internal Git hostnames). Effective only when repository context and remotes are available.
- `rule.<name>.match.remoteOrg`
  - Required: Conditionally
  - Default: `<none>`
  - Allowed: org/user string, e.g. `corp-org`
  - Notes: Compared against org/user segment parsed from remote URL path. Useful for separating identities on the same host by organization namespace.
- `rule.<name>.match.fileExists`
  - Required: Conditionally
  - Default: `<none>`
  - Allowed: file name/path at repo root, e.g. `.gpx-personal`
  - Notes: Checks file existence at repository root (not arbitrary parent traversal). This is useful for explicit opt-in marker files committed per repo.
- Rule composition
  - Required: N/A
  - Default: N/A
  - Allowed: N/A
  - Notes: Each rule must define at least one `match.*`. If a rule defines multiple matchers, all must pass (logical AND); this lets one rule combine path, remote, and marker constraints precisely.

## Commands

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
gpx -- <git args...>      # gpx run -- <git args...>
```

## Behavior

- Supported rule conditions: `match.path`, `match.remoteHost`, `match.remoteOrg`, `match.fileExists`
- If one rule defines multiple `match.*` conditions, all of them must match
- Falls back to `core.defaultProfile` when no rule matches
- `list` shows configured `profiles` / `rules`; use `--json` for machine-readable output
- `run` mode does not write any config; it only injects environment variables for the current Git command
- Whether `--profile` can force override is controlled by `run.allowProfileOverride`

## Submodule and Worktree

- Submodules apply configuration independently based on each repository context
- In `repo-local` mode:
  - If `extensions.worktreeConfig` is enabled, write `--worktree include.path`
  - If it is disabled and `worktree.allowSharedFallback=false`, writes are rejected with remediation guidance
  - If `worktree.allowSharedFallback=true`, it falls back to shared `--local` (which affects all worktrees)

## Hook

- Shell hook: automatic scripts for `bash/zsh/fish`; integration templates for `nushell/tcsh/elvish`
- Git hook: installs `pre-commit`, `pre-push`, and `post-checkout`
- `hook.fixPolicy`:
  - `continue`: continue after fix
  - `abort-once`: exit this time after fix, then run Git action on next attempt

## SSH Dynamic Matching (Optional)

Disabled by default. After enabling `ssh.dynamicMatch=true`, `gpx apply` generates `Match exec "gpx ssh-eval ..."` blocks to select identities dynamically by current directory.
