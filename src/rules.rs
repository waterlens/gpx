use crate::config::{AppContext, Config, Rule, RuleMode};
use crate::error::GpxError;
use crate::output::{info, item, section, warn};
use anyhow::Result;
use globset::Glob;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub struct RuleContext {
  pub cwd: PathBuf,
  pub repo_root: Option<PathBuf>,
  pub is_submodule: bool,
  pub remotes: Vec<RemoteInfo>,
}

#[derive(Debug)]
pub struct RemoteInfo {
  pub name: String,
  pub host: Option<String>,
  pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextSummary {
  pub cwd: PathBuf,
  pub repo_root: Option<PathBuf>,
  pub is_submodule: bool,
  pub remotes: Vec<RemoteInfoSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteInfoSummary {
  pub name: String,
  pub host: Option<String>,
  pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
  pub resolved_profile: Option<String>,
  pub matched_rule: Option<String>,
  pub reason: String,
  pub context_summary: ContextSummary,
}

pub fn gather_context(cwd: &Path) -> Result<RuleContext> {
  let repo_root = discover_repo_root(cwd).or_else(|| match gix_discover::upwards(cwd) {
    Ok(discovery) => Some(discovery.0.as_ref().to_path_buf()),
    Err(_) => None,
  });

  let mut is_submodule = false;
  let mut remotes = Vec::new();
  if let Some(ref root) = repo_root {
    let (git_dir, submodule) = resolve_git_dir(root);
    is_submodule = submodule;

    if let Ok(git_config) = gix_config::File::from_git_dir(git_dir)
      && let Some(sections) = git_config.sections_by_name("remote")
    {
      for section in sections {
        if let Some(name) = section.header().subsection_name()
          && let Some(url_bytes) = section.value("url")
        {
          let url_str = String::from_utf8_lossy(&url_bytes);
          let info = parse_remote_url(name.to_string(), &url_str);
          remotes.push(info);
        }
      }
    }
  }

  Ok(RuleContext {
    cwd: cwd.to_path_buf(),
    repo_root,
    is_submodule,
    remotes,
  })
}

fn resolve_git_dir(repo_root: &Path) -> (PathBuf, bool) {
  let dot_git = repo_root.join(".git");
  if dot_git.is_dir() {
    return (dot_git, false);
  }

  if dot_git.is_file()
    && let Ok(content) = std::fs::read_to_string(&dot_git)
  {
    for line in content.lines() {
      if let Some(raw) = line.trim().strip_prefix("gitdir:") {
        let path = PathBuf::from(raw.trim());
        let resolved = if path.is_absolute() {
          path
        } else {
          repo_root.join(path)
        };
        let normalized = std::fs::canonicalize(&resolved).unwrap_or(resolved);
        return (normalized, true);
      }
    }
  }

  (repo_root.to_path_buf(), false)
}

fn discover_repo_root(cwd: &Path) -> Option<PathBuf> {
  for ancestor in cwd.ancestors() {
    if ancestor.join(".git").exists() {
      return Some(ancestor.to_path_buf());
    }
  }
  None
}

fn parse_remote_url(name: String, url: &str) -> RemoteInfo {
  let mut host = None;
  let mut org = None;

  if url.starts_with("git@") {
    if let Some(host_part) = url.strip_prefix("git@").and_then(|s| s.split(':').next()) {
      host = Some(host_part.to_string());
      if let Some(path_part) = url.split(':').nth(1) {
        org = path_part.split('/').next().map(|s| s.to_string());
      }
    }
  } else if url.starts_with("https://")
    && let Some(rem) = url.strip_prefix("https://")
  {
    host = rem.split('/').next().map(|s| s.to_string());
    org = rem.split('/').nth(1).map(|s| s.to_string());
  }

  RemoteInfo { name, host, org }
}

pub fn resolve_profile<'a>(ctx: &RuleContext, config: &'a Config) -> Result<Option<&'a String>> {
  let mut matches = Vec::new();

  for (name, rule) in &config.rule {
    if is_match(ctx, rule) {
      matches.push((name, rule));
    }
  }

  if matches.is_empty() {
    return Ok(config.core.default_profile.as_ref());
  }

  match config.core.rule_mode {
    RuleMode::FirstMatch => Ok(Some(&matches[0].1.profile)),
    RuleMode::HighestScore => {
      matches.sort_by(|a, b| {
        b.1
          .priority
          .cmp(&a.1.priority)
          .then_with(|| rule_specificity(b.1).cmp(&rule_specificity(a.1)))
      });
      if matches.len() > 1
        && matches[0].1.priority == matches[1].1.priority
        && rule_specificity(matches[0].1) == rule_specificity(matches[1].1)
      {
        anyhow::bail!(
          "Conflict: Multiple rules with same priority and specificity match: {} and {}",
          matches[0].0,
          matches[1].0
        );
      }
      Ok(Some(&matches[0].1.profile))
    }
  }
}

