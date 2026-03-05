use anyhow::Result;
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
  let cli = Cli::parse();

  match cli.command {
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
      Commands::Run { profile, args } => {
        info!("Running git command with profile {:?}: {:?}", profile, args);
        run::execute(&ctx, profile, args)?;
      }
      Commands::SshEval { profile, cwd } => {
        let matched = sshops::ssh_eval_matches(&ctx, &profile, cwd)?;
        std::process::exit(if matched { 0 } else { 1 });
      }
    },
    None => {
      if !cli.args.is_empty() {
        info!("Running git command (default): {:?}", cli.args);
        run::execute(&ctx, None, cli.args)?;
      } else {
        use clap::CommandFactory;
        Cli::command().print_help()?;
        println!();
      }
    }
  }

  Ok(())
}
