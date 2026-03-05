# gpx

`gpx` (Git Profile Extension) helps you switch Git/SSH identities between projects.

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

## What You Can Do

- Initialize GPX files and bootstrap config:

  ```bash
  gpx init
  ```

- Remove GPX-managed bootstrap/cache/state when needed (and prune empty GPX root dirs):

  ```bash
  gpx deinit
  ```

- Inspect which profile matches the current directory:

  ```bash
  gpx check
  ```

- Apply the selected profile now:

  ```bash
  gpx apply
  ```

- See current status and diagnostics:

  ```bash
  gpx status --verbose
  gpx doctor
  ```

- List available profiles/rules:

  ```bash
  gpx list profiles
  gpx list rules
  ```

- Run one Git command with a profile without persistent changes:

  ```bash
  gpx run --profile work -- git fetch
  gpx -- git status
  ```

## Config Workflow

- Run `gpx init` first.
- If no config exists, GPX creates a commented template `config.toml` for you.
- Edit `config.toml` to define your profiles and matching rules.
- Run `gpx check` and `gpx apply` to verify and activate.

## Config Reference

- `[core].defaultProfile`: Required `no` (but required if no rule can match); Default `unset`; Allowed `profile name`.
- `[core].ruleMode`: Required `no`; Default `first-match`; Allowed `first-match | highest-score`.
- `[core].mode`: Required `no`; Default `global-active`; Allowed `global-active | repo-local`.
- `[hook].fixPolicy`: Required `no`; Default `continue`; Allowed `continue | abort-once`.
- `[run].allowProfileOverride`: Required `no`; Default `false`; Allowed `true | false`.
- `[worktree].allowSharedFallback`: Required `no`; Default `false`; Allowed `true | false`.
- `[ssh].dynamicMatch`: Required `no`; Default `false`; Allowed `true | false`.
- `[rule.<name>]`: Required `yes` for automatic switching; `profile` is required and at least one matcher is required from `match.path | match.remoteHost | match.remoteOrg | match.fileExists`.

Hook installation is command-driven (`gpx hook install/uninstall ...`) and no longer uses config toggles like `hook.shell` or `hook.git`.

## Hooks

You can enable automatic profile updates with shell and/or Git hooks.

- Install shell hook:

  ```bash
  gpx hook install --shell zsh
  ```

- Install Git hook:

  ```bash
  gpx hook install --git
  ```

- Install both:

  ```bash
  gpx hook install --shell zsh --git
  ```

- Uninstall hooks:

  ```bash
  gpx hook uninstall --shell zsh
  gpx hook uninstall --git
  ```

Supported shells: `bash`, `zsh`, `fish`, `nushell`, `tcsh`, `elvish`.

## Command List

```text
gpx init
gpx deinit
gpx doctor
gpx status [--verbose]
gpx list [profiles|rules] [--json]
gpx check [--cwd <path>] [--json]
gpx apply [--cwd <path>] [--profile <name>] [--dry-run]
gpx hook install [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx hook uninstall [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx run [--profile <name>] -- <git args...>
gpx -- <git args...>      # alias of gpx run --
```

## Detailed Spec

For full config reference and behavior details, see `docs/SPEC.md`.
