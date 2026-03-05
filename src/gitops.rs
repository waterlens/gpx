use crate::config::{AppContext, ApplyMode, Config, HookFixPolicy, Profile, SshConfig};
use crate::error::GpxError;
use crate::output::{info, item, note, section, strong, warn};
use crate::rules::{gather_context, resolve_profile_detailed};
use crate::state;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const MANAGED_INCLUDE_BEGIN: &str = "# >>> gpx managed include >>>";
const MANAGED_INCLUDE_END: &str = "# <<< gpx managed include <<<";
const MANAGED_SSH_INCLUDE_BEGIN: &str = "# >>> gpx managed ssh include >>>";
const MANAGED_SSH_INCLUDE_END: &str = "# <<< gpx managed ssh include <<<";
const CONFIG_TEMPLATE: &str = include_str!("../templates/config.template.toml");

pub fn init(ctx: &AppContext) -> Result<()> {
  section("Init report");
  ctx.create_dirs()?;
  ensure_example_config(ctx)?;

  let home = resolve_home_dir()?;
  let gitconfig_path = home.join(".gitconfig");

  let include_path = ctx.git_active_include();
  let include_status =
    match ensure_managed_gitconfig_include(&gitconfig_path, &include_path, &home)? {
      ManagedIncludeStatus::Created => {
        tracing::info!("Created ~/.gitconfig with managed GPX include");
        format!("{} (~/.gitconfig created)", info("CREATED"))
      }
      ManagedIncludeStatus::Updated => {
        tracing::info!("Updated managed GPX include in ~/.gitconfig");
        format!("{} (managed block refreshed)", info("UPDATED"))
      }
      ManagedIncludeStatus::Exists => {
        tracing::info!("Managed GPX include already exists in ~/.gitconfig");
        format!("{} (already present)", info("OK"))
      }
    };

  item("~/.gitconfig include", include_status);
  let active_file_status = ensure_bootstrap_file(
    &include_path,
    "# gpx managed file\n# run `gpx apply` to set active profile\n",
  )?;
  item(
    "Active include file",
    format!(
      "{} ({})",
      info(active_file_status.label()),
      include_path.display()
    ),
  );

  let ssh_include_path = ctx.ssh_include_file();
  let ssh_file_status = ensure_bootstrap_file(
    &ssh_include_path,
    "# gpx managed file\n# run `gpx apply` to refresh ssh identity settings\n",
  )?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&ssh_include_path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&ssh_include_path, perms)?;
  }
  item(
    "SSH include file",
    format!(
      "{} ({})",
      info(ssh_file_status.label()),
      ssh_include_path.display()
    ),
  );
  let ssh_config_path = home.join(".ssh").join("config");
  let ssh_config_status =
    match ensure_managed_ssh_config_include(&ssh_config_path, &ssh_include_path, &home)? {
      ManagedIncludeStatus::Created => format!("{} (~/.ssh/config created)", info("CREATED")),
      ManagedIncludeStatus::Updated => format!("{} (managed block refreshed)", info("UPDATED")),
      ManagedIncludeStatus::Exists => format!("{} (already present)", info("OK")),
    };
  item("~/.ssh/config include", ssh_config_status);

  let preferred_config = preferred_config_path(ctx);
  note(format!(
    "{} edit {} and add at least one [profile.<name>.user] plus [core] defaultProfile.",
    strong("Next step:"),
    preferred_config.display()
  ));
  note(format!(
    "{} run `gpx apply` to activate a profile.",
    strong("Next step:")
  ));

  Ok(())
}

