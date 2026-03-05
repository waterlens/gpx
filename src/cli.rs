use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gpx")]
#[command(about = "Git Profile Extension", long_about = None)]
pub struct Cli {
  /// Profile override for passthrough mode (when no subcommand is used)
  #[arg(short = 'p', long, value_name = "PROFILE")]
  pub profile: Option<String>,

  #[command(subcommand)]
  pub command: Option<Commands>,

  /// Command and arguments to pass through (when no subcommand is used)
  #[arg(trailing_var_arg = true, value_name = "CMD")]
  pub args: Vec<String>,
}

#[derive(Subcommand)]
pub enum Commands {
  /// Initialize GPX setup in ~/.gitconfig
  Init,
  /// Remove GPX bootstrap setup from ~/.gitconfig
  Deinit,
  /// Run full diagnostics
  Doctor,
  /// Show current GPX status
  Status {
    #[arg(short, long)]
    verbose: bool,
  },
  /// List configured profiles or rules
  List {
    #[arg(value_enum)]
    kind: Option<ListKind>,
    #[arg(long)]
    json: bool,
  },
  /// Check which profile would be applied for a path
  Check {
    #[arg(long, value_name = "PATH")]
    cwd: Option<PathBuf>,
    #[arg(long)]
    json: bool,
  },
  /// Apply profile configuration (usually triggered by hooks)
  Apply {
    #[arg(long, value_name = "PATH")]
    cwd: Option<PathBuf>,
    #[arg(long, value_name = "PROFILE")]
    profile: Option<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, hide = true)]
    hook_mode: bool,
  },
  /// Install or uninstall hooks
  Hook {
    #[command(subcommand)]
    command: HookCommands,
  },
  /// Run a git command with profile environment (zero-persistence)
  #[command(trailing_var_arg = true)]
  Git {
    #[arg(short = 'p', long, value_name = "PROFILE")]
    profile: Option<String>,
    /// Git arguments to pass through
    args: Vec<String>,
  },
  /// Internal command for SSH Match exec profile evaluation
  #[command(hide = true)]
  SshEval {
    #[arg(long, value_name = "PROFILE")]
    profile: String,
    #[arg(long, value_name = "PATH")]
    cwd: Option<PathBuf>,
  },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ListKind {
  Profiles,
  Rules,
}

#[derive(Subcommand, Debug)]
pub enum HookCommands {
  /// Install shell or git hooks
  Install {
    #[arg(long)]
    shell: Option<Shell>,
    #[arg(long)]
    git: bool,
  },
  /// Uninstall shell or git hooks
  Uninstall {
    #[arg(long)]
    shell: Option<Shell>,
    #[arg(long)]
    git: bool,
  },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum Shell {
  Bash,
  Zsh,
  Fish,
  Nushell,
  Tcsh,
  Elvish,
}