pub fn resolve_profile_detailed(ctx: &RuleContext, config: &Config) -> Result<Resolution> {
  let mut matches: Vec<(&String, &Rule)> = Vec::new();

  for (name, rule) in &config.rule {
    if is_match(ctx, rule) {
      matches.push((name, rule));
    }
  }

  let (resolved_profile, matched_rule, reason) = if matches.is_empty() {
    match config.core.default_profile.as_ref() {
      Some(default) => (
        Some(default.clone()),
        None,
        format!(
          "No rule matched; falling back to defaultProfile={}",
          default
        ),
      ),
      None => (
        None,
        None,
        "No rule matched and no defaultProfile is set".to_string(),
      ),
    }
  } else {
    match config.core.rule_mode {
      RuleMode::FirstMatch => {
        let (name, rule) = matches[0];
        (
          Some(rule.profile.clone()),
          Some(name.clone()),
          format!("Matched rule '{}' by first-match order", name),
        )
      }
      RuleMode::HighestScore => {
        matches.sort_by(|a, b| {
          b.1
            .priority
            .cmp(&a.1.priority)
            .then_with(|| rule_specificity(b.1).cmp(&rule_specificity(a.1)))
        });

        if matches.len() > 1
          && matches[0].1.priority == matches[1].1.priority
          && rule_specificity(matches[0].1) == rule_specificity(matches[1].1)
        {
          anyhow::bail!(
            "Conflict: Multiple rules with same priority and specificity match: {} and {}",
            matches[0].0,
            matches[1].0
          );
        }

        let (name, rule) = matches[0];
        (
          Some(rule.profile.clone()),
          Some(name.clone()),
          format!(
            "Matched highest-score rule '{}' (priority={}, specificity={})",
            name,
            rule.priority,
            rule_specificity(rule)
          ),
        )
      }
    }
  };

  Ok(Resolution {
    resolved_profile,
    matched_rule,
    reason,
    context_summary: ContextSummary {
      cwd: ctx.cwd.clone(),
      repo_root: ctx.repo_root.clone(),
      is_submodule: ctx.is_submodule,
      remotes: ctx
        .remotes
        .iter()
        .map(|remote| RemoteInfoSummary {
          name: remote.name.clone(),
          host: remote.host.clone(),
          org: remote.org.clone(),
        })
        .collect(),
    },
  })
}

fn rule_specificity(rule: &Rule) -> u8 {
  let mut score = 0;
  if rule.match_path.is_some() {
    score += 1;
  }
  if rule.match_remote_host.is_some() {
    score += 1;
  }
  if rule.match_remote_org.is_some() {
    score += 1;
  }
  if rule.match_file_exists.is_some() {
    score += 1;
  }
  score
}

fn is_match(ctx: &RuleContext, rule: &Rule) -> bool {
  let mut has_any_condition = false;

  if let Some(ref pattern) = rule.match_path {
    has_any_condition = true;
    let pattern = if pattern.starts_with("~/") {
      if let Some(home) = directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()) {
        pattern.replacen("~", &home.to_string_lossy(), 1)
      } else {
        pattern.clone()
      }
    } else {
      pattern.clone()
    };

    let glob = Glob::new(&pattern).ok();
    if let Some(g) = glob {
      let matcher = g.compile_matcher();
      if !matcher.is_match(&ctx.cwd) {
        return false;
      }
    } else {
      return false;
    }
  }

  if rule.match_remote_host.is_some() || rule.match_remote_org.is_some() {
    has_any_condition = true;

    let remote_match = ctx.remotes.iter().any(|remote| {
      let host_ok = rule
        .match_remote_host
        .as_ref()
        .is_none_or(|host_pattern| remote.host.as_deref() == Some(host_pattern));
      let org_ok = rule
        .match_remote_org
        .as_ref()
        .is_none_or(|org_pattern| remote.org.as_deref() == Some(org_pattern));

      host_ok && org_ok
    });

    if !remote_match {
      return false;
    }
  }

  if let Some(ref filename) = rule.match_file_exists {
    has_any_condition = true;
    let file_exists = ctx
      .repo_root
      .as_ref()
      .is_some_and(|root| root.join(filename).exists());
    if !file_exists {
      return false;
    }
  }

  has_any_condition
}

