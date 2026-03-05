use crate::config::{AppContext, ApplyMode, Config, HookFixPolicy, Profile, SshConfig};
use crate::error::GpxError;
use crate::output::{info, item, section, warn};
use crate::rules::{gather_context, resolve_profile_detailed};
use crate::state;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn init(ctx: &AppContext) -> Result<()> {
  section("Init report");
  ctx.create_dirs()?;

  let gitconfig_path = directories::UserDirs::new()
    .context("Could not find user directories")?
    .home_dir()
    .join(".gitconfig");

  let include_path = ctx.git_active_include();
  let include_line = format!("[include]\n    path = {}", include_path.to_string_lossy());

  let include_status = if gitconfig_path.exists() {
    let content = std::fs::read_to_string(&gitconfig_path)?;
    if !content.contains(&include_path.to_string_lossy().to_string()) {
      let mut new_content = content;
      if !new_content.ends_with('\n') {
        new_content.push('\n');
      }
      new_content.push_str(&include_line);
      new_content.push('\n');
      std::fs::write(&gitconfig_path, new_content)?;
      tracing::info!("Added GPX include to ~/.gitconfig");
      format!("{} (added to existing ~/.gitconfig)", info("UPDATED"))
    } else {
      tracing::info!("GPX include already exists in ~/.gitconfig");
      format!("{} (already present)", info("OK"))
    }
  } else {
    std::fs::write(&gitconfig_path, include_line)?;
    tracing::info!("Created ~/.gitconfig with GPX include");
    format!("{} (~/.gitconfig created)", info("CREATED"))
  };

  item("~/.gitconfig include", include_status);
  item(
    "Active include path",
    info(&include_path.display().to_string()),
  );

  Ok(())
}

pub fn apply(
  ctx: &AppContext,
  cwd: Option<PathBuf>,
  profile_name: Option<String>,
  dry_run: bool,
  hook_mode: bool,
) -> Result<()> {
  ctx.create_dirs()?;
  let _lock = apply_lock(ctx)?;
  let cwd = match cwd {
    Some(path) => path,
    None => std::env::current_dir().map_err(|_| GpxError::ResolveCurrentDir)?,
  };
  let config = ctx.load_config()?;
  config.validate()?;

  let mut matched_rule: Option<String> = None;
  let mut reason = String::from("Explicit profile override");
  let rule_ctx = gather_context(&cwd)?;

  let profile_name = if let Some(name) = profile_name {
    Some(name)
  } else {
    let resolution = resolve_profile_detailed(&rule_ctx, &config)?;
    matched_rule = resolution.matched_rule;
    reason = resolution.reason;
    resolution.resolved_profile
  };

  let profile_name = match profile_name {
    Some(name) => name,
    None => {
      tracing::warn!("No profile matched and no default profile set.");
      return Ok(());
    }
  };

  let profile = config
    .profile
    .get(&profile_name)
    .context(format!("Profile '{}' not found", profile_name))?;

  if dry_run {
    item(
      "Apply",
      format!("{} profile '{}'", warn("DRY-RUN"), info(&profile_name)),
    );
    return Ok(());
  }

  // 1. Generate profile gitconfig
  let profile_gitconfig_path = ctx
    .git_profiles_dir()
    .join(format!("{}.gitconfig", profile_name));
  let profile_content = generate_git_config(profile);
  atomic_write(&profile_gitconfig_path, &profile_content)?;

  // 2. Apply include strategy
  match config.core.mode {
    ApplyMode::GlobalActive => {
      let active_gitconfig_path = ctx.git_active_include();
      let active_content = format!(
        "[include]\n    path = {}\n",
        profile_gitconfig_path.to_string_lossy()
      );
      atomic_write(&active_gitconfig_path, &active_content)?;
    }
    ApplyMode::RepoLocal => {
      apply_repo_local_include(
        &cwd,
        &profile_gitconfig_path,
        rule_ctx.repo_root.is_some(),
        config.worktree.allow_shared_fallback,
      )?;
    }
  }

  // 3. Refresh SSH include (always rewritten to avoid stale identity state)
  let ssh_config_path = ctx.ssh_include_file();
  let ssh_content = render_ssh_include(&config, profile.ssh.as_ref());
  atomic_write(&ssh_config_path, &ssh_content)?;

  // SSH include needs 0600
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&ssh_config_path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&ssh_config_path, perms)?;
  }

  tracing::info!("Applied profile '{}'", profile_name);

  let previous_state = state::load_state(ctx).unwrap_or_default();
  let change_summary = build_change_summary(previous_state.last_profile.as_deref(), &profile_name);

  state::record_apply(
    ctx,
    &profile_name,
    matched_rule.as_deref(),
    &reason,
    Some(&change_summary),
    &cwd,
  )?;

  if hook_mode
    && config.hook.fix_policy == HookFixPolicy::AbortOnce
    && previous_state.last_profile.as_deref() != Some(profile_name.as_str())
  {
    anyhow::bail!(
      "Profile fixed to '{}' in hook-mode; aborting once due to hook.fixPolicy=abort-once",
      profile_name
    );
  }

  Ok(())
}

