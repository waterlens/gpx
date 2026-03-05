use crate::error::GpxError;
use anyhow::{Context as _, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

pub struct AppContext {
  pub config_dir: PathBuf,
  pub cache_dir: PathBuf,
  pub state_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
  Toml,
  Ini,
  Default,
}

#[derive(Debug)]
pub struct ConfigLoadResult {
  pub config: Config,
  pub source: ConfigSource,
  pub both_configs_present: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CoreConfig {
  pub default_profile: Option<String>,
  #[serde(default = "default_rule_mode")]
  pub rule_mode: RuleMode,
  #[serde(default)]
  pub mode: ApplyMode,
}

fn default_rule_mode() -> RuleMode {
  RuleMode::FirstMatch
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RuleMode {
  #[default]
  FirstMatch,
  HighestScore,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyMode {
  #[default]
  GlobalActive,
  RepoLocal,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Profile {
  pub user: Option<GitUser>,
  pub gpg: Option<GitGpg>,
  pub ssh: Option<SshConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitUser {
  pub name: Option<String>,
  pub email: Option<String>,
  pub signingkey: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitGpg {
  pub format: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
  pub key: Option<String>,
  #[serde(default)]
  pub identities_only: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Rule {
  pub profile: String,
  #[serde(default)]
  pub priority: i32,
  #[serde(rename = "match.path")]
  pub match_path: Option<String>,
  #[serde(rename = "match.remoteHost")]
  pub match_remote_host: Option<String>,
  #[serde(rename = "match.remoteOrg")]
  pub match_remote_org: Option<String>,
  #[serde(rename = "match.fileExists")]
  pub match_file_exists: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HookConfig {
  #[serde(default)]
  pub shell: bool,
  #[serde(default)]
  pub git: bool,
  #[serde(default = "default_hook_fix_policy")]
  pub fix_policy: HookFixPolicy,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
  #[serde(default)]
  pub allow_profile_override: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HookFixPolicy {
  #[default]
  Continue,
  AbortOnce,
}

fn default_hook_fix_policy() -> HookFixPolicy {
  HookFixPolicy::Continue
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SshRuntimeConfig {
  #[serde(default)]
  pub dynamic_match: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeConfig {
  #[serde(default)]
  pub allow_shared_fallback: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Config {
  pub core: CoreConfig,
  pub profile: IndexMap<String, Profile>,
  pub rule: IndexMap<String, Rule>,
  pub hook: HookConfig,
  pub run: RunConfig,
  pub ssh: SshRuntimeConfig,
  pub worktree: WorktreeConfig,
}

impl Config {
  pub fn validate(&self) -> Result<()> {
    for (name, rule) in &self.rule {
      if rule.match_path.is_none()
        && rule.match_remote_host.is_none()
        && rule.match_remote_org.is_none()
        && rule.match_file_exists.is_none()
      {
        anyhow::bail!("Rule '{}' must have at least one match condition", name);
      }
      if !self.profile.contains_key(&rule.profile) {
        anyhow::bail!(
          "Rule '{}' refers to non-existent profile '{}'",
          name,
          rule.profile
        );
      }
    }
    if let Some(ref default) = self.core.default_profile
      && !self.profile.contains_key(default)
    {
      anyhow::bail!(
        "defaultProfile '{}' refers to non-existent profile",
        default
      );
    }
    Ok(())
  }
}

impl AppContext {
  pub fn new() -> Result<Self> {
    let home = resolve_home_dir().context("Could not determine home directory")?;
    let config_dir = resolve_xdg_dir("XDG_CONFIG_HOME", &home.join(".config")).join("gpx");
    let cache_dir = resolve_xdg_dir("XDG_CACHE_HOME", &home.join(".cache")).join("gpx");
    let state_dir =
      resolve_xdg_dir("XDG_STATE_HOME", &home.join(".local").join("state")).join("gpx");

    Ok(Self {
      config_dir,
      cache_dir,
      state_dir,
    })
  }

  pub fn config_file(&self) -> PathBuf {
    self.config_dir.join("config.toml")
  }

  pub fn config_file_ini(&self) -> PathBuf {
    self.config_dir.join("config")
  }

  pub fn git_profiles_dir(&self) -> PathBuf {
    self.cache_dir.join("git").join("profiles")
  }

  pub fn git_active_include(&self) -> PathBuf {
    self.cache_dir.join("git").join("active.gitconfig")
  }

  pub fn ssh_include_file(&self) -> PathBuf {
    self.cache_dir.join("ssh").join("gpx_ssh.conf")
  }

  pub fn state_file(&self) -> PathBuf {
    self.state_dir.join("state.toml")
  }

  pub fn create_dirs(&self) -> Result<()> {
    std::fs::create_dir_all(&self.config_dir)?;
    std::fs::create_dir_all(self.git_profiles_dir())?;
    let ssh_include_file = self.ssh_include_file();
    let ssh_parent = ssh_include_file
      .parent()
      .ok_or_else(|| GpxError::MissingParent(ssh_include_file.display().to_string()))?;
    std::fs::create_dir_all(ssh_parent)?;
    std::fs::create_dir_all(&self.state_dir)?;
    Ok(())
  }

  pub fn load_config(&self) -> Result<Config> {
    Ok(self.load_config_with_info()?.config)
  }

  pub fn load_config_with_info(&self) -> Result<ConfigLoadResult> {
    let toml_path = self.config_file();
    let ini_path = self.config_file_ini();
    let both_configs_present = toml_path.exists() && ini_path.exists();

    if toml_path.exists() {
      if both_configs_present {
        tracing::warn!("Both config.toml and config exist. Prioritizing config.toml.");
      }
      let content = std::fs::read_to_string(&toml_path)?;
      let config: Config = toml::from_str(&content)?;
      Ok(ConfigLoadResult {
        config,
        source: ConfigSource::Toml,
        both_configs_present,
      })
    } else if ini_path.exists() {
      let config = self.load_ini_config(&ini_path)?;
      Ok(ConfigLoadResult {
        config,
        source: ConfigSource::Ini,
        both_configs_present,
      })
    } else {
      Ok(ConfigLoadResult {
        config: Config::default(),
        source: ConfigSource::Default,
        both_configs_present,
      })
    }
  }

  fn load_ini_config(&self, path: &std::path::Path) -> Result<Config> {
    let buf = std::fs::read(path)?;
    let options = gix_config::file::init::Options::default();
    let git_config = gix_config::File::from_bytes_no_includes(
      &buf,
      gix_config::file::Metadata::default(),
      options,
    )
    .map_err(|e| anyhow::anyhow!("Failed to parse INI config: {}", e))?;

    let mut config = Config::default();

    if let Some(val) = git_config.string("core.defaultProfile") {
      config.core.default_profile = Some(val.to_string());
    }
    if let Some(val) = git_config.string("core.ruleMode") {
      config.core.rule_mode = match val.to_string().as_str() {
        "highest-score" => RuleMode::HighestScore,
        _ => RuleMode::FirstMatch,
      };
    }
    if let Some(val) = git_config.string("core.mode") {
      config.core.mode = match val.to_string().as_str() {
        "repo-local" => ApplyMode::RepoLocal,
        _ => ApplyMode::GlobalActive,
      };
    }

    if let Some(sections) = git_config.sections_by_name("profile") {
      for section in sections {
        if let Some(profile_name) = section.header().subsection_name() {
          let mut profile = Profile {
            user: None,
            gpg: None,
            ssh: None,
          };

          let user = GitUser {
            name: section
              .value("user.name")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
            email: section
              .value("user.email")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
            signingkey: section
              .value("user.signingkey")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
          };
          if user.name.is_some() || user.email.is_some() || user.signingkey.is_some() {
            profile.user = Some(user);
          }

          let gpg = GitGpg {
            format: section
              .value("gpg.format")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
          };
          if gpg.format.is_some() {
            profile.gpg = Some(gpg);
          }

          let ssh = SshConfig {
            key: section
              .value("ssh.key")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
            identities_only: section
              .value("ssh.identitiesOnly")
              .and_then(|v| String::from_utf8_lossy(&v).parse::<bool>().ok())
              .unwrap_or_default(),
          };
          if ssh.key.is_some() || ssh.identities_only {
            profile.ssh = Some(ssh);
          }

          config.profile.insert(profile_name.to_string(), profile);
        }
      }
    }

    if let Some(sections) = git_config.sections_by_name("rule") {
      for section in sections {
        if let Some(rule_name) = section.header().subsection_name()
          && let Some(profile) = section.value("profile")
        {
          let rule = Rule {
            profile: String::from_utf8_lossy(&profile).to_string(),
            priority: section
              .value("priority")
              .and_then(|v| String::from_utf8_lossy(&v).parse::<i32>().ok())
              .unwrap_or_default(),
            match_path: section
              .value("match.path")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
            match_remote_host: section
              .value("match.remoteHost")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
            match_remote_org: section
              .value("match.remoteOrg")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
            match_file_exists: section
              .value("match.fileExists")
              .map(|v| String::from_utf8_lossy(&v).to_string()),
          };
          config.rule.insert(rule_name.to_string(), rule);
        }
      }
    }

    config.hook.shell = git_config
      .boolean("hook.shell")
      .and_then(|r| r.ok())
      .unwrap_or_default();
    config.hook.git = git_config
      .boolean("hook.git")
      .and_then(|r| r.ok())
      .unwrap_or_default();
    if let Some(policy) = git_config.string("hook.fixPolicy") {
      config.hook.fix_policy = match policy.to_string().as_str() {
        "abort-once" => HookFixPolicy::AbortOnce,
        _ => HookFixPolicy::Continue,
      };
    }
    config.run.allow_profile_override = git_config
      .boolean("run.allowProfileOverride")
      .and_then(|r| r.ok())
      .unwrap_or_default();
    config.ssh.dynamic_match = git_config
      .boolean("ssh.dynamicMatch")
      .and_then(|r| r.ok())
      .unwrap_or_default();
    config.worktree.allow_shared_fallback = git_config
      .boolean("worktree.allowSharedFallback")
      .and_then(|r| r.ok())
      .unwrap_or_default();

    Ok(config)
  }
}

fn resolve_home_dir() -> Option<PathBuf> {
  std::env::var_os("HOME")
    .map(PathBuf::from)
    .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()))
}

fn resolve_xdg_dir(var_name: &str, fallback: &Path) -> PathBuf {
  std::env::var_os(var_name)
    .map(PathBuf::from)
    .unwrap_or_else(|| fallback.to_path_buf())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_config_validation_missing_match() {
    let mut config = Config::default();
    config.profile.insert(
      "work".to_string(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.rule.insert(
      "bad-rule".to_string(),
      Rule {
        profile: "work".to_string(),
        priority: 0,
        match_path: None,
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_config_validation_missing_profile() {
    let mut config = Config::default();
    config.rule.insert(
      "good-rule".to_string(),
      Rule {
        profile: "non-existent".to_string(),
        priority: 0,
        match_path: Some("~/code/**".to_string()),
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_config_validation_valid() {
    let mut config = Config::default();
    config.profile.insert(
      "work".to_string(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.rule.insert(
      "good-rule".to_string(),
      Rule {
        profile: "work".to_string(),
        priority: 0,
        match_path: Some("~/code/**".to_string()),
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );
    assert!(config.validate().is_ok());
  }

  #[test]
  fn test_toml_ssh_identities_only_camel_case() {
    let input = r#"
            [core]
            defaultProfile = "work"
            mode = "repo-local"

            [profile.work.ssh]
            key = "~/.ssh/id_ed25519_work"
            identitiesOnly = true

            [ssh]
            dynamicMatch = true

            [hook]
            fixPolicy = "abort-once"

            [worktree]
            allowSharedFallback = true
        "#;

    let config: Config = toml::from_str(input).unwrap();
    let ssh = config
      .profile
      .get("work")
      .and_then(|p| p.ssh.as_ref())
      .unwrap();
    assert!(ssh.identities_only);
    assert!(config.ssh.dynamic_match);
    assert_eq!(config.hook.fix_policy, HookFixPolicy::AbortOnce);
    assert_eq!(config.core.mode, ApplyMode::RepoLocal);
    assert!(config.worktree.allow_shared_fallback);
  }

  #[test]
  fn test_app_context_uses_xdg_env() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let old_cfg = std::env::var_os("XDG_CONFIG_HOME");
    let old_cache = std::env::var_os("XDG_CACHE_HOME");
    let old_state = std::env::var_os("XDG_STATE_HOME");

    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", base.join("cfg"));
      std::env::set_var("XDG_CACHE_HOME", base.join("cache"));
      std::env::set_var("XDG_STATE_HOME", base.join("state"));
    }

    let ctx = AppContext::new().unwrap();
    assert_eq!(ctx.config_dir, base.join("cfg").join("gpx"));
    assert_eq!(ctx.cache_dir, base.join("cache").join("gpx"));
    assert_eq!(ctx.state_dir, base.join("state").join("gpx"));

    unsafe {
      match old_cfg {
        Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
      }
      match old_cache {
        Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
        None => std::env::remove_var("XDG_CACHE_HOME"),
      }
      match old_state {
        Some(v) => std::env::set_var("XDG_STATE_HOME", v),
        None => std::env::remove_var("XDG_STATE_HOME"),
      }
    }
  }
}
