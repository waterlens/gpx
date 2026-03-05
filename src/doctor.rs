use crate::config::AppContext;
use crate::config::ConfigSource;
use crate::output::{fail, info, item, note, ok, section, strong, warn};
use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn run(ctx: &AppContext) -> Result<()> {
  let loaded = ctx.load_config_with_info()?;
  let mut suggest_init = false;
  let mut suggest_apply = false;
  let mut suggest_profile_setup = false;

  section("Doctor report");
  item("Config dir", ctx.config_dir.display());
  item("Cache dir", ctx.cache_dir.display());
  item("State dir", ctx.state_dir.display());

  item(
    "Config source",
    match loaded.source {
      ConfigSource::Toml => info("config.toml"),
      ConfigSource::Ini => info("config (INI)"),
      ConfigSource::Default => warn("defaults"),
    },
  );
  if loaded.both_configs_present {
    note(format!(
      "Warning: {} both config.toml and config exist; config.toml takes precedence.",
      warn("WARN")
    ));
  }

  match loaded.config.validate() {
    Ok(_) => item("Config validation", ok("OK")),
    Err(e) => item("Config validation", format!("{} ({})", fail("FAIL"), e)),
  }
  if loaded.config.profile.is_empty() {
    suggest_profile_setup = true;
    let preferred_config = preferred_config_path(ctx);
    note(format!(
      "No profiles found. Edit {} and add at least one [profile.<name>.user] plus [core] defaultProfile.",
      preferred_config.display()
    ));
  }
  if loaded.config.ssh.dynamic_match {
    item(
      "SSH dynamicMatch",
      format!("{} (experimental)", warn("ENABLED")),
    );
  } else {
    item("SSH dynamicMatch", format!("{} (default)", ok("DISABLED")));
  }

  let home = match directories::UserDirs::new() {
    Some(u) => u.home_dir().to_path_buf(),
    None => {
      item("Home directory resolution", fail("FAIL"));
      return Ok(());
    }
  };
  let gitconfig_path = home.join(".gitconfig");
  let active_include = ctx.git_active_include();
  let include_candidates = include_path_candidates(&active_include, &home);

  if gitconfig_path.exists() {
    let content = std::fs::read_to_string(&gitconfig_path)?;
    if contains_git_include_path(&content, &include_candidates) {
      item("~/.gitconfig include", ok("OK"));
    } else {
      item(
        "~/.gitconfig include",
        format!("{} (run `gpx init`)", fail("MISSING")),
      );
    }
  } else {
    item("~/.gitconfig", fail("MISSING"));
  }

  if ctx.git_active_include().exists() {
    item(
      "Active include file",
      format!("{} ({})", ok("OK"), ctx.git_active_include().display()),
    );
    let active_content = std::fs::read_to_string(ctx.git_active_include())?;
    if has_active_include_target(&active_content) {
      item("Active include target", ok("OK"));
    } else {
      suggest_apply = true;
      item(
        "Active include target",
        format!(
          "{} (run `gpx apply` to activate one profile)",
          warn("UNSET")
        ),
      );
    }
  } else {
    suggest_init = true;
    item(
      "Active include file",
      format!("{} (run `gpx init`)", fail("MISSING")),
    );
  }

  if ctx.ssh_include_file().exists() {
    item(
      "SSH include file",
      format!("{} ({})", ok("OK"), ctx.ssh_include_file().display()),
    );
    let ssh_include_content = std::fs::read_to_string(ctx.ssh_include_file())?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mode = std::fs::metadata(ctx.ssh_include_file())?
        .permissions()
        .mode()
        & 0o777;
      if mode == 0o600 {
        item("SSH include permissions", format!("{} (0600)", ok("OK")));
      } else {
        item(
          "SSH include permissions",
          format!("{} ({:o}, expected 600)", warn("WARN"), mode),
        );
      }
    }

    if loaded.config.ssh.dynamic_match {
      if ssh_include_content.contains("Match exec") && ssh_include_content.contains("gpx ssh-eval")
      {
        item("SSH dynamic match include", ok("OK"));
      } else {
        item(
          "SSH dynamic match include",
          format!(
            "{} (run `gpx apply` to regenerate dynamic Match exec blocks)",
            fail("MISSING")
          ),
        );
      }
    }
  } else {
    suggest_init = true;
    item(
      "SSH include file",
      format!("{} (run `gpx init`)", warn("UNSET")),
    );
    if loaded.config.ssh.dynamic_match {
      item(
        "SSH dynamic match include",
        format!(
          "{} (run `gpx apply` to generate Match exec blocks)",
          fail("MISSING")
        ),
      );
    }
  }
  let ssh_include_mounted = check_ssh_include_mounted(&ctx.ssh_include_file())?;

  match Command::new("git").arg("--version").output() {
    Ok(out) => {
      let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
      if version.is_empty() {
        item("Git version", warn("UNKNOWN"));
      } else {
        item("Git version", version.as_str());
        if let Some((major, minor)) = parse_git_version(&version) {
          // Heuristic threshold for broad compatibility with GIT_CONFIG_COUNT based injection.
          if (major, minor) >= (2, 18) {
            item(
              "Run mode compatibility",
              format!("{} (GIT_CONFIG_COUNT supported)", ok("OK")),
            );
          } else {
            item(
              "Run mode compatibility",
              format!(
                "{} (git < 2.18 may not support env-based config override reliably)",
                warn("WARN")
              ),
            );
          }
        } else {
          item(
            "Run mode compatibility",
            format!("{} (failed to parse git version)", warn("UNKNOWN")),
          );
        }
      }
    }
    Err(e) => item(
      "Git version check failed",
      format!("{} ({})", fail("FAIL"), e),
    ),
  }

  check_worktree_risk()?;

  if suggest_init {
    note(format!(
      "{} run `gpx init` to bootstrap include files.",
      strong("Next step:")
    ));
  }
  if suggest_profile_setup {
    note(format!(
      "{} define profiles/rules in config, then run `gpx apply`.",
      strong("Next step:")
    ));
  } else if suggest_apply {
    note(format!(
      "{} run `gpx apply` to generate active Git/SSH includes.",
      strong("Next step:")
    ));
  }
  if !ssh_include_mounted && !suggest_init {
    note(format!(
      "{} run `gpx init` to restore GPX-managed SSH include.",
      strong("Next step:")
    ));
  }
  note(format!(
    "{} install hook for auto refresh: `gpx hook install --shell ...` and/or `gpx hook install --git`.",
    strong("Note:")
  ));

  Ok(())
}