pub fn deinit(ctx: &AppContext) -> Result<()> {
  section("Deinit report");
  let home = resolve_home_dir()?;
  let gitconfig_path = home.join(".gitconfig");

  let include_status = remove_managed_gitconfig_include(&gitconfig_path)?;
  item(
    "~/.gitconfig include",
    match include_status {
      ManagedRemoveStatus::Updated => {
        format!("{} (managed include removed)", info("UPDATED"))
      }
      ManagedRemoveStatus::Missing => format!("{} (already absent)", warn("MISSING")),
    },
  );
  let ssh_config_path = home.join(".ssh").join("config");
  let ssh_include_status = remove_managed_ssh_config_include(&ssh_config_path)?;
  item(
    "~/.ssh/config include",
    match ssh_include_status {
      ManagedRemoveStatus::Updated => {
        format!("{} (managed include removed)", info("UPDATED"))
      }
      ManagedRemoveStatus::Missing => format!("{} (already absent)", warn("MISSING")),
    },
  );

  item(
    "Git cache dir",
    remove_path(&ctx.cache_dir.join("git"), true)?,
  );
  item(
    "SSH cache dir",
    remove_path(&ctx.cache_dir.join("ssh"), true)?,
  );
  item("Cache root dir", remove_empty_dir(&ctx.cache_dir)?);
  item("State file", remove_path(&ctx.state_file(), false)?);
  item(
    "Apply lock",
    remove_path(&ctx.state_dir.join("apply.lock"), false)?,
  );
  item("State root dir", remove_empty_dir(&ctx.state_dir)?);
  item("Config file", remove_bootstrap_config(ctx)?);
  item("Config root dir", remove_empty_dir(&ctx.config_dir)?);

  note(
    "If hooks were installed, run `gpx hook uninstall --shell ...` and/or `gpx hook uninstall --git`.",
  );

  Ok(())
}

enum BootstrapStatus {
  Created,
  Exists,
}

impl BootstrapStatus {
  fn label(&self) -> &'static str {
    match self {
      Self::Created => "CREATED",
      Self::Exists => "OK",
    }
  }
}

fn ensure_bootstrap_file(path: &Path, default_content: &str) -> Result<BootstrapStatus> {
  if path.exists() {
    return Ok(BootstrapStatus::Exists);
  }
  atomic_write(path, default_content)?;
  Ok(BootstrapStatus::Created)
}

fn preferred_config_path(ctx: &AppContext) -> PathBuf {
  let toml = ctx.config_file();
  let ini = ctx.config_file_ini();
  if toml.exists() || !ini.exists() {
    toml
  } else {
    ini
  }
}

fn ensure_example_config(ctx: &AppContext) -> Result<()> {
  let toml = ctx.config_file();
  let ini = ctx.config_file_ini();
  if toml.exists() || ini.exists() {
    return Ok(());
  }
  atomic_write(&toml, CONFIG_TEMPLATE)?;
  Ok(())
}

enum ManagedIncludeStatus {
  Created,
  Updated,
  Exists,
}

enum ManagedRemoveStatus {
  Updated,
  Missing,
}

fn ensure_managed_gitconfig_include(
  gitconfig_path: &Path,
  include_path: &Path,
  home: &Path,
) -> Result<ManagedIncludeStatus> {
  let include_path_value = display_path_for_config(include_path, home);
  let managed_block = render_managed_include_block(&include_path_value);
  if !gitconfig_path.exists() {
    atomic_write(gitconfig_path, &managed_block)?;
    return Ok(ManagedIncludeStatus::Created);
  }

  let content = std::fs::read_to_string(gitconfig_path)?;
  if let Some((start, end)) = managed_block_range(&content) {
    if &content[start..end] == managed_block.as_str() {
      return Ok(ManagedIncludeStatus::Exists);
    }
    let mut updated = content;
    updated.replace_range(start..end, &managed_block);
    atomic_write(gitconfig_path, &updated)?;
    return Ok(ManagedIncludeStatus::Updated);
  }

  let mut updated = content;
  if !updated.is_empty() && !updated.ends_with('\n') {
    updated.push('\n');
  }
  updated.push_str(&managed_block);
  atomic_write(gitconfig_path, &updated)?;
  Ok(ManagedIncludeStatus::Updated)
}

