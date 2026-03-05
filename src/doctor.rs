use crate::config::AppContext;
use crate::config::ConfigSource;
use crate::output::{fail, info, ok, section, warn};
use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn run(ctx: &AppContext) -> Result<()> {
  let loaded = ctx.load_config_with_info()?;

  section("Doctor report");
  println!("- Config dir: {}", ctx.config_dir.display());
  println!("- Cache dir: {}", ctx.cache_dir.display());
  println!("- State dir: {}", ctx.state_dir.display());

  println!(
    "- Config source: {}",
    match loaded.source {
      ConfigSource::Toml => info("config.toml"),
      ConfigSource::Ini => info("config (INI)"),
      ConfigSource::Default => warn("defaults"),
    }
  );
  if loaded.both_configs_present {
    println!(
      "- Warning: {} both config.toml and config exist; config.toml takes precedence.",
      warn("WARN")
    );
  }

  match loaded.config.validate() {
    Ok(_) => println!("- Config validation: {}", ok("OK")),
    Err(e) => println!("- Config validation: {} ({})", fail("FAIL"), e),
  }
  if loaded.config.ssh.dynamic_match {
    println!("- SSH dynamicMatch: {} (experimental)", warn("ENABLED"));
  } else {
    println!("- SSH dynamicMatch: {} (default)", ok("DISABLED"));
  }

  let gitconfig_path = match directories::UserDirs::new() {
    Some(u) => u.home_dir().join(".gitconfig"),
    None => {
      println!("- Home directory resolution: {}", fail("FAIL"));
      return Ok(());
    }
  };
  let active_include = ctx.git_active_include();
  let include_needle = active_include.to_string_lossy().to_string();

  if gitconfig_path.exists() {
    let content = std::fs::read_to_string(&gitconfig_path)?;
    if content.contains(&include_needle) {
      println!("- ~/.gitconfig include: {}", ok("OK"));
    } else {
      println!(
        "- ~/.gitconfig include: {} (run `gpx init`)",
        fail("MISSING")
      );
    }
  } else {
    println!("- ~/.gitconfig: {}", fail("MISSING"));
  }

  if ctx.git_active_include().exists() {
    println!(
      "- Active include file: present ({})",
      ctx.git_active_include().display()
    );
    let active_content = std::fs::read_to_string(ctx.git_active_include())?;
    if active_content.contains("path =") {
      println!("- Active include target: {}", ok("OK"));
    } else {
      println!(
        "- Active include target: {} (missing include path)",
        fail("INVALID")
      );
    }
  } else {
    println!(
      "- Active include file: {} (run `gpx apply`)",
      fail("MISSING")
    );
  }

  if ctx.ssh_include_file().exists() {
    println!(
      "- SSH include file: {} ({})",
      ok("PRESENT"),
      ctx.ssh_include_file().display()
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
        println!("- SSH include permissions: {} (0600)", ok("OK"));
      } else {
        println!(
          "- SSH include permissions: {} ({:o}, expected 600)",
          warn("WARN"),
          mode
        );
      }
    }

    if loaded.config.ssh.dynamic_match {
      if ssh_include_content.contains("Match exec") && ssh_include_content.contains("gpx ssh-eval")
      {
        println!("- SSH dynamic match include: {}", ok("OK"));
      } else {
        println!(
          "- SSH dynamic match include: {} (run `gpx apply` to regenerate dynamic Match exec blocks)",
          fail("MISSING")
        );
      }
    }
  } else {
    println!(
      "- SSH include file: {} (will be created on `gpx apply`)",
      fail("MISSING")
    );
    if loaded.config.ssh.dynamic_match {
      println!(
        "- SSH dynamic match include: {} (run `gpx apply` to generate Match exec blocks)",
        fail("MISSING")
      );
    }
  }
  check_ssh_include_mounted(&ctx.ssh_include_file())?;

  match Command::new("git").arg("--version").output() {
    Ok(out) => {
      let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
      if version.is_empty() {
        println!("- Git version: {}", warn("UNKNOWN"));
      } else {
        println!("- Git version: {}", version);
        if let Some((major, minor)) = parse_git_version(&version) {
          // Heuristic threshold for broad compatibility with GIT_CONFIG_COUNT based injection.
          if (major, minor) >= (2, 18) {
            println!(
              "- Run mode compatibility: {} (GIT_CONFIG_COUNT supported)",
              ok("OK")
            );
          } else {
            println!(
              "- Run mode compatibility: {} (git < 2.18 may not support env-based config override reliably)",
              warn("WARN")
            );
          }
        } else {
          println!(
            "- Run mode compatibility: {} (failed to parse git version)",
            warn("UNKNOWN")
          );
        }
      }
    }
    Err(e) => println!("- Git version check failed: {} ({})", fail("FAIL"), e),
  }

  check_worktree_risk()?;

  Ok(())
}

fn check_ssh_include_mounted(ssh_include: &Path) -> Result<()> {
  let home = match directories::UserDirs::new() {
    Some(u) => u.home_dir().to_path_buf(),
    None => return Ok(()),
  };
  let ssh_config = home.join(".ssh").join("config");
  let include_needle = ssh_include.to_string_lossy().to_string();

  if !ssh_config.exists() {
    println!("- ~/.ssh/config: {}", fail("MISSING"));
    return Ok(());
  }

  let content = std::fs::read_to_string(&ssh_config)?;
  if content.contains(&include_needle) {
    println!("- ~/.ssh/config include: {}", ok("OK"));
  } else {
    println!(
      "- ~/.ssh/config include: {} (add `Include {}`)",
      fail("MISSING"),
      include_needle
    );
  }
  Ok(())
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
    println!(
      "- Worktree check: {} (not in a git repository)",
      warn("SKIPPED")
    );
    return Ok(());
  };
  if !git_dir.status.success() || !git_common_dir.status.success() {
    println!(
      "- Worktree check: {} (not in a git repository)",
      warn("SKIPPED")
    );
    return Ok(());
  }

  let git_dir = String::from_utf8_lossy(&git_dir.stdout).trim().to_string();
  let git_common_dir = String::from_utf8_lossy(&git_common_dir.stdout)
    .trim()
    .to_string();
  let in_linked_worktree = git_dir != git_common_dir;

  if in_linked_worktree {
    println!(
      "- Worktree mode: {} (linked worktree detected)",
      warn("LINKED")
    );
  } else {
    println!("- Worktree mode: {} (main worktree/repo)", ok("MAIN"));
  }

  let worktree_cfg = Command::new("git")
    .args(["config", "--bool", "extensions.worktreeConfig"])
    .output()?;
  if worktree_cfg.status.success() {
    let enabled = String::from_utf8_lossy(&worktree_cfg.stdout).trim() == "true";
    if enabled {
      println!("- worktreeConfig: {}", ok("ENABLED"));
    } else if in_linked_worktree {
      println!(
        "- worktreeConfig: {} disabled in linked worktree (profile switching may leak across worktrees)",
        warn("WARN")
      );
      println!(
        "- Suggested fix: {}",
        info("git config extensions.worktreeConfig true")
      );
    } else {
      println!("- worktreeConfig: {}", ok("DISABLED"));
    }
  } else if in_linked_worktree {
    println!(
      "- worktreeConfig: {} unset in linked worktree",
      warn("WARN")
    );
    println!(
      "- Suggested fix: {}",
      info("git config extensions.worktreeConfig true")
    );
  } else {
    println!("- worktreeConfig: {}", warn("UNSET"));
  }

  Ok(())
}
