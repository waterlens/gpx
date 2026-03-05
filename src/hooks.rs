use crate::cli::{HookCommands, Shell};
use crate::config::AppContext;
use crate::output::{info, item, warn};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const MANAGED_SHELL_HOOK_BEGIN: &str = "# >>> gpx managed shell hook >>>";
const MANAGED_SHELL_HOOK_END: &str = "# <<< gpx managed shell hook <<<";

#[derive(Clone, Copy)]
enum ManagedWriteStatus {
  Updated,
  Ok,
}

#[derive(Clone, Copy)]
enum ManagedRemoveStatus {
  Updated,
  Missing,
}

pub fn handle(ctx: &AppContext, command: HookCommands) -> Result<()> {
  match command {
    HookCommands::Install { shell, git } => {
      if let Some(s) = shell {
        install_shell_hook(ctx, s)?;
      }
      if git {
        install_git_hooks(ctx)?;
      }
    }
    HookCommands::Uninstall { shell, git } => {
      if let Some(s) = shell {
        uninstall_shell_hook(ctx, s)?;
      }
      if git {
        uninstall_git_hooks(ctx)?;
      }
    }
  }
  Ok(())
}

fn install_shell_hook(ctx: &AppContext, shell: Shell) -> Result<()> {
  let shell_dir = ctx.config_dir.join("hooks").join("shell");
  std::fs::create_dir_all(&shell_dir)?;

  match shell {
    Shell::Bash => {
      let path = shell_dir.join("gpx-bash.hook.sh");
      std::fs::write(&path, bash_hook_script())?;
      let startup = bash_startup_file()?;
      let source = format!(
        "[ -f \"{}\" ] && source \"{}\"",
        path.display(),
        path.display()
      );
      let status = upsert_shell_managed_block(&startup, &source)?;
      item("Shell hook", format!("{} (bash)", info("INSTALLED")));
      item("Hook script", path.display());
      item(
        "Shell startup file",
        format!("{} ({})", status_label(status), startup.display()),
      );
    }
    Shell::Zsh => {
      let path = shell_dir.join("gpx-zsh.hook.sh");
      std::fs::write(&path, zsh_hook_script())?;
      let startup = zsh_startup_file()?;
      let source = format!(
        "[ -f \"{}\" ] && source \"{}\"",
        path.display(),
        path.display()
      );
      let status = upsert_shell_managed_block(&startup, &source)?;
      item("Shell hook", format!("{} (zsh)", info("INSTALLED")));
      item("Hook script", path.display());
      item(
        "Shell startup file",
        format!("{} ({})", status_label(status), startup.display()),
      );
    }
    Shell::Fish => {
      let path = shell_dir.join("gpx-fish.hook.fish");
      std::fs::write(&path, fish_hook_script())?;
      let startup = fish_startup_file(ctx)?;
      let source = format!("test -f {} ; and source {}", path.display(), path.display());
      let status = upsert_shell_managed_block(&startup, &source)?;
      item("Shell hook", format!("{} (fish)", info("INSTALLED")));
      item("Hook script", path.display());
      item(
        "Shell startup file",
        format!("{} ({})", status_label(status), startup.display()),
      );
    }
    Shell::Nushell => {
      let path = shell_dir.join("gpx-nushell.hook.nu");
      std::fs::write(&path, nushell_hook_script())?;
      let startup = nushell_startup_file(ctx)?;
      let source = format!("source \"{}\"", path.display());
      let status = upsert_shell_managed_block(&startup, &source)?;
      item("Shell hook", format!("{} (nushell)", info("INSTALLED")));
      item("Hook script", path.display());
      item(
        "Shell startup file",
        format!("{} ({})", status_label(status), startup.display()),
      );
    }
    Shell::Tcsh => {
      let path = shell_dir.join("gpx-tcsh.hook.csh");
      std::fs::write(&path, tcsh_hook_script())?;
      let startup = tcsh_startup_file()?;
      let source = format!("source \"{}\"", path.display());
      let status = upsert_shell_managed_block(&startup, &source)?;
      item("Shell hook", format!("{} (tcsh)", info("INSTALLED")));
      item("Hook script", path.display());
      item(
        "Shell startup file",
        format!("{} ({})", status_label(status), startup.display()),
      );
    }
    Shell::Elvish => {
      let path = shell_dir.join("gpx-elvish.hook.elv");
      std::fs::write(&path, elvish_hook_script())?;
      let startup = elvish_startup_file(ctx)?;
      let source = format!("source \"{}\"", path.display());
      let status = upsert_shell_managed_block(&startup, &source)?;
      item("Shell hook", format!("{} (elvish)", info("INSTALLED")));
      item("Hook script", path.display());
      item(
        "Shell startup file",
        format!("{} ({})", status_label(status), startup.display()),
      );
    }
  }
  Ok(())
}