fn remove_managed_gitconfig_include(gitconfig_path: &Path) -> Result<ManagedRemoveStatus> {
  if !gitconfig_path.exists() {
    return Ok(ManagedRemoveStatus::Missing);
  }

  let content = std::fs::read_to_string(gitconfig_path)?;
  let Some((start, end)) = managed_block_range(&content) else {
    return Ok(ManagedRemoveStatus::Missing);
  };
  let mut updated = content;
  updated.replace_range(start..end, "");

  let normalized = normalize_after_block_removal(updated);
  if normalized.trim().is_empty() {
    std::fs::remove_file(gitconfig_path)?;
  } else {
    atomic_write(gitconfig_path, &normalized)?;
  }
  Ok(ManagedRemoveStatus::Updated)
}

fn ensure_managed_ssh_config_include(
  ssh_config_path: &Path,
  include_path: &Path,
  home: &Path,
) -> Result<ManagedIncludeStatus> {
  let include_value = display_path_for_config(include_path, home);
  let managed_block = render_managed_ssh_include_block(&include_value);
  if !ssh_config_path.exists() {
    if let Some(parent) = ssh_config_path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    atomic_write(ssh_config_path, &managed_block)?;
    return Ok(ManagedIncludeStatus::Created);
  }

  let content = std::fs::read_to_string(ssh_config_path)?;
  let mut body = content.clone();
  if let Some((start, end)) = managed_ssh_block_range(&body) {
    body.replace_range(start..end, "");
  }

  let had_trailing_newline = body.ends_with('\n');
  let mut updated = body;
  if had_trailing_newline && !updated.ends_with('\n') && !updated.is_empty() {
    updated.push('\n');
  }
  updated = updated.trim_start_matches('\n').to_string();
  let desired = if updated.is_empty() {
    managed_block.clone()
  } else {
    format!("{managed_block}{updated}")
  };
  if content == desired {
    return Ok(ManagedIncludeStatus::Exists);
  }

  atomic_write(ssh_config_path, &desired)?;
  Ok(ManagedIncludeStatus::Updated)
}

fn remove_managed_ssh_config_include(ssh_config_path: &Path) -> Result<ManagedRemoveStatus> {
  if !ssh_config_path.exists() {
    return Ok(ManagedRemoveStatus::Missing);
  }

  let content = std::fs::read_to_string(ssh_config_path)?;
  let Some((start, end)) = managed_ssh_block_range(&content) else {
    return Ok(ManagedRemoveStatus::Missing);
  };
  let mut updated = content;
  updated.replace_range(start..end, "");

  let normalized = normalize_after_block_removal(updated);
  if normalized.trim().is_empty() {
    std::fs::remove_file(ssh_config_path)?;
  } else {
    atomic_write(ssh_config_path, &normalized)?;
  }
  Ok(ManagedRemoveStatus::Updated)
}

fn render_managed_include_block(path: &str) -> String {
  format!("{MANAGED_INCLUDE_BEGIN}\n[include]\n\tpath = {path}\n{MANAGED_INCLUDE_END}\n")
}

fn managed_block_range(content: &str) -> Option<(usize, usize)> {
  let start = content.find(MANAGED_INCLUDE_BEGIN)?;
  let end_marker_start = content[start..].find(MANAGED_INCLUDE_END)? + start;
  let end = content[end_marker_start..]
    .find('\n')
    .map(|idx| end_marker_start + idx + 1)
    .unwrap_or(content.len());
  Some((start, end))
}

fn render_managed_ssh_include_block(path: &str) -> String {
  format!("{MANAGED_SSH_INCLUDE_BEGIN}\nInclude {path}\n{MANAGED_SSH_INCLUDE_END}\n")
}

fn managed_ssh_block_range(content: &str) -> Option<(usize, usize)> {
  let start = content.find(MANAGED_SSH_INCLUDE_BEGIN)?;
  let end_marker_start = content[start..].find(MANAGED_SSH_INCLUDE_END)? + start;
  let end = content[end_marker_start..]
    .find('\n')
    .map(|idx| end_marker_start + idx + 1)
    .unwrap_or(content.len());
  Some((start, end))
}

fn normalize_after_block_removal(mut content: String) -> String {
  while content.contains("\n\n\n") {
    content = content.replace("\n\n\n", "\n\n");
  }
  while content.starts_with('\n') {
    content.remove(0);
  }
  while content.ends_with("\n\n") {
    content.pop();
  }
  content
}