fn build_change_summary(previous_profile: Option<&str>, current_profile: &str) -> String {
  match previous_profile {
    Some(prev) if prev == current_profile => format!("Profile unchanged: {}", current_profile),
    Some(prev) => format!("Profile switched: {} -> {}", prev, current_profile),
    None => format!("Profile initialized: {}", current_profile),
  }
}

fn generate_git_config(profile: &Profile) -> String {
  let mut content = String::new();

  if let Some(ref user) = profile.user {
    content.push_str("[user]\n");
    if let Some(ref name) = user.name {
      content.push_str(&format!("    name = {}\n", name));
    }
    if let Some(ref email) = user.email {
      content.push_str(&format!("    email = {}\n", email));
    }
    if let Some(ref signingkey) = user.signingkey {
      content.push_str(&format!("    signingkey = {}\n", signingkey));
    }
  }

  if let Some(ref gpg) = profile.gpg {
    content.push_str("[gpg]\n");
    if let Some(ref format) = gpg.format {
      content.push_str(&format!("    format = {}\n", format));
    }
  }

  content
}

fn render_ssh_include(config: &Config, ssh: Option<&SshConfig>) -> String {
  if config.ssh.dynamic_match {
    return render_dynamic_ssh_include(config);
  }

  let Some(ssh) = ssh else {
    return String::new();
  };

  let mut ssh_content = String::from("Host *\n");
  if let Some(ref key) = ssh.key {
    ssh_content.push_str(&format!("    IdentityFile {}\n", key));
  }
  if ssh.identities_only {
    ssh_content.push_str("    IdentitiesOnly yes\n");
  }
  ssh_content
}

fn render_dynamic_ssh_include(config: &Config) -> String {
  let mut out = String::from("# gpx dynamic ssh profile selection (experimental)\n");
  for (name, profile) in &config.profile {
    let Some(ssh) = profile.ssh.as_ref() else {
      continue;
    };
    out.push_str(&format!(
      "Match exec \"gpx ssh-eval --profile {}\"\n",
      shell_quote_single(name)
    ));
    if let Some(ref key) = ssh.key {
      out.push_str(&format!("    IdentityFile {}\n", key));
    }
    if ssh.identities_only {
      out.push_str("    IdentitiesOnly yes\n");
    }
    out.push('\n');
  }
  out
}

fn shell_quote_single(input: &str) -> String {
  format!("'{}'", input.replace('\'', r"'\''"))
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
  let parent = path
    .parent()
    .ok_or_else(|| GpxError::MissingParent(path.display().to_string()))?;
  let mut temp = tempfile::NamedTempFile::new_in(parent)?;
  use std::io::Write;
  temp.write_all(content.as_bytes())?;
  temp.persist(path)?;
  Ok(())
}

struct FileLockGuard {
  path: PathBuf,
}