fn install_git_hooks(ctx: &AppContext) -> Result<()> {
  let hooks_dir = ctx.config_dir.join("hooks").join("git");
  std::fs::create_dir_all(&hooks_dir)?;

  write_executable(hooks_dir.join("pre-commit"), &git_hook_script("pre-commit"))?;
  write_executable(hooks_dir.join("pre-push"), &git_hook_script("pre-push"))?;
  write_executable(
    hooks_dir.join("post-checkout"),
    &git_hook_script("post-checkout"),
  )?;

  let status = Command::new("git")
    .args(["config", "--global", "core.hooksPath"])
    .arg(&hooks_dir)
    .status()?;
  if !status.success() {
    anyhow::bail!("Failed to set global core.hooksPath");
  }
  item(
    "Git hooks",
    format!("{} ({})", info("INSTALLED"), hooks_dir.display()),
  );
  item(
    "Hook behavior",
    "follows hook.fixPolicy in config (continue or abort-once)",
  );
  Ok(())
}

fn uninstall_shell_hook(ctx: &AppContext, shell: Shell) -> Result<()> {
  let shell_dir = ctx.config_dir.join("hooks").join("shell");
  let path = match shell {
    Shell::Bash => shell_dir.join("gpx-bash.hook.sh"),
    Shell::Zsh => shell_dir.join("gpx-zsh.hook.sh"),
    Shell::Fish => shell_dir.join("gpx-fish.hook.fish"),
    Shell::Nushell => shell_dir.join("gpx-nushell.hook.nu"),
    Shell::Tcsh => shell_dir.join("gpx-tcsh.hook.csh"),
    Shell::Elvish => shell_dir.join("gpx-elvish.hook.elv"),
  };

  if path.exists() {
    std::fs::remove_file(&path)?;
    item(
      "Shell hook",
      format!("{} ({})", info("REMOVED"), path.display()),
    );
  } else {
    item(
      "Shell hook",
      format!("{} ({})", warn("MISSING"), path.display()),
    );
  }

  match shell {
    Shell::Bash => {
      let startup = bash_startup_file()?;
      let status = remove_shell_managed_block(&startup)?;
      item(
        "Shell startup file",
        format!("{} ({})", remove_status_label(status), startup.display()),
      );
    }
    Shell::Zsh => {
      let startup = zsh_startup_file()?;
      let status = remove_shell_managed_block(&startup)?;
      item(
        "Shell startup file",
        format!("{} ({})", remove_status_label(status), startup.display()),
      );
    }
    Shell::Fish => {
      let startup = fish_startup_file(ctx)?;
      let status = remove_shell_managed_block(&startup)?;
      item(
        "Shell startup file",
        format!("{} ({})", remove_status_label(status), startup.display()),
      );
    }
    Shell::Nushell => {
      let startup = nushell_startup_file(ctx)?;
      let status = remove_shell_managed_block(&startup)?;
      item(
        "Shell startup file",
        format!("{} ({})", remove_status_label(status), startup.display()),
      );
    }
    Shell::Tcsh => {
      let startup = tcsh_startup_file()?;
      let status = remove_shell_managed_block(&startup)?;
      item(
        "Shell startup file",
        format!("{} ({})", remove_status_label(status), startup.display()),
      );
    }
    Shell::Elvish => {
      let startup = elvish_startup_file(ctx)?;
      let status = remove_shell_managed_block(&startup)?;
      item(
        "Shell startup file",
        format!("{} ({})", remove_status_label(status), startup.display()),
      );
    }
  }
  Ok(())
}

