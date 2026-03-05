use anyhow::Result;
use std::path::PathBuf;

use crate::config::AppContext;
use crate::rules::{gather_context, resolve_profile_detailed};

pub fn ssh_eval_matches(
  ctx: &AppContext,
  expected_profile: &str,
  cwd: Option<PathBuf>,
) -> Result<bool> {
  let config = ctx.load_config()?;
  config.validate()?;

  let cwd = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
  let rule_ctx = gather_context(&cwd)?;
  let resolution = resolve_profile_detailed(&rule_ctx, &config)?;

  Ok(resolution.resolved_profile.as_deref() == Some(expected_profile))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::{Config, Profile};
  use crate::rules::RuleContext;

  #[test]
  fn test_expected_profile_match_logic() {
    let cfg = Config {
      profile: [(
        "work".to_string(),
        Profile {
          user: None,
          gpg: None,
          ssh: None,
        },
      )]
      .into_iter()
      .collect(),
      ..Config::default()
    };
    let ctx = RuleContext {
      cwd: PathBuf::from("/tmp"),
      repo_root: None,
      is_submodule: false,
      remotes: vec![],
    };
    let res = crate::rules::resolve_profile_detailed(&ctx, &cfg).unwrap();
    assert_ne!(res.resolved_profile.as_deref(), Some("work"));
  }
}
