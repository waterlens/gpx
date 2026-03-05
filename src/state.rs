use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::{AppContext, ConfigSource};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RuntimeState {
  pub last_profile: Option<String>,
  pub last_rule: Option<String>,
  pub last_reason: Option<String>,
  pub last_change_summary: Option<String>,
  pub last_cwd: Option<String>,
  pub last_applied_unix: Option<u64>,
}

pub fn status(ctx: &AppContext, verbose: bool) -> Result<()> {
  let loaded = ctx.load_config_with_info()?;
  let config = loaded.config;
  if let Err(e) = config.validate() {
    println!("Config validation: FAIL ({})", e);
  } else {
    println!("Config validation: OK");
  }
  let state = load_state(ctx).unwrap_or_default();

  println!("Config dir: {}", ctx.config_dir.display());
  println!(
    "Config source: {}",
    match loaded.source {
      ConfigSource::Toml => "config.toml",
      ConfigSource::Ini => "config (INI)",
      ConfigSource::Default => "defaults (no config file)",
    }
  );
  if loaded.both_configs_present {
    println!("Warning: both config.toml and config exist; config.toml takes precedence.");
  }

  println!("Profiles: {}", config.profile.len());
  println!("Rules: {}", config.rule.len());
  println!(
    "Apply mode: {}",
    match config.core.mode {
      crate::config::ApplyMode::GlobalActive => "global-active",
      crate::config::ApplyMode::RepoLocal => "repo-local",
    }
  );
  println!(
    "Worktree shared fallback: {}",
    if config.worktree.allow_shared_fallback {
      "enabled"
    } else {
      "disabled"
    }
  );
  println!(
    "Hook fix policy: {}",
    match config.hook.fix_policy {
      crate::config::HookFixPolicy::Continue => "continue",
      crate::config::HookFixPolicy::AbortOnce => "abort-once",
    }
  );
  println!(
    "SSH dynamicMatch: {}",
    if config.ssh.dynamic_match {
      "enabled (experimental)"
    } else {
      "disabled"
    }
  );
  println!(
    "Last applied profile: {}",
    state.last_profile.unwrap_or_else(|| "<none>".to_string())
  );

  if verbose {
    println!(
      "Last rule: {}",
      state.last_rule.unwrap_or_else(|| "<none>".to_string())
    );
    println!(
      "Last reason: {}",
      state.last_reason.unwrap_or_else(|| "<none>".to_string())
    );
    println!(
      "Last cwd: {}",
      state.last_cwd.unwrap_or_else(|| "<none>".to_string())
    );
    println!(
      "Last change summary: {}",
      state
        .last_change_summary
        .unwrap_or_else(|| "<none>".to_string())
    );
    println!(
      "Last applied unix timestamp: {}",
      state
        .last_applied_unix
        .map(|v| v.to_string())
        .unwrap_or_else(|| "<none>".to_string())
    );
    println!("{:#?}", config);
  }
  Ok(())
}

pub fn record_apply(
  ctx: &AppContext,
  profile: &str,
  matched_rule: Option<&str>,
  reason: &str,
  change_summary: Option<&str>,
  cwd: &Path,
) -> Result<()> {
  let mut state = load_state(ctx).unwrap_or_default();
  state.last_profile = Some(profile.to_string());
  state.last_rule = matched_rule.map(|r| r.to_string());
  state.last_reason = Some(reason.to_string());
  state.last_change_summary = change_summary.map(|s| s.to_string());
  state.last_cwd = Some(cwd.display().to_string());
  state.last_applied_unix = Some(
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)?
      .as_secs(),
  );
  save_state(ctx, &state)
}

pub fn load_state(ctx: &AppContext) -> Result<RuntimeState> {
  let path = ctx.state_file();
  if !path.exists() {
    return Ok(RuntimeState::default());
  }
  let content = std::fs::read_to_string(path)?;
  let state = toml::from_str(&content)?;
  Ok(state)
}

fn save_state(ctx: &AppContext, state: &RuntimeState) -> Result<()> {
  let path = ctx.state_file();
  let parent = path
    .parent()
    .ok_or_else(|| anyhow::anyhow!("Invalid state file path"))?;
  std::fs::create_dir_all(parent)?;
  let content = toml::to_string_pretty(state)?;
  let mut temp = tempfile::NamedTempFile::new_in(parent)?;
  use std::io::Write;
  temp.write_all(content.as_bytes())?;
  temp.persist(path)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::AppContext;

  #[test]
  fn test_record_and_load_state() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = AppContext {
      config_dir: temp.path().join("config"),
      cache_dir: temp.path().join("cache"),
      state_dir: temp.path().join("state"),
    };

    record_apply(
      &ctx,
      "work",
      Some("corp-rule"),
      "Matched rule",
      Some("Profile switched from personal to work"),
      std::path::Path::new("/tmp/repo"),
    )
    .unwrap();

    let state = load_state(&ctx).unwrap();
    assert_eq!(state.last_profile.as_deref(), Some("work"));
    assert_eq!(state.last_rule.as_deref(), Some("corp-rule"));
    assert_eq!(state.last_reason.as_deref(), Some("Matched rule"));
    assert_eq!(
      state.last_change_summary.as_deref(),
      Some("Profile switched from personal to work")
    );
    assert_eq!(state.last_cwd.as_deref(), Some("/tmp/repo"));
    assert!(state.last_applied_unix.is_some());
  }
}
