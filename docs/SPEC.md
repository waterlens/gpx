# GPX Design Plan (Rust)

This is the full product/design specification.

- User docs: `README.md`
- Contributor/agent execution rules: `AGENTS.md`

## 1. Project Positioning

- Executable name: `gpx`
- Candidate full name:
  - `Git Profile Extension` (concise and intuitive)
- Core goal: Automatically switch Git and SSH identity configuration based on the active profile, preferring the include mechanism to avoid frequent direct rewrites of the main config file.
- Platform scope: Unix (Linux/macOS).

## 2. Design Principles

- Minimal intrusion: Prefer one-time bootstrap, then complete switching via include files and rule evaluation.
- Recoverable: All automatic rewrites must be traceable and reversible.
- Independent operation: Shell hooks and Git global hooks can be enabled independently or together.
- Low latency: Normal execution on hook paths should stay in the tens-of-milliseconds range.
- Explicit priority: Behavior must be deterministic and explainable when rules conflict.
- No daemon: Do not introduce a long-running daemon; use hooks and short-lived commands for state evaluation to avoid state contention across terminals.

## 3. XDG Paths and File Layout

Follow the XDG Base Directory specification:

- Primary config: `$XDG_CONFIG_HOME/gpx/config.toml` (default `~/.config/gpx/config.toml`, TOML syntax)
- Compatible config: `$XDG_CONFIG_HOME/gpx/config` (gitconfig/INI syntax, optional)
- Generated Git include: `$XDG_CACHE_HOME/gpx/git/profiles/<name>.gitconfig`
- Generated SSH include: `$XDG_CACHE_HOME/gpx/ssh/gpx_ssh.conf`
- Runtime state: `$XDG_STATE_HOME/gpx/state.toml`
- Logs (optional): `$XDG_STATE_HOME/gpx/logs/*.log`
- Hook scripts directory: `$XDG_CONFIG_HOME/gpx/hooks/`

Notes:

- `cache` stores reproducible files; `state` stores runtime state information.
- Recommended file permission for all sensitive files (including key path references): `0600`.

## 4. Config Format (`gitconfig` and `config.toml` Compatible)

Use TOML (`config.toml`) by default:

```toml
[core]
defaultProfile = "personal"
ruleMode = "first-match" # first-match | highest-score
mode = "global-active"   # global-active | repo-local

[profile.work.user]
name = "Alice Chen"
email = "alice@corp.com"
signingkey = "ABCDEF1234567890"

[profile.work.gpg]
format = "openpgp"

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

[rule.personal-file]
profile = "personal"
priority = 150
"match.fileExists" = ".gpx-personal"
```

Git-compatible INI syntax (`config`) is also supported, with semantics aligned to the TOML version, for example:

```ini
[core]
defaultProfile = personal
ruleMode = first-match ; first-match | highest-score

[profile "work"]
user.name = Alice Chen
user.email = alice@corp.com
user.signingkey = ABCDEF1234567890

[profile "personal"]
user.name = Alice
user.email = me@example.com
ssh.key = ~/.ssh/id_ed25519_personal

[rule "corp-path"]
profile = work
priority = 200
match.path = ~/code/company/**
```

Load order and constraints:

- If both `config` and `config.toml` exist, `config.toml` is preferred by default, and `doctor/status --verbose` should emit a hint to avoid ambiguity.
- Semantics of `profile/rule/hook/run` in `config` (INI) must map one-to-one with TOML semantics, without introducing extra behavior branches.

Constraints:

- `profile.*` supports available Git `user.*` items, plus extension fields `user.signingkey`, `gpg.*`, and `ssh.*`.
- A rule must contain at least one `match.*` condition.
- When no rule matches, fallback to `defaultProfile`; if undefined, return an actionable error.

Common config item reference (must stay in sync with README):

- `[core].defaultProfile`: Fallback profile when no rule matches; must be defined under `profile.<name>.*`.
- `[core].ruleMode`: Rule decision strategy.
  - `first-match`: Use declaration order and take the first matched rule.
  - `highest-score`: Compare `priority` first, then condition specificity.
- `[core].mode`: Write strategy for `gpx apply`.
  - `global-active`: Update the global active include (`~/.cache/gpx/git/active.gitconfig`).
  - `repo-local`: Write include into the current repository/worktree.