pub fn check(ctx: &AppContext, cwd: Option<PathBuf>, json: bool) -> Result<()> {
  let cwd = match cwd {
    Some(path) => path,
    None => std::env::current_dir().map_err(|_| GpxError::ResolveCurrentDir)?,
  };
  let rule_ctx = gather_context(&cwd)?;

  let config = ctx.load_config()?;
  config.validate()?;

  let resolution = resolve_profile_detailed(&rule_ctx, &config)?;

  if json {
    println!("{}", serde_json::to_string_pretty(&resolution)?);
  } else {
    section("Check report");
    match resolution.resolved_profile {
      Some(profile) => item("Profile", info(&profile)),
      None => item("Profile", warn("NONE")),
    }
    if let Some(rule) = resolution.matched_rule {
      item("Matched rule", info(&rule));
    } else {
      item("Matched rule", warn("<none>"));
    }
    item("Reason", info(&resolution.reason));
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::{Config, Profile, Rule, RuleMode};

  #[test]
  fn test_resolve_profile_first_match() {
    let mut config = Config::default();
    config.core.rule_mode = RuleMode::FirstMatch;
    config.profile.insert(
      "p1".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.profile.insert(
      "p2".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );

    config.rule.insert(
      "r1".into(),
      Rule {
        profile: "p1".into(),
        priority: 0,
        match_path: Some("/a/**".into()),
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );
    config.rule.insert(
      "r2".into(),
      Rule {
        profile: "p2".into(),
        priority: 0,
        match_path: Some("/a/b/**".into()),
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );

    let ctx = RuleContext {
      cwd: PathBuf::from("/a/b/c"),
      repo_root: None,
      is_submodule: false,
      remotes: vec![],
    };

    let res = resolve_profile(&ctx, &config).unwrap();
    assert_eq!(res, Some(&"p1".to_string()));
  }

  #[test]
  fn test_resolve_profile_highest_score() {
    let mut config = Config::default();
    config.core.rule_mode = RuleMode::HighestScore;
    config.profile.insert(
      "p1".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.profile.insert(
      "p2".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );

    config.rule.insert(
      "r1".into(),
      Rule {
        profile: "p1".into(),
        priority: 10,
        match_path: Some("/a/**".into()),
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );
    config.rule.insert(
      "r2".into(),
      Rule {
        profile: "p2".into(),
        priority: 20,
        match_path: Some("/a/b/**".into()),
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );

    let ctx = RuleContext {
      cwd: PathBuf::from("/a/b/c"),
      repo_root: None,
      is_submodule: false,
      remotes: vec![],
    };

    let res = resolve_profile(&ctx, &config).unwrap();
    assert_eq!(res, Some(&"p2".to_string()));
  }

  #[test]
  fn test_rule_requires_all_remote_conditions_on_same_remote() {
    let mut config = Config::default();
    config.core.rule_mode = RuleMode::FirstMatch;
    config.profile.insert(
      "work".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.profile.insert(
      "personal".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.core.default_profile = Some("personal".into());

    config.rule.insert(
      "corp".into(),
      Rule {
        profile: "work".into(),
        priority: 0,
        match_path: None,
        match_remote_host: Some("github.com".into()),
        match_remote_org: Some("corp-org".into()),
        match_file_exists: None,
      },
    );

    let ctx = RuleContext {
      cwd: PathBuf::from("/tmp/repo"),
      repo_root: Some(PathBuf::from("/tmp/repo")),
      is_submodule: false,
      remotes: vec![RemoteInfo {
        name: "origin".into(),
        host: Some("github.com".into()),
        org: Some("other-org".into()),
      }],
    };

    let res = resolve_profile(&ctx, &config).unwrap();
    assert_eq!(res, Some(&"personal".to_string()));
  }

  #[test]
  fn test_rule_requires_all_conditions_across_types() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path().to_path_buf();

    let mut config = Config::default();
    config.core.rule_mode = RuleMode::FirstMatch;
    config.profile.insert(
      "work".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.profile.insert(
      "personal".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.core.default_profile = Some("personal".into());

    config.rule.insert(
      "corp".into(),
      Rule {
        profile: "work".into(),
        priority: 0,
        match_path: Some(format!("{}/**", repo_root.display())),
        match_remote_host: Some("github.com".into()),
        match_remote_org: Some("corp-org".into()),
        match_file_exists: Some(".gpx-work".into()),
      },
    );

    let ctx = RuleContext {
      cwd: repo_root.join("subdir"),
      repo_root: Some(repo_root),
      is_submodule: false,
      remotes: vec![RemoteInfo {
        name: "origin".into(),
        host: Some("github.com".into()),
        org: Some("corp-org".into()),
      }],
    };

    let res = resolve_profile(&ctx, &config).unwrap();
    assert_eq!(res, Some(&"personal".to_string()));
  }

  #[test]
  fn test_highest_score_uses_specificity_tiebreaker() {
    let mut config = Config::default();
    config.core.rule_mode = RuleMode::HighestScore;
    config.profile.insert(
      "p1".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );
    config.profile.insert(
      "p2".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );

    config.rule.insert(
      "less-specific".into(),
      Rule {
        profile: "p1".into(),
        priority: 10,
        match_path: Some("/repo/**".into()),
        match_remote_host: None,
        match_remote_org: None,
        match_file_exists: None,
      },
    );
    config.rule.insert(
      "more-specific".into(),
      Rule {
        profile: "p2".into(),
        priority: 10,
        match_path: Some("/repo/**".into()),
        match_remote_host: Some("github.com".into()),
        match_remote_org: None,
        match_file_exists: None,
      },
    );

    let ctx = RuleContext {
      cwd: PathBuf::from("/repo/sub"),
      repo_root: Some(PathBuf::from("/repo")),
      is_submodule: false,
      remotes: vec![RemoteInfo {
        name: "origin".into(),
        host: Some("github.com".into()),
        org: Some("org".into()),
      }],
    };

    let res = resolve_profile(&ctx, &config).unwrap();
    assert_eq!(res, Some(&"p2".to_string()));
  }

  #[test]
  fn test_resolve_profile_detailed_contains_required_fields() {
    let mut config = Config::default();
    config.core.rule_mode = RuleMode::FirstMatch;
    config.core.default_profile = Some("personal".into());
    config.profile.insert(
      "personal".into(),
      Profile {
        user: None,
        gpg: None,
        ssh: None,
      },
    );

    let ctx = RuleContext {
      cwd: PathBuf::from("/repo"),
      repo_root: None,
      is_submodule: false,
      remotes: vec![],
    };

    let resolution = resolve_profile_detailed(&ctx, &config).unwrap();
    assert_eq!(resolution.resolved_profile, Some("personal".into()));
    assert!(resolution.matched_rule.is_none());
    assert!(!resolution.reason.is_empty());
    assert_eq!(resolution.context_summary.cwd, PathBuf::from("/repo"));
  }

  #[test]
  fn test_resolve_git_dir_supports_submodule_gitfile() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path().join("child");
    let modules = temp.path().join(".git").join("modules").join("child");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::fs::create_dir_all(&modules).unwrap();
    std::fs::write(repo_root.join(".git"), "gitdir: ../.git/modules/child\n").unwrap();

    let (git_dir, is_submodule) = resolve_git_dir(&repo_root);
    let expected = std::fs::canonicalize(&modules).unwrap_or(modules);
    assert!(is_submodule);
    assert_eq!(git_dir, expected);
  }

  #[test]
  fn test_submodule_and_parent_resolve_independently() {
    fn run_ok(cwd: &Path, args: &[&str]) {
      let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
      assert!(status.success(), "git {:?} failed", args);
    }

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let super_repo = root.join("super");
    let sub_src = root.join("sub-src");
    std::fs::create_dir_all(&super_repo).unwrap();
    std::fs::create_dir_all(&sub_src).unwrap();

    run_ok(&sub_src, &["init"]);
    run_ok(&sub_src, &["config", "user.name", "Tester"]);
    run_ok(&sub_src, &["config", "user.email", "tester@example.com"]);
    std::fs::write(sub_src.join("README.md"), "submodule").unwrap();
    run_ok(&sub_src, &["add", "README.md"]);
    run_ok(&sub_src, &["commit", "-m", "init sub"]);

    run_ok(&super_repo, &["init"]);
    run_ok(&super_repo, &["config", "user.name", "Tester"]);
    run_ok(&super_repo, &["config", "user.email", "tester@example.com"]);
    run_ok(
      &super_repo,
      &[
        "remote",
        "add",
        "origin",
        "https://github.com/parent-org/parent.git",
      ],
    );
    run_ok(
      &super_repo,
      &[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        sub_src.to_str().unwrap(),
        "libs/sub",
      ],
    );
    run_ok(&super_repo, &["commit", "-am", "add submodule"]);
    let submodule_dir = super_repo.join("libs").join("sub");
    run_ok(
      &submodule_dir,
      &[
        "remote",
        "set-url",
        "origin",
        "https://github.com/sub-org/sub.git",
      ],
    );

    let super_ctx = gather_context(&super_repo).unwrap();
    let sub_ctx = gather_context(&submodule_dir).unwrap();

    assert!(!super_ctx.is_submodule);
    assert!(sub_ctx.is_submodule);
    assert_eq!(super_ctx.repo_root.as_deref(), Some(super_repo.as_path()));
    assert_eq!(sub_ctx.repo_root.as_deref(), Some(submodule_dir.as_path()));

    let super_origin = super_ctx
      .remotes
      .iter()
      .find(|r| r.name == "origin")
      .unwrap();
    assert_eq!(super_origin.org.as_deref(), Some("parent-org"));
    let sub_origin = sub_ctx.remotes.iter().find(|r| r.name == "origin").unwrap();
    assert_eq!(sub_origin.org.as_deref(), Some("sub-org"));
  }
}