fn resolve_home_dir() -> Result<PathBuf> {
  let user_dirs = directories::UserDirs::new().context("Could not find user directories")?;
  Ok(user_dirs.home_dir().to_path_buf())
}

fn display_path_for_config(path: &Path, home: &Path) -> String {
  if let Ok(rel) = path.strip_prefix(home) {
    if rel.as_os_str().is_empty() {
      "~".to_string()
    } else {
      format!("~/{}", rel.to_string_lossy())
    }
  } else {
    path.to_string_lossy().to_string()
  }
}

fn remove_path(path: &Path, is_dir: bool) -> Result<String> {
  if !path.exists() {
    return Ok(format!("{} ({})", warn("MISSING"), path.display()));
  }
  if is_dir {
    std::fs::remove_dir_all(path)?;
  } else {
    std::fs::remove_file(path)?;
  }
  Ok(format!("{} ({})", info("UPDATED"), path.display()))
}

fn remove_empty_dir(path: &Path) -> Result<String> {
  if !path.exists() {
    return Ok(format!("{} ({})", warn("MISSING"), path.display()));
  }
  if !path.is_dir() {
    return Ok(format!(
      "{} ({}, not a directory)",
      warn("WARN"),
      path.display()
    ));
  }
  if (std::fs::read_dir(path)?).next().is_some() {
    return Ok(format!("{} ({}, not empty)", warn("WARN"), path.display()));
  }
  std::fs::remove_dir(path)?;
  Ok(format!("{} ({})", info("UPDATED"), path.display()))
}

fn remove_bootstrap_config(ctx: &AppContext) -> Result<String> {
  let config_path = ctx.config_file();
  if !config_path.exists() {
    return Ok(format!("{} ({})", warn("MISSING"), config_path.display()));
  }

  let content = std::fs::read_to_string(&config_path)?;
  if normalize_config_text(&content) == normalize_config_text(CONFIG_TEMPLATE) {
    std::fs::remove_file(&config_path)?;
    return Ok(format!("{} ({})", info("UPDATED"), config_path.display()));
  }

  Ok(format!(
    "{} ({}, modified by user)",
    warn("WARN"),
    config_path.display()
  ))
}

