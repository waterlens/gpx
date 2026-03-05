use crate::cli::ListKind;
use crate::config::{AppContext, Config, ConfigSource, Profile, Rule};
use crate::output::{info, item, note, ok, section, warn};
use anyhow::Result;
use serde_json::json;

pub fn run(ctx: &AppContext, kind: Option<ListKind>, json_output: bool) -> Result<()> {
  let loaded = ctx.load_config_with_info()?;
  let validation_error = loaded.config.validate().err().map(|e| e.to_string());

  if json_output {
    print_json(loaded.config, kind)?;
    return Ok(());
  }

  section("List report");
  item(
    "Config source",
    match loaded.source {
      ConfigSource::Toml => info("config.toml"),
      ConfigSource::Ini => info("config (INI)"),
      ConfigSource::Default => warn("defaults (no config file)"),
    },
  );
  if loaded.both_configs_present {
    note(format!(
      "Warning: {} both config.toml and config exist; config.toml takes precedence.",
      warn("WARN")
    ));
  }
  if let Some(err) = validation_error {
    item("Config validation", format!("{} ({})", warn("WARN"), err));
  } else {
    item("Config validation", ok("OK"));
  }

  match kind {
    Some(ListKind::Profiles) => print_profiles(&loaded.config),
    Some(ListKind::Rules) => print_rules(&loaded.config),
    None => {
      print_profiles(&loaded.config);
      print_rules(&loaded.config);
    }
  }

  Ok(())
}

fn print_json(config: Config, kind: Option<ListKind>) -> Result<()> {
  let profiles = config
    .profile
    .into_iter()
    .map(|(name, profile)| json!({ "name": name, "profile": profile }))
    .collect::<Vec<_>>();
  let rules = config
    .rule
    .into_iter()
    .map(|(name, rule)| json!({ "name": name, "rule": rule }))
    .collect::<Vec<_>>();

  let out = match kind {
    Some(ListKind::Profiles) => json!({ "profiles": profiles }),
    Some(ListKind::Rules) => json!({ "rules": rules }),
    None => json!({
      "profiles": profiles,
      "rules": rules
    }),
  };

  println!("{}", serde_json::to_string_pretty(&out)?);
  Ok(())
}

fn print_profiles(config: &Config) {
  item("Profiles", info(&config.profile.len().to_string()));
  if config.profile.is_empty() {
    note(format!("{} no profiles configured.", warn("WARN")));
    return;
  }

  for (name, profile) in &config.profile {
    let fields = profile_field_names(profile);
    let summary = if fields.is_empty() {
      warn("<empty>")
    } else {
      info(&fields.join(", "))
    };
    note(format!("profile.{} -> {}", name, summary));
  }
}

fn print_rules(config: &Config) {
  item("Rules", info(&config.rule.len().to_string()));
  if config.rule.is_empty() {
    note(format!("{} no rules configured.", warn("WARN")));
    return;
  }

  for (name, rule) in &config.rule {
    let matcher_text = rule_matchers(rule);
    let matcher_summary = if matcher_text.is_empty() {
      warn("<none>")
    } else {
      info(&matcher_text.join(" && "))
    };
    note(format!(
      "rule.{} -> profile={} priority={} match={}",
      name, rule.profile, rule.priority, matcher_summary
    ));
  }
}

fn profile_field_names(profile: &Profile) -> Vec<&'static str> {
  let mut fields = Vec::new();

  if let Some(user) = profile.user.as_ref() {
    if user.name.is_some() {
      fields.push("user.name");
    }
    if user.email.is_some() {
      fields.push("user.email");
    }
    if user.signingkey.is_some() {
      fields.push("user.signingkey");
    }
  }
  if let Some(gpg) = profile.gpg.as_ref()
    && gpg.format.is_some()
  {
    fields.push("gpg.format");
  }
  if let Some(ssh) = profile.ssh.as_ref() {
    if ssh.key.is_some() {
      fields.push("ssh.key");
    }
    if ssh.identities_only {
      fields.push("ssh.identitiesOnly");
    }
  }

  fields
}

fn rule_matchers(rule: &Rule) -> Vec<String> {
  let mut matchers = Vec::new();

  if let Some(path) = rule.match_path.as_ref() {
    matchers.push(format!("path={}", path));
  }
  if let Some(host) = rule.match_remote_host.as_ref() {
    matchers.push(format!("remoteHost={}", host));
  }
  if let Some(org) = rule.match_remote_org.as_ref() {
    matchers.push(format!("remoteOrg={}", org));
  }
  if let Some(file) = rule.match_file_exists.as_ref() {
    matchers.push(format!("fileExists={}", file));
  }

  matchers
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::{GitGpg, GitUser, SshConfig};

  #[test]
  fn profile_field_names_collects_configured_fields() {
    let profile = Profile {
      user: Some(GitUser {
        name: Some("Alice".to_string()),
        email: Some("alice@example.com".to_string()),
        signingkey: None,
      }),
      gpg: Some(GitGpg {
        format: Some("openpgp".to_string()),
      }),
      ssh: Some(SshConfig {
        key: Some("~/.ssh/id_ed25519".to_string()),
        identities_only: true,
      }),
    };

    let fields = profile_field_names(&profile);
    assert_eq!(
      fields,
      vec![
        "user.name",
        "user.email",
        "gpg.format",
        "ssh.key",
        "ssh.identitiesOnly"
      ]
    );
  }

  #[test]
  fn rule_matchers_collects_all_match_conditions() {
    let rule = Rule {
      profile: "work".to_string(),
      priority: 100,
      match_path: Some("~/code/company/**".to_string()),
      match_remote_host: Some("github.com".to_string()),
      match_remote_org: Some("corp-org".to_string()),
      match_file_exists: Some(".gpx-work".to_string()),
    };

    let matchers = rule_matchers(&rule);
    assert_eq!(
      matchers,
      vec![
        "path=~/code/company/**".to_string(),
        "remoteHost=github.com".to_string(),
        "remoteOrg=corp-org".to_string(),
        "fileExists=.gpx-work".to_string()
      ]
    );
  }
}
