use crate::config::{AppContext, Config};
use crate::error::GpxError;
use crate::rules::{gather_context, resolve_profile};
use anyhow::{Context, Result, anyhow, bail};
use std::process::Command;

pub fn execute_git(
  ctx: &AppContext,
  profile_name: Option<String>,
  git_args: Vec<String>,
) -> Result<()> {
  execute_command(ctx, profile_name, "git".to_string(), git_args)
}

pub fn execute_passthrough(
  ctx: &AppContext,
  profile_name: Option<String>,
  command_and_args: Vec<String>,
) -> Result<()> {
  let (program, args) = command_and_args
    .split_first()
    .context("No command provided for passthrough mode")?;
  execute_command(ctx, profile_name, program.clone(), args.to_vec())
}

fn execute_command(
  ctx: &AppContext,
  profile_name: Option<String>,
  program: String,
  args: Vec<String>,
) -> Result<()> {
  let config = ctx.load_config()?;
  config.validate()?;

  let profile_name = if profile_name.is_some() {
    select_requested_profile(&config, profile_name, None)?
  } else {
    let cwd = std::env::current_dir().map_err(|_| GpxError::ResolveCurrentDir)?;
    let rule_ctx = gather_context(&cwd)?;
    let resolved = resolve_profile(&rule_ctx, &config)?.cloned();
    select_requested_profile(&config, None, resolved)?
  };

  let mut cmd = Command::new(&program);
  cmd.args(args);

  if let Some(profile_name) = profile_name {
    let profile = config
      .profile
      .get(&profile_name)
      .context(format!("Profile '{}' not found", profile_name))?;
    apply_profile_env(&mut cmd, profile);
  }

  exec_command(cmd, &program)
}

fn apply_profile_env(cmd: &mut Command, profile: &crate::config::Profile) {
  let mut config_idx = 0;
  if let Some(ref user) = profile.user {
    if let Some(ref name) = user.name {
      cmd.env(format!("GIT_CONFIG_KEY_{}", config_idx), "user.name");
      cmd.env(format!("GIT_CONFIG_VALUE_{}", config_idx), name);
      config_idx += 1;
    }
    if let Some(ref email) = user.email {
      cmd.env(format!("GIT_CONFIG_KEY_{}", config_idx), "user.email");
      cmd.env(format!("GIT_CONFIG_VALUE_{}", config_idx), email);
      config_idx += 1;
    }
    if let Some(ref signingkey) = user.signingkey {
      cmd.env(format!("GIT_CONFIG_KEY_{}", config_idx), "user.signingkey");
      cmd.env(format!("GIT_CONFIG_VALUE_{}", config_idx), signingkey);
      config_idx += 1;
    }
  }
  if let Some(ref gpg) = profile.gpg
    && let Some(ref format) = gpg.format
  {
    cmd.env(format!("GIT_CONFIG_KEY_{}", config_idx), "gpg.format");
    cmd.env(format!("GIT_CONFIG_VALUE_{}", config_idx), format);
    config_idx += 1;
  }
  cmd.env("GIT_CONFIG_COUNT", config_idx.to_string());

  if let Some(ref ssh) = profile.ssh {
    let mut ssh_args = Vec::new();
    if let Some(ref key) = ssh.key {
      ssh_args.push(format!("-i {}", key));
    }
    if ssh.identities_only {
      ssh_args.push("-o IdentitiesOnly=yes".to_string());
    }
    if !ssh_args.is_empty() {
      cmd.env("GIT_SSH_COMMAND", format!("ssh {}", ssh_args.join(" ")));
    }
  }
}

fn exec_command(mut cmd: Command, program: &str) -> Result<()> {
  #[cfg(unix)]
  {
    use std::os::unix::process::CommandExt;
    let err = cmd.exec();
    Err(anyhow!("Failed to exec {}: {}", program, err))
  }

  #[cfg(not(unix))]
  {
    let status = cmd.status()?;
    std::process::exit(status.code().unwrap_or(1));
  }
}

fn select_requested_profile(
  config: &Config,
  requested_profile: Option<String>,
  resolved_profile: Option<String>,
) -> Result<Option<String>> {
  if let Some(name) = requested_profile {
    if !config.run.allow_profile_override {
      bail!("--profile override is disabled by config: run.allowProfileOverride=false");
    }
    Ok(Some(name))
  } else {
    Ok(resolved_profile)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::RunConfig;

  #[test]
  fn test_select_requested_profile_rejects_override_when_disabled() {
    let config = Config {
      run: RunConfig {
        allow_profile_override: false,
      },
      ..Config::default()
    };

    let res = select_requested_profile(&config, Some("work".into()), None);
    assert!(res.is_err());
  }

  #[test]
  fn test_select_requested_profile_allows_override_when_enabled() {
    let config = Config {
      run: RunConfig {
        allow_profile_override: true,
      },
      ..Config::default()
    };

    let res = select_requested_profile(&config, Some("work".into()), None).unwrap();
    assert_eq!(res, Some("work".to_string()));
  }
}