- `[profile.<name>.user]`: Git identity fields (e.g. `name`, `email`, `signingkey`) compiled into profile include files.
- `[profile.<name>.gpg]`: Optional `gpg.*` fields for the profile.
- `[profile.<name>.ssh].key`: SSH private key path for the profile (supports `~` expansion).
- `[profile.<name>.ssh].identitiesOnly`: When `true`, only explicitly configured keys are used, avoiding unrelated keys.
- `[hook].shell`: Whether to enable the shell-hook installation path (`gpx hook install --shell ...`).
- `[hook].git`: Whether to enable the git-hook installation path (`gpx hook install --git`).
- `[hook].fixPolicy`: Behavior after hook auto-fixes identity.
  - `continue`: Continue current Git action after fixing.
  - `abort-once`: Abort current action once after fixing; user retries manually.
- `[run].allowProfileOverride`: Controls whether `gpx run --profile <name> -- ...` may force a profile.
- `[worktree].allowSharedFallback`: In `repo-local` mode when `extensions.worktreeConfig` is disabled, controls fallback to shared `--local` include writes.
- `[ssh].dynamicMatch`: When `true`, generate dynamic SSH blocks using `Match exec "gpx ssh-eval ..."`; default off for predictability and performance.
- `[rule.<name>].profile`: Target profile when rule matches; must exist.
- `[rule.<name>].priority`: Priority used in `highest-score` mode (higher value means higher priority).
- `[rule.<name>].match.path`: Glob match against normalized absolute `cwd`.
- `[rule.<name>].match.remoteHost`: Match remote host (e.g. `github.com`).
- `[rule.<name>].match.remoteOrg`: Match org/user parsed from remote URL.
- `[rule.<name>].match.fileExists`: Check file existence at repository root (e.g. `.gpx-personal`).
- Combined rule conditions: Multiple `match.*` conditions in a single rule are logical AND and all must match.

Documentation synchronization requirement:

- If config items are added, removed, renamed, or semantics change, `README.md` `Config Reference` must be updated accordingly (including `Required` / `Default` / `Allowed`).
- Review passes only when config descriptions in both `AGENTS.md` and `README.md` are consistent; changing only one side is not allowed.

## 5. Core Command Design

```text
gpx init
gpx doctor
gpx status
gpx list [profiles|rules] [--json]
gpx check [--cwd <path>] [--json]
gpx apply [--cwd <path>] [--profile <name>] [--dry-run]
gpx hook install [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx hook uninstall [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx run [--profile <name>] -- <git args...>
gpx ssh-eval --profile <name> [--cwd <path>]   # internal; for Match exec
```

Behavior conventions:

- `gpx run -- <git args...>`: Evaluate rules in real time and execute Git with zero persistence (no disk writes).
- `gpx run --profile ... -- <git args...>`: Force a profile with zero persistence.
- `gpx -- <git args...>`: Alias of `gpx run -- <git args...>`, exactly identical semantics.
- `gpx list [profiles|rules]`: List defined profiles or rules; defaults to human-readable report, `--json` outputs machine-readable data.
- `gpx ssh-eval --profile <name>`: Internal command for SSH `Match exec`; returns exit code `0` when matched profile equals target, otherwise `1`.

## 6. Rule Engine Design

### 6.1 Input Context

- Current directory `cwd`
- Current repository root (if in a Git repository)
- Remotes (origin/upstream URLs, etc.)
- File probe results (whether specified marker files exist in repository)
- Optional environment-variable context (future extension)

### 6.2 Match Types

- path-based: `match.path` uses glob matching against normalized absolute path
- remote-url-based: parse remote URL and extract host/org/repo
- file-based: find specified files at repository root (extensible to upward search)

### 6.3 Evaluation Flow

1. Discover repository context (if not in repo, evaluate path rules only)
2. Precompute remote host/org (avoid repeated parsing)
3. Evaluate rules and get candidate set
4. Decide by strategy:
   - `first-match`: choose first matched rule by declaration order
   - `highest-score`: compare `priority` first, then score "more specific" conditions
5. Output `ResolvedProfile` and `reason` (for `status/doctor`)

### 6.4 Conflict Handling

- Multiple matched rules tied on score: report error with conflicting rule names and require priority adjustment.
- Non-existent profile: report config error and refuse apply.

## 7. Git Config Injection Strategy (Prefer Include)

### 7.1 One-Time Bootstrap

`gpx init` runs once:

- Add a stable include entry to `~/.gitconfig` (if missing), for example:

```ini
[include]
path = ~/.cache/gpx/git/active.gitconfig
```

- `active.gitconfig` is maintained by gpx and includes the concrete profile file:

```ini
[include]
path = ~/.cache/gpx/git/profiles/work.gitconfig
```

Advantages:

- Main config is not repeatedly rewritten.
- Switching only updates the target path in `active.gitconfig`.

### 7.2 Profile Compilation

Each profile is compiled into an independent file:

- `~/.cache/gpx/git/profiles/<profile>.gitconfig`
- Contains only keys for that profile (e.g. `user.name/user.email/user.signingkey/gpg.*`)

### 7.3 Repository-Level Override

For repository isolation, inject repo-local include into `.git/config` (optional mode):

- Default prefers global active include (simpler)
- When `mode = repo-local` is enabled, inject `include.path = ~/.cache/gpx/git/profiles/<profile>.gitconfig` per repository

## 8. SSH Config Injection Strategy (Include + Host Alias/Match)

Practical constraint: Native SSH is not cwd-aware, so static config alone cannot fully implement directory-driven identity switching.

Recommended implementation (stable approach):

- Add fixed include in `~/.ssh/config`:

```sshconfig
Include ~/.cache/gpx/ssh/gpx_ssh.conf
```

- `gpx_ssh.conf` contains segment generated from current profile:

```sshconfig
Host *
    IdentityFile ~/.ssh/id_ed25519_work
    IdentitiesOnly yes
```

- Refresh this file when profile switches.

Optional enhancement (advanced):

- Use `Match exec "gpx ssh-eval ..."` for dynamic evaluation, with strict performance and portability control.
- Disabled by default, available as experimental feature via `ssh.dynamicMatch = true`.
- If enabled, `gpx apply` generates profile-based blocks, for example:

```sshconfig
Match exec "gpx ssh-eval --profile 'work'"
    IdentityFile ~/.ssh/id_ed25519_work
    IdentitiesOnly yes
```

- `gpx ssh-eval` evaluates rules by current directory; returns `0` if resolved profile equals `--profile`, else `1`.

## 9. Shell Hook and Git Hook Coordination

### 9.1 Shell Hook (Proactive Switching)

- Bash/Zsh: trigger `gpx apply --cwd "$PWD"` via prompt hooks after directory changes
- Fish: trigger via `--on-variable PWD`
- Nushell: trigger via `env_change.PWD` hook
- Tcsh/Elvish: trigger via directory-change events (`chpwd/after-chdir`)
- Debounce: write only when repository root changes or path match result changes
- For shells without built-in adapter, `gpx hook install --shell <name>` outputs a generic script snippet that can be `source`d manually

Goal: Complete profile switching quickly after `cd`, covering the primary command-line path.

### 9.2 Git Global Hook (Fallback)

- Use `core.hooksPath = ~/.config/gpx/hooks`
- Trigger `gpx apply --cwd "$PWD" --hook-mode` in `pre-commit`/`pre-push`/`post-checkout`, etc.

Policy:

- Default "auto-fix and continue": ensure identity is correct before commit/push
- Configurable "auto-fix and abort once": stronger auditability
- Recommended team default is "auto-fix and abort once" to avoid continuing the current Git action at a boundary where identity was just corrected.

Independence:

- Shell hook only: smooth interactive flow, GUI clients may miss triggers
- Git hook only: no shell dependency, but switching is delayed until Git action
- Both enabled: recommended

## 10. Run Mode (Zero Persistence)

Syntax:

```bash
gpx [--profile <name>] -- <git command...>
```

Equivalent to:

```bash
gpx run [--profile <name>] -- <git command...>
```

Implementation:

- Do not modify `.gitconfig/.git/config/.ssh/config`
- Override Git config via environment variables:
  - `GIT_CONFIG_COUNT`
  - `GIT_CONFIG_KEY_n`
  - `GIT_CONFIG_VALUE_n`
- Inject temporary SSH key via `GIT_SSH_COMMAND`:
  - `ssh -i <key> -o IdentitiesOnly=yes`
- Finally run Git with process replacement via `exec` (Unix)
- Compatibility: `GIT_CONFIG_COUNT` approach depends on relatively new Git versions; `gpx doctor` must detect Git version and show downgrade guidance for older versions.

Result:

- Effects expire when command exits; no persistent side effects.

## 11. Submodule and Worktree

### 11.1 Submodule

- Submodules are independent repositories and should evaluate rules independently.
- Default behavior: when running Git inside submodule directory, match profile using submodule context.
- When parent repo triggers recursive commands (e.g. `git submodule foreach`), each submodule computes by its own `cwd`.

### 11.2 Worktree

Key point: multiple worktrees share part of the main repository config.

Strategy:

- Detect whether `extensions.worktreeConfig` is enabled
- If enabled: in `repo-local` mode, write `git config --worktree include.path ...` (independent profile per worktree)
- If not enabled:
  - In linked worktree, default to error and suggest enabling it
  - If `worktree.allowSharedFallback = true`, fallback to `git config --local include.path ...` (causes all worktrees to switch together)