fn uninstall_git_hooks(ctx: &AppContext) -> Result<()> {
  let hooks_dir = ctx.config_dir.join("hooks").join("git");
  for file in ["pre-commit", "pre-push", "post-checkout"] {
    let path = hooks_dir.join(file);
    if path.exists() {
      std::fs::remove_file(path)?;
    }
  }

  let current = Command::new("git")
    .args(["config", "--global", "--get", "core.hooksPath"])
    .output()?;
  if current.status.success() {
    let value = String::from_utf8_lossy(&current.stdout).trim().to_string();
    if value == hooks_dir.display().to_string() {
      let _ = Command::new("git")
        .args(["config", "--global", "--unset", "core.hooksPath"])
        .status()?;
    }
  }
  item(
    "Git hooks",
    format!("{} ({})", info("UNINSTALLED"), hooks_dir.display()),
  );
  Ok(())
}

fn home_dir() -> Result<PathBuf> {
  let user_dirs = directories::UserDirs::new().context("Could not find user directories")?;
  Ok(user_dirs.home_dir().to_path_buf())
}

fn bash_startup_file() -> Result<PathBuf> {
  Ok(home_dir()?.join(".bashrc"))
}

fn zsh_startup_file() -> Result<PathBuf> {
  Ok(home_dir()?.join(".zshrc"))
}

fn fish_startup_file(ctx: &AppContext) -> Result<PathBuf> {
  let xdg_config_home = match ctx.config_dir.parent() {
    Some(parent) => parent.to_path_buf(),
    None => home_dir()?.join(".config"),
  };
  Ok(xdg_config_home.join("fish").join("config.fish"))
}

fn nushell_startup_file(ctx: &AppContext) -> Result<PathBuf> {
  let xdg_config_home = match ctx.config_dir.parent() {
    Some(parent) => parent.to_path_buf(),
    None => home_dir()?.join(".config"),
  };
  Ok(xdg_config_home.join("nushell").join("config.nu"))
}

fn tcsh_startup_file() -> Result<PathBuf> {
  Ok(home_dir()?.join(".tcshrc"))
}

fn elvish_startup_file(ctx: &AppContext) -> Result<PathBuf> {
  let xdg_config_home = match ctx.config_dir.parent() {
    Some(parent) => parent.to_path_buf(),
    None => home_dir()?.join(".config"),
  };
  Ok(xdg_config_home.join("elvish").join("rc.elv"))
}

fn render_shell_block(source_line: &str) -> String {
  format!("{MANAGED_SHELL_HOOK_BEGIN}\n{source_line}\n{MANAGED_SHELL_HOOK_END}\n")
}

fn shell_block_range(content: &str) -> Option<(usize, usize)> {
  let start = content.find(MANAGED_SHELL_HOOK_BEGIN)?;
  let end_marker_start = content[start..].find(MANAGED_SHELL_HOOK_END)? + start;
  let end_idx = content[end_marker_start..]
    .find('\n')
    .map(|idx| end_marker_start + idx + 1)
    .unwrap_or(content.len());
  Some((start, end_idx))
}

fn upsert_shell_managed_block(path: &Path, source_line: &str) -> Result<ManagedWriteStatus> {
  let block = render_shell_block(source_line);
  if !path.exists() {
    write_text(path, &block)?;
    return Ok(ManagedWriteStatus::Updated);
  }

  let content = std::fs::read_to_string(path)?;
  if let Some((start, end)) = shell_block_range(&content) {
    if &content[start..end] == block.as_str() {
      return Ok(ManagedWriteStatus::Ok);
    }
    let mut updated = content;
    updated.replace_range(start..end, &block);
    write_text(path, &updated)?;
    return Ok(ManagedWriteStatus::Updated);
  }

  let mut updated = content;
  if !updated.is_empty() && !updated.ends_with('\n') {
    updated.push('\n');
  }
  updated.push_str(&block);
  write_text(path, &updated)?;
  Ok(ManagedWriteStatus::Updated)
}

fn remove_shell_managed_block(path: &Path) -> Result<ManagedRemoveStatus> {
  if !path.exists() {
    return Ok(ManagedRemoveStatus::Missing);
  }

  let content = std::fs::read_to_string(path)?;
  let Some((start, end)) = shell_block_range(&content) else {
    return Ok(ManagedRemoveStatus::Missing);
  };

  let mut updated = content;
  updated.replace_range(start..end, "");
  let normalized = normalize_after_block_removal(updated);
  write_text(path, &normalized)?;
  Ok(ManagedRemoveStatus::Updated)
}

