use std::error::Error;
use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use chrono::Local;
use itertools::Itertools;
use serde_json::json;

use crate::config::CONFIG;
use crate::isbn::get_isbn;

/// An error whose message is already user-facing and should be shown to the
/// user verbatim (without the generic "unexpected error" wrapper).
#[derive(Debug)]
pub struct UserError(pub String);

impl fmt::Display for UserError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl Error for UserError {}

#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! debug_log {
  ($($t:tt)*) => {{
    crate::utils::debug_log_inner(&format!($($t)*));
  }};
}

// `log!` writes human diagnostics to stderr and to the on-device log file.
// stdout is reserved for machine-readable JSON (see `emit_json!`).
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! log {
  ($($t:tt)*) => {{
    let msg = format!($($t)*);
    crate::utils::debug_log_inner(&msg);
    eprintln!("{}", msg);
  }};
}

/// Writes a JSON document to stdout — the CLI's machine-readable channel that
/// the hook parses. Nothing else may be written to stdout, so the hook can
/// treat its entire content as JSON (no BEGIN_JSON sentinel required).
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! emit_json {
  ($($t:tt)*) => {{
    println!("{}", format!($($t)*));
  }};
}

pub static VERSION: LazyLock<&str> =
  LazyLock::new(|| option_env!("VERSION").unwrap_or(env!("CARGO_PKG_VERSION")));

// In-memory buffer for the current CLI invocation; flushed to disk by write_logfile().
static LOG: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

pub fn debug_log_inner(msg: &str) {
  LOG
    .lock()
    .unwrap()
    .push_str(&format!("{} {msg}\n", Local::now().format("%c")));
}

/// Appends the current invocation's log buffer to a single persistent log file.
/// All CLI calls share one file so the full session history is in one place.
pub fn write_logfile() {
  let res = || -> Result<()> {
    let log_path = std::env::current_exe()
      .context("Failed to get current binary path")?
      .as_path()
      .parent()
      .context("Failed to get current binary directory")?
      .join("nickelstoregraph.log");

    let mut file = OpenOptions::new()
      .create(true)
      .append(true)
      .open(log_path)
      .context("Failed to open log file")?;

    file
      .write_all(LOG.lock().unwrap().as_bytes())
      .context("Failed to write log file")
  }();

  if let Err(e) = res {
    eprintln!(
      "Encountered an unexpected error. Please report this.<br><br>{:#}",
      e.chain().join("<br>> ")
    );
  }
}

/// Returns `(book_slug, isbns)`. If `linked_id` is set, returns it directly.
/// Otherwise extracts ISBNs from the Kobo content ID (EPUB path).
pub fn normalize_identifiers(
  linked_id: Option<String>,
  content_id: Option<&str>,
) -> Result<(String, Vec<String>)> {
  match (linked_id, content_id) {
    (Some(id), _) if !id.is_empty() => Ok((id, Vec::new())),
    (_, Some(content_id)) => Ok((String::new(), get_isbn(content_id))),
    _ => Err(UserError("One of --content-id or --linked-id is required".to_owned()).into()),
  }
}

/// Emits a BOOK_NOT_FOUND JSON error to stdout and exits cleanly.
/// Used when a book can't be found by ISBN or linked ID so the hook
/// can show a user-facing message without treating it as a crash.
pub fn book_not_found(msg: &str) -> ! {
  emit_json!(
    "{}",
    json!({ "error_code": "BOOK_NOT_FOUND", "message": msg })
  );

  if CONFIG.debug {
    write_logfile();
  }

  std::process::exit(0);
}
