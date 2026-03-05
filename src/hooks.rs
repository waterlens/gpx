use crate::cli::{HookCommands, Shell};
use crate::config::AppContext;
use crate::output::{info, item, warn};
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

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
      item("Shell hook", format!("{} (bash)", info("INSTALLED")));
      item("Hook script", path.display());
      item("Source hint", format!("source {}", path.display()));
    }
    Shell::Zsh => {
      let path = shell_dir.join("gpx-zsh.hook.sh");
      std::fs::write(&path, zsh_hook_script())?;
      item("Shell hook", format!("{} (zsh)", info("INSTALLED")));
      item("Hook script", path.display());
      item("Source hint", format!("source {}", path.display()));
    }
    Shell::Fish => {
      let path = shell_dir.join("gpx-fish.hook.fish");
      std::fs::write(&path, fish_hook_script())?;
      item("Shell hook", format!("{} (fish)", info("INSTALLED")));
      item("Hook script", path.display());
      item("Source hint", format!("source {}", path.display()));
    }
    Shell::Nushell | Shell::Tcsh | Shell::Elvish => {
      let path = shell_dir.join(format!("gpx-{}.hook.txt", shell_name(&shell)));
      let content = format!(
        "Manual integration for {}:\nRun `gpx apply --cwd \"$PWD\" --hook-mode` after directory changes.\n",
        shell_name(&shell)
      );
      std::fs::write(&path, content)?;
      item(
        "Shell hook template",
        format!("{} ({})", warn("MANUAL"), shell_name(&shell)),
      );
      item("Template path", path.display());
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
    Shell::Nushell | Shell::Tcsh | Shell::Elvish => {
      shell_dir.join(format!("gpx-{}.hook.txt", shell_name(&shell)))
    }
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

fn shell_name(shell: &Shell) -> &'static str {
  match shell {
    Shell::Bash => "bash",
    Shell::Zsh => "zsh",
    Shell::Fish => "fish",
    Shell::Nushell => "nushell",
    Shell::Tcsh => "tcsh",
    Shell::Elvish => "elvish",
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