impl Drop for FileLockGuard {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.path);
  }
}

fn apply_lock(ctx: &AppContext) -> Result<FileLockGuard> {
  let lock_path = ctx.state_dir.join("apply.lock");
  std::fs::create_dir_all(&ctx.state_dir)?;

  let file = std::fs::OpenOptions::new()
    .create_new(true)
    .write(true)
    .open(&lock_path);

  match file {
    Ok(_) => Ok(FileLockGuard { path: lock_path }),
    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
      anyhow::bail!(
        "Another gpx apply is running (lock: {})",
        lock_path.display()
      )
    }
    Err(e) => Err(e.into()),
  }
}

fn apply_repo_local_include(
  cwd: &Path,
  profile_gitconfig_path: &Path,
  in_repo: bool,
  allow_shared_fallback: bool,
) -> Result<()> {
  if !in_repo {
    return Err(anyhow::Error::new(GpxError::RepoLocalOutsideRepo));
  }

  let linked = is_linked_worktree(cwd)?;
  let worktree_cfg_enabled = is_worktree_config_enabled(cwd)?;
  let profile_path = profile_gitconfig_path.to_string_lossy().to_string();
  if worktree_cfg_enabled {
    git_config_set(cwd, &["--worktree", "include.path"], &profile_path)?;
    return Ok(());
  }

  if linked {
    if !allow_shared_fallback {
      return Err(anyhow::Error::new(GpxError::WorktreeConfigRequired));
    }
    git_config_set(cwd, &["--local", "include.path"], &profile_path)?;
    return Ok(());
  }

  git_config_set(cwd, &["--local", "include.path"], &profile_path)?;
  Ok(())
}

fn is_linked_worktree(cwd: &Path) -> Result<bool> {
  let git_dir = git_output(cwd, &["rev-parse", "--git-dir"])?;
  let common_dir = git_output(cwd, &["rev-parse", "--git-common-dir"])?;
  Ok(git_dir.trim() != common_dir.trim())
}

fn is_worktree_config_enabled(cwd: &Path) -> Result<bool> {
  let out = Command::new("git")
    .args(["config", "--bool", "extensions.worktreeConfig"])
    .current_dir(cwd)
    .output()?;
  if !out.status.success() {
    return Ok(false);
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim() == "true")
}