fn normalize_config_text(text: &str) -> String {
  text.trim_end_matches(['\n', '\r']).to_string()
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
      let home = resolve_home_dir()?;
      let include_target = display_path_for_config(&profile_gitconfig_path, &home);
      let active_gitconfig_path = ctx.git_active_include();
      let active_content = format!(
        "# gpx managed file\n[include]\n\tpath = {}\n",
        include_target
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
      content.push_str(&format!("\tname = {}\n", name));
    }
    if let Some(ref email) = user.email {
      content.push_str(&format!("\temail = {}\n", email));
    }
    if let Some(ref signingkey) = user.signingkey {
      content.push_str(&format!("\tsigningkey = {}\n", signingkey));
    }
  }

  if let Some(ref gpg) = profile.gpg {
    content.push_str("[gpg]\n");
    if let Some(ref format) = gpg.format {
      content.push_str(&format!("\tformat = {}\n", format));
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

  let mut ssh_content = String::from("# gpx managed file\nHost *\n");
  if let Some(ref key) = ssh.key {
    ssh_content.push_str(&format!("\tIdentityFile {}\n", key));
  }
  if ssh.identities_only {
    ssh_content.push_str("\tIdentitiesOnly yes\n");
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
      out.push_str(&format!("\tIdentityFile {}\n", key));
    }
    if ssh.identities_only {
      out.push_str("\tIdentitiesOnly yes\n");
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
  let home = resolve_home_dir()?;
  let profile_path = display_path_for_config(profile_gitconfig_path, &home);
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
    assert!(content.contains("# gpx managed file"));
    assert!(content.contains("Host *"));
    assert!(content.contains("\tIdentityFile ~/.ssh/id_ed25519_work"));
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
    assert!(content.contains("\tIdentityFile ~/.ssh/id_work"));
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
  fn test_ensure_example_config_creates_commented_toml_when_missing() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = AppContext {
      config_dir: temp.path().join("config"),
      cache_dir: temp.path().join("cache"),
      state_dir: temp.path().join("state"),
    };
    ctx.create_dirs().unwrap();

    ensure_example_config(&ctx).unwrap();
    let generated = std::fs::read_to_string(ctx.config_file()).unwrap();
    assert!(generated.contains("# gpx config template"));
    assert!(
      generated
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
    );
  }

  #[test]
  fn test_ensure_example_config_skips_when_ini_exists() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = AppContext {
      config_dir: temp.path().join("config"),
      cache_dir: temp.path().join("cache"),
      state_dir: temp.path().join("state"),
    };
    ctx.create_dirs().unwrap();
    std::fs::write(ctx.config_file_ini(), "[core]\ndefaultProfile = work\n").unwrap();

    ensure_example_config(&ctx).unwrap();
    assert!(!ctx.config_file().exists());
  }

  #[test]
  fn test_display_path_for_config_prefers_tilde_under_home() {
    let home = PathBuf::from("/home/alice");
    let path = PathBuf::from("/home/alice/.cache/gpx/git/active.gitconfig");
    assert_eq!(
      display_path_for_config(&path, &home),
      "~/.cache/gpx/git/active.gitconfig"
    );
  }

  #[test]
  fn test_render_managed_include_block_uses_tab_and_markers() {
    let block = render_managed_include_block("~/.cache/gpx/git/active.gitconfig");
    assert!(block.contains(MANAGED_INCLUDE_BEGIN));
    assert!(block.contains("[include]\n\tpath = ~/.cache/gpx/git/active.gitconfig"));
    assert!(block.contains(MANAGED_INCLUDE_END));
  }

  #[test]
  fn test_render_managed_ssh_include_block_with_markers() {
    let block = render_managed_ssh_include_block("~/.cache/gpx/ssh/gpx_ssh.conf");
    assert!(block.contains(MANAGED_SSH_INCLUDE_BEGIN));
    assert!(block.contains("Include ~/.cache/gpx/ssh/gpx_ssh.conf"));
    assert!(block.contains(MANAGED_SSH_INCLUDE_END));
  }

  #[test]
  fn test_ensure_managed_ssh_config_include_prepends_managed_block() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let ssh_dir = home.join(".ssh");
    std::fs::create_dir_all(&ssh_dir).unwrap();
    let ssh_config = ssh_dir.join("config");
    std::fs::write(&ssh_config, "Host github.com\n  User git\n").unwrap();
    let include = home.join(".cache/gpx/ssh/gpx_ssh.conf");

    let status = ensure_managed_ssh_config_include(&ssh_config, &include, &home).unwrap();
    assert!(matches!(status, ManagedIncludeStatus::Updated));
    let content = std::fs::read_to_string(&ssh_config).unwrap();
    assert!(content.starts_with(MANAGED_SSH_INCLUDE_BEGIN));
    assert_eq!(
      content
        .matches("Include ~/.cache/gpx/ssh/gpx_ssh.conf")
        .count(),
      1
    );
    assert!(content.contains("Host github.com"));
    assert!(content.contains(MANAGED_SSH_INCLUDE_BEGIN));
  }

  #[test]
  fn test_remove_empty_dir_behaviors() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let empty = temp.path().join("empty");
    let non_empty = temp.path().join("non-empty");

    std::fs::create_dir_all(&empty).unwrap();
    std::fs::create_dir_all(&non_empty).unwrap();
    std::fs::write(non_empty.join("keep.txt"), "1").unwrap();

    let missing_status = remove_empty_dir(&missing).unwrap();
    assert!(missing_status.contains("MISSING"));

    let removed_status = remove_empty_dir(&empty).unwrap();
    assert!(removed_status.contains("UPDATED"));
    assert!(!empty.exists());

    let kept_status = remove_empty_dir(&non_empty).unwrap();
    assert!(kept_status.contains("WARN"));
    assert!(non_empty.exists());
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
      .args(["-c", "core.hooksPath=/dev/null"])
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
      .args(["-c", "core.hooksPath=/dev/null"])
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
