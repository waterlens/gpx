use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use owo_colors::OwoColorize;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod cli;
mod config;
mod doctor;
mod error;
mod gitops;
mod hooks;
mod list;
mod output;
mod rules;
mod run;
mod sshops;
mod state;

use cli::{Cli, Commands};
use config::AppContext;

fn main() {
  if let Err(err) = run() {
    eprintln!("{}", format!("error: {}", err).red().bold());
    for cause in err.chain().skip(1) {
      eprintln!("{}", format!("  caused by: {}", cause).yellow());
    }
    std::process::exit(1);
  }
}

fn run() -> Result<()> {
  // Initialize tracing
  let subscriber = FmtSubscriber::builder()
    .with_max_level(Level::WARN)
    .finish();
  tracing::subscriber::set_global_default(subscriber)?;

  let ctx = AppContext::new()?;
  let Cli {
    profile: top_level_profile,
    command,
    args,
  } = Cli::parse();

  if top_level_profile.is_some() && !matches!(&command, None | Some(Commands::Git { .. })) {
    bail!(
      "--profile/-p is only valid for `gpx git ...` or passthrough mode (`gpx -p ... -- <command>`)"
    );
  }

  match command {
    Some(command) => match command {
      Commands::Init => {
        info!("Initializing gpx...");
        gitops::init(&ctx)?;
      }
      Commands::Deinit => {
        info!("Deinitializing gpx...");
        gitops::deinit(&ctx)?;
      }
      Commands::Doctor => {
        info!("Running doctor...");
        doctor::run(&ctx)?;
      }
      Commands::Status { verbose } => {
        info!("Checking status (verbose: {})...", verbose);
        state::status(&ctx, verbose)?;
      }
      Commands::List { kind, json } => {
        info!(
          "Listing config entities (kind: {:?}, json: {})...",
          kind, json
        );
        list::run(&ctx, kind, json)?;
      }
      Commands::Check { cwd, json } => {
        info!("Checking profile for {:?} (json: {})...", cwd, json);
        rules::check(&ctx, cwd, json)?;
      }
      Commands::Apply {
        cwd,
        profile,
        dry_run,
        hook_mode,
      } => {
        info!(
          "Applying profile {:?} for {:?} (dry_run: {}, hook_mode: {})...",
          profile, cwd, dry_run, hook_mode
        );
        gitops::apply(&ctx, cwd, profile, dry_run, hook_mode)?;
      }
      Commands::Hook { command } => {
        info!("Hook command: {:?}", command);
        hooks::handle(&ctx, command)?;
      }
      Commands::Git { profile, args } => {
        let profile = profile.or(top_level_profile);
        info!("Running git command with profile {:?}: {:?}", profile, args);
        run::execute_git(&ctx, profile, args)?;
      }
      Commands::SshEval { profile, cwd } => {
        let matched = sshops::ssh_eval_matches(&ctx, &profile, cwd)?;
        std::process::exit(if matched { 0 } else { 1 });
      }
    },
    None => {
      if !args.is_empty() {
        info!(
          "Running passthrough command with profile {:?}: {:?}",
          top_level_profile, args
        );
        run::execute_passthrough(&ctx, top_level_profile, args)?;
      } else {
        if top_level_profile.is_some() {
          bail!("--profile/-p requires a command in passthrough mode");
        }
        use clap::CommandFactory;
        Cli::command().print_help()?;
        println!();
      }
    }
  }

  Ok(())
}