fn git_config_set(cwd: &Path, mode_and_key: &[&str], value: &str) -> Result<()> {
  let mut args = vec!["config", "--replace-all"];
  args.extend_from_slice(mode_and_key);
  args.push(value);
  let status = Command::new("git").args(args).current_dir(cwd).status()?;
  if !status.success() {
    anyhow::bail!("Failed to set git config {:?}={}", mode_and_key, value);
  }
  Ok(())
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String> {
  let out = Command::new("git").args(args).current_dir(cwd).output()?;
  if !out.status.success() {
    anyhow::bail!("git {:?} failed", args);
  }
  Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::process::Command;

  #[test]
  fn test_render_ssh_include_empty_for_profile_without_ssh() {
    let config = Config::default();
    assert_eq!(render_ssh_include(&config, None), "");
  }

  #[test]
  fn test_render_ssh_include_with_key_and_identities_only() {
    let ssh = SshConfig {
      key: Some("~/.ssh/id_ed25519_work".into()),
      identities_only: true,
    };

    let config = Config::default();
    let content = render_ssh_include(&config, Some(&ssh));
    assert!(content.contains("Host *"));
    assert!(content.contains("IdentityFile ~/.ssh/id_ed25519_work"));
    assert!(content.contains("IdentitiesOnly yes"));
  }

  #[test]
  fn test_render_dynamic_ssh_include_uses_match_exec_blocks() {
    let mut config = Config::default();
    config.ssh.dynamic_match = true;
    config.profile.insert(
      "work".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: Some(SshConfig {
          key: Some("~/.ssh/id_work".into()),
          identities_only: true,
        }),
      },
    );
    config.profile.insert(
      "personal".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: Some(SshConfig {
          key: Some("~/.ssh/id_personal".into()),
          identities_only: false,
        }),
      },
    );

    let content = render_ssh_include(&config, None);
    assert!(content.contains("Match exec \"gpx ssh-eval --profile 'work'\""));
    assert!(content.contains("IdentityFile ~/.ssh/id_work"));
    assert!(content.contains("IdentitiesOnly yes"));
    assert!(content.contains("Match exec \"gpx ssh-eval --profile 'personal'\""));
    assert!(content.contains("IdentityFile ~/.ssh/id_personal"));
  }

  #[test]
  fn test_build_change_summary() {
    assert_eq!(
      build_change_summary(Some("work"), "work"),
      "Profile unchanged: work"
    );
    assert_eq!(
      build_change_summary(Some("personal"), "work"),
      "Profile switched: personal -> work"
    );
    assert_eq!(
      build_change_summary(None, "work"),
      "Profile initialized: work"
    );
  }

  #[test]
  fn test_repo_local_mode_requires_repo() {
    let temp = tempfile::tempdir().unwrap();
    let res = apply_repo_local_include(
      temp.path(),
      temp.path().join("work.gitconfig").as_path(),
      false,
      false,
    );
    assert!(res.is_err());
  }

  fn git_ok(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
      .args(args)
      .current_dir(cwd)
      .status()
      .unwrap();
    assert!(
      status.success(),
      "git {:?} failed in {}",
      args,
      cwd.display()
    );
  }

  fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
      .args(args)
      .current_dir(cwd)
      .output()
      .unwrap();
    assert!(
      out.status.success(),
      "git {:?} failed in {}",
      args,
      cwd.display()
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
  }

  fn init_repo_with_commit(repo: &Path) {
    std::fs::create_dir_all(repo).unwrap();
    git_ok(repo, &["init"]);
    git_ok(repo, &["config", "user.name", "Tester"]);
    git_ok(repo, &["config", "user.email", "tester@example.com"]);
    std::fs::write(repo.join("README.md"), "hello").unwrap();
    git_ok(repo, &["add", "README.md"]);
    git_ok(repo, &["commit", "-m", "init"]);
  }

  #[test]
  fn test_repo_local_linked_worktree_requires_worktree_config_or_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let wt = temp.path().join("wt");
    init_repo_with_commit(&repo);
    git_ok(&repo, &["worktree", "add", wt.to_str().unwrap()]);

    let profile_path = temp.path().join("work.gitconfig");
    std::fs::write(&profile_path, "[user]\n    email = test@example.com\n").unwrap();

    let err = apply_repo_local_include(&wt, &profile_path, true, false).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("extensions.worktreeConfig is disabled"));
  }

  #[test]
  fn test_repo_local_linked_worktree_fallback_writes_shared_local() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let wt = temp.path().join("wt");
    init_repo_with_commit(&repo);
    git_ok(&repo, &["worktree", "add", wt.to_str().unwrap()]);

    let profile_path = temp.path().join("work.gitconfig");
    std::fs::write(&profile_path, "[user]\n    email = test@example.com\n").unwrap();

    apply_repo_local_include(&wt, &profile_path, true, true).unwrap();
    let got = git_out(&repo, &["config", "--local", "--get", "include.path"]);
    assert_eq!(got, profile_path.to_string_lossy());
  }

  #[test]
  fn test_repo_local_worktree_config_writes_per_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let wt = temp.path().join("wt");
    init_repo_with_commit(&repo);
    git_ok(&repo, &["config", "extensions.worktreeConfig", "true"]);
    git_ok(&repo, &["worktree", "add", wt.to_str().unwrap()]);

    let profile_path = temp.path().join("work.gitconfig");
    std::fs::write(&profile_path, "[user]\n    email = test@example.com\n").unwrap();

    apply_repo_local_include(&wt, &profile_path, true, false).unwrap();
    let got = git_out(&wt, &["config", "--worktree", "--get", "include.path"]);
    assert_eq!(got, profile_path.to_string_lossy());
  }
}
