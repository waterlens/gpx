use thiserror::Error;

#[derive(Debug, Error)]
pub enum GpxError {
  #[error("failed to resolve current working directory")]
  ResolveCurrentDir,
  #[error("invalid path: missing parent directory for {0}")]
  MissingParent(String),
  #[error("core.mode=repo-local requires running inside a git repository")]
  RepoLocalOutsideRepo,
  #[error(
    "linked worktree detected but extensions.worktreeConfig is disabled; enable it with `git config extensions.worktreeConfig true`, or set worktree.allowSharedFallback=true"
  )]
  WorktreeConfigRequired,
}