fn normalize_after_block_removal(mut content: String) -> String {
  while content.contains("\n\n\n") {
    content = content.replace("\n\n\n", "\n\n");
  }
  while content.starts_with('\n') {
    content.remove(0);
  }
  content
}

fn write_text(path: &Path, content: &str) -> Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(path, content)?;
  Ok(())
}

fn status_label(status: ManagedWriteStatus) -> String {
  match status {
    ManagedWriteStatus::Updated => info("UPDATED"),
    ManagedWriteStatus::Ok => info("OK"),
  }
}

fn remove_status_label(status: ManagedRemoveStatus) -> String {
  match status {
    ManagedRemoveStatus::Updated => info("UPDATED"),
    ManagedRemoveStatus::Missing => warn("MISSING"),
  }
}

fn bash_hook_script() -> &'static str {
  r#"_gpx_hook() {
  local previous_exit_status=$?
  gpx apply --cwd "$PWD" --hook-mode >/dev/null 2>&1 || true
  return $previous_exit_status
}
if [[ ! "$PROMPT_COMMAND" == *_gpx_hook* ]]; then
  PROMPT_COMMAND="_gpx_hook;$PROMPT_COMMAND"
fi
"#
}

fn zsh_hook_script() -> &'static str {
  r#"_gpx_hook() {
  local previous_exit_status=$?
  gpx apply --cwd "$PWD" --hook-mode >/dev/null 2>&1 || true
  return $previous_exit_status
}
if [[ ! "$precmd_functions" == *_gpx_hook* ]]; then
  precmd_functions+=(_gpx_hook)
fi
"#
}

fn fish_hook_script() -> &'static str {
  r#"function _gpx_hook --on-variable PWD
    gpx apply --cwd "$PWD" --hook-mode >/dev/null 2>&1
end
"#
}

fn nushell_hook_script() -> &'static str {
  r#"# gpx managed hook for nushell
$env.config = ($env.config | upsert hooks.env_change.PWD (
  ($env.config.hooks.env_change.PWD? | default [] | append { |before, after|
    ^gpx apply --cwd $after --hook-mode | ignore
  })
))
"#
}

fn tcsh_hook_script() -> &'static str {
  r#"# gpx managed hook for tcsh
alias cwdcmd 'gpx apply --cwd "$cwd" --hook-mode >& /dev/null'
"#
}

fn elvish_hook_script() -> &'static str {
  r#"# gpx managed hook for elvish
fn gpx:after-chdir {|@_| 
  gpx apply --cwd $pwd --hook-mode > /dev/null 2> /dev/null
}
set after-chdir = (conj $after-chdir $gpx:after-chdir)
"#
}

fn write_executable(path: PathBuf, content: &str) -> Result<()> {
  std::fs::write(&path, content)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms)?;
  }
  Ok(())
}

fn git_hook_script(hook_name: &str) -> String {
  format!(
    "#!/usr/bin/env sh\n# gpx managed hook: {}\nexec gpx apply --cwd \"$PWD\" --hook-mode\n",
    hook_name
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_upsert_shell_block_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".zshrc");
    let source = "[ -f \"/tmp/gpx-zsh.hook.sh\" ] && source \"/tmp/gpx-zsh.hook.sh\"";

    let first = upsert_shell_managed_block(&rc, source).unwrap();
    let second = upsert_shell_managed_block(&rc, source).unwrap();
    assert!(matches!(first, ManagedWriteStatus::Updated));
    assert!(matches!(second, ManagedWriteStatus::Ok));
  }

  #[test]
  fn test_remove_shell_block_only_removes_managed_section() {
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".bashrc");
    let before = "export PATH=/usr/local/bin:$PATH\n";
    let source = "[ -f \"/tmp/gpx-bash.hook.sh\" ] && source \"/tmp/gpx-bash.hook.sh\"";
    let block = render_shell_block(source);
    write_text(&rc, &format!("{}{}", before, block)).unwrap();

    let status = remove_shell_managed_block(&rc).unwrap();
    let content = std::fs::read_to_string(&rc).unwrap();
    assert!(matches!(status, ManagedRemoveStatus::Updated));
    assert_eq!(content, before);
  }
}