Command recommendation:

- `gpx doctor` reports worktree risks and suggested remediation commands.

## 12. Rust Implementation Architecture

Recommended crates:

- CLI: `clap`
- Git config/parser: `gix-config`, `gix-discover`
- Path matching: `globset`
- URL parsing: `url`
- XDG: `directories`
- Serialization: `serde`, `serde_json`
- Error handling: `thiserror`, `anyhow`
- Logging: `tracing`, `tracing-subscriber`

Engineering constraints (must stay consistent in implementation):

- Rust formatting is managed persistently via `rustfmt.toml`, with 2-space indentation (`tab_spaces = 2`).
- Before commit, pass at least: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.
- Runtime code should avoid `unwrap/expect` where possible; use `anyhow` with `Context` for propagation, and use `thiserror` for domain error types.
- Handle final error exit in `main`, and use `owo-colors` for essential highlighting of key error messages.

Module layout:

- `src/main.rs`: entrypoint
- `src/cli.rs`: argument definitions
- `src/config/`: config loading, validation, profile compilation
- `src/rules/`: rule engine and matchers
- `src/gitops/`: git include injection, repo/worktree operations
- `src/sshops/`: ssh include generation and permission management
- `src/hooks/`: shell/git hook installation and script templates
- `src/run/`: env overrides and `exec`
- `src/state/`: state cache and idempotent updates
- `src/doctor/`: diagnostics

## 13. Performance and Concurrency

- Cache latest `cwd -> profile` result (state + memory)
- Use "write temp file + atomic rename" for file writes
- Add file locks to key write operations to avoid concurrent hook races
- Avoid scanning whole repo on every hook:
  - file-based rules check only explicitly declared files
  - remote parsing is cached at repository level

## 14. Security and Robustness

- Expand and existence-check `ssh.key` paths
- Enforce minimal permissions when writing SSH include
- Sanitize profile names and paths to prevent newline/special-character injection into config
- Record change summaries for all rewrites, and support auditing via `gpx status --verbose`

## 15. Testing and Acceptance

Test layers:

- Unit tests: rule matching, priority decisions, URL parsing
- Integration tests: real git repo/submodule/worktree scenarios in temporary directories
- End-to-end tests: hook installation, `cd` switching, commit signing identity verification

Acceptance checklist:

- path/remote/file rule types match correctly
- Shell Hook and Git Hook can be enabled independently without dependency on each other
- run mode leaves no config residue after execution
- submodule and worktree behavior follows expected strategy
- repeated profile switching does not create duplicate include entries or dirty config

## 16. Current Capability List

- profile/rule parsing and validation (TOML/INI compatible)
- `check/apply/status/list/run/doctor` commands available
- Git include injection (global active include + repo-local/worktree strategy)
- SSH include static refresh and optional dynamic matching via `ssh.dynamicMatch`
- shell hook (bash/zsh/fish) and git hook (pre-commit/pre-push/post-checkout) install/uninstall
- hook fix policy (`continue` / `abort-once`) and state audit output
- submodule independent evaluation and linked-worktree risk control

## 17. Key Trade-off Conclusions

- On Git side, prioritize the "global stable entry + active include" model to reduce risks from frequent main-config rewrites.
- On SSH side, default to "fixed include + rewrite target segment on switch" for predictability and performance.
- Hooks use "dual-path complement with independent enablement" to support both CLI and GUI users.
- `run` uses environment variables and `exec` to achieve true zero persistence.
- submodule/worktree are first-class citizens in rule and write strategies, avoiding later rework.

## 18. Unified Output Specification

- All human-readable CLI output must use one unified format:
  - Title line: `<Title> report` (bold)
  - Item line: `- <Label>: <Value>`
  - Note line: `- <Message>`
- Commands like `doctor/status/init/list/check/hook/apply --dry-run` must follow the same output style; no command may use inconsistent prefixes or scattered wording.
- Unified status coloring:
  - Success/normal: `OK`/`ENABLED`/`CREATED`/`UPDATED`/`INSTALLED`, etc. in green or cyan (informational color)
  - Warning/default/missing: `WARN`/`UNSET`/`<none>`/`MISSING`, etc. in yellow
  - Failure: `FAIL`/`INVALID` in red
- Output implementation should reuse unified helpers (for example `src/output.rs` `section/item/note/ok/warn/fail/info`) to avoid duplicated color/format logic across commands.
- Machine-readable outputs such as `--json` must not use the human-readable format above and should remain clean JSON.
- Default log level must not interfere with the output structure above (avoid mixing non-report INFO logs into stdout).