fn check_ssh_include_mounted(ssh_include: &Path) -> Result<bool> {
  let home = match directories::UserDirs::new() {
    Some(u) => u.home_dir().to_path_buf(),
    None => return Ok(true),
  };
  let ssh_config = home.join(".ssh").join("config");
  let include_candidates = include_path_candidates(ssh_include, &home);
  let display_include = display_path_for_config(ssh_include, &home);

  if !ssh_config.exists() {
    item("~/.ssh/config", fail("MISSING"));
    return Ok(false);
  }

  let content = std::fs::read_to_string(&ssh_config)?;
  if contains_ssh_include_path(&content, &include_candidates) {
    item("~/.ssh/config include", ok("OK"));
    Ok(true)
  } else {
    item(
      "~/.ssh/config include",
      format!(
        "{} (run `gpx init` to restore managed include: {})",
        fail("MISSING"),
        display_include
      ),
    );
    Ok(false)
  }
}

fn has_active_include_target(content: &str) -> bool {
  content
    .lines()
    .map(str::trim)
    .any(|line| line.starts_with("path ="))
}

fn preferred_config_path(ctx: &AppContext) -> std::path::PathBuf {
  let toml = ctx.config_file();
  let ini = ctx.config_file_ini();
  if toml.exists() || !ini.exists() {
    toml
  } else {
    ini
  }
}

fn include_path_candidates(path: &Path, home: &Path) -> Vec<String> {
  let abs = path.to_string_lossy().to_string();
  let pretty = display_path_for_config(path, home);
  if abs == pretty {
    vec![abs]
  } else {
    vec![abs, pretty]
  }
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

fn contains_git_include_path(content: &str, candidates: &[String]) -> bool {
  content.lines().map(str::trim).any(|line| {
    line
      .strip_prefix("path =")
      .map(|path| candidates.iter().any(|c| c == path.trim()))
      .unwrap_or(false)
  })
}

fn contains_ssh_include_path(content: &str, candidates: &[String]) -> bool {
  content.lines().map(str::trim).any(|line| {
    if let Some(path) = line.strip_prefix("Include") {
      candidates.iter().any(|c| c == path.trim())
    } else {
      false
    }
  })
}

fn parse_git_version(version_line: &str) -> Option<(u32, u32)> {
  let mut parts = version_line.split_whitespace();
  let _git = parts.next()?;
  let _version = parts.next()?;
  let numeric = parts.next()?;
  let mut nums = numeric.split('.');
  let major = nums.next()?.parse::<u32>().ok()?;
  let minor = nums.next()?.parse::<u32>().ok()?;
  Some((major, minor))
}

fn check_worktree_risk() -> Result<()> {
  let git_dir = Command::new("git")
    .args(["rev-parse", "--git-dir"])
    .output();
  let git_common_dir = Command::new("git")
    .args(["rev-parse", "--git-common-dir"])
    .output();

  let (Ok(git_dir), Ok(git_common_dir)) = (git_dir, git_common_dir) else {
    item(
      "Worktree check",
      format!("{} (not in a git repository)", warn("SKIPPED")),
    );
    return Ok(());
  };
  if !git_dir.status.success() || !git_common_dir.status.success() {
    item(
      "Worktree check",
      format!("{} (not in a git repository)", warn("SKIPPED")),
    );
    return Ok(());
  }

  let git_dir = String::from_utf8_lossy(&git_dir.stdout).trim().to_string();
  let git_common_dir = String::from_utf8_lossy(&git_common_dir.stdout)
    .trim()
    .to_string();
  let in_linked_worktree = git_dir != git_common_dir;

  if in_linked_worktree {
    item(
      "Worktree mode",
      format!("{} (linked worktree detected)", warn("LINKED")),
    );
  } else {
    item(
      "Worktree mode",
      format!("{} (main worktree/repo)", ok("MAIN")),
    );
  }

  let worktree_cfg = Command::new("git")
    .args(["config", "--bool", "extensions.worktreeConfig"])
    .output()?;
  if worktree_cfg.status.success() {
    let enabled = String::from_utf8_lossy(&worktree_cfg.stdout).trim() == "true";
    if enabled {
      item("worktreeConfig", ok("ENABLED"));
    } else if in_linked_worktree {
      item(
        "worktreeConfig",
        format!(
          "{} disabled in linked worktree (profile switching may leak across worktrees)",
          warn("WARN")
        ),
      );
      item(
        "Suggested fix",
        info("git config extensions.worktreeConfig true"),
      );
    } else {
      item("worktreeConfig", ok("DISABLED"));
    }
  } else if in_linked_worktree {
    item(
      "worktreeConfig",
      format!("{} unset in linked worktree", warn("WARN")),
    );
    item(
      "Suggested fix",
      info("git config extensions.worktreeConfig true"),
    );
  } else {
    item("worktreeConfig", warn("UNSET"));
  }

  Ok(())
}
