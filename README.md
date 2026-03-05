# gpx

`gpx`（Git Profile Extension）用于在不同仓库/目录间自动切换 Git 与 SSH 身份。

## 适用范围

- Unix 平台（macOS / Linux）
- 需要在个人/公司、多账号仓库之间切换身份

## 安装与构建

```bash
cargo build --release
./target/release/gpx --help
```

## 快速开始

- 初始化 include 入口：

  ```bash
  gpx init
  ```

- 在配置目录写入 `config.toml`（见下方示例）。

- 检查当前目录适用的规则：

  ```bash
  gpx check
  ```

- 应用当前 profile：

  ```bash
  gpx apply
  ```

- 检查状态与诊断：

  ```bash
  gpx status --verbose
  gpx doctor
  ```

## 配置文件路径

- 配置：`$XDG_CONFIG_HOME/gpx/config.toml`（默认 `~/.config/gpx/config.toml`）
- 兼容的 `.gitconfig` 风格配置，优先级低于 `.toml` 配置：`$XDG_CONFIG_HOME/gpx/config`
- Git 配置缓存：`$XDG_CACHE_HOME/gpx/git/...`
- SSH 配置缓存：`$XDG_CACHE_HOME/gpx/ssh/gpx_ssh.conf`
- 状态：`$XDG_STATE_HOME/gpx/state.toml`

## 配置示例

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

## 命令

```text
gpx init
gpx doctor
gpx status [--verbose]
gpx check [--cwd <path>] [--json]
gpx apply [--cwd <path>] [--profile <name>] [--dry-run]
gpx hook install [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx hook uninstall [--shell bash|zsh|fish|nushell|tcsh|elvish] [--git]
gpx run [--profile <name>] -- <git args...>
gpx -- <git args...>      # gpx run -- <git args...>
```

## 行为说明

- 规则支持：`match.path`、`match.remoteHost`、`match.remoteOrg`、`match.fileExists`
- 一条规则配置多个 `match.*` 时，需全部满足才算命中
- 未命中规则时回退 `core.defaultProfile`
- `run` 模式下不写入配置，只对当前 Git 命令注入环境变量
- `--profile` 是否允许强制覆盖由 `run.allowProfileOverride` 控制

## Submodule 与 Worktree

- 子模块按各自仓库上下文独立进行配置应用
- `repo-local` 模式下：
  - 若启用 `extensions.worktreeConfig`，写入 `--worktree include.path`
  - 若未启用且 `worktree.allowSharedFallback=false`，拒绝写入并给出修复建议
  - 若 `worktree.allowSharedFallback=true`，回退到共享 `--local`（会联动所有 worktree）

## Hook

- Shell hook：支持 `bash/zsh/fish` 自动脚本；`nushell/tcsh/elvish` 生成接入模板
- Git hook：安装 `pre-commit`、`pre-push`、`post-checkout`
- `hook.fixPolicy`：
  - `continue`：修复后继续
  - `abort-once`：修复后本次退出，下一次再执行 Git 动作

## SSH 动态匹配（可选）

默认关闭。开启 `ssh.dynamicMatch=true` 后，`gpx apply` 会生成 `Match exec "gpx ssh-eval ..."` 段按当前目录动态选身份。
