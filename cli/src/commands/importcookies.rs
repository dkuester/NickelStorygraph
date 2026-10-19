use anyhow::{Context, Result};
use argh::FromArgs;
use serde_json::json;

use crate::config;
use crate::emit_json;
use crate::log;
use crate::utils::{UserError, VERSION};

/// Import StoryGraph cookies from a file into config.ini.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "import-cookies")]
pub struct ImportCookies {
  /// path to the cookies file (bare `key=value` lines: session_cookie and remember_user_token)
  #[argh(positional)]
  pub path: String,
}

pub fn run(args: ImportCookies) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);

  let content = std::fs::read_to_string(&args.path).context("Failed to read cookies file")?;

  let mut session_cookie = String::new();
  let mut remember_user_token = String::new();
  for line in content.lines() {
    if let Some((key, value)) = line.split_once('=') {
      match key.trim() {
        "session_cookie" => session_cookie = value.trim().to_owned(),
        "remember_user_token" => remember_user_token = value.trim().to_owned(),
        _ => {}
      }
    }
  }

  if session_cookie.is_empty() {
    return Err(UserError("No <i>session_cookie</i> found in the cookies file.".to_owned()).into());
  }
  if remember_user_token.is_empty() {
    return Err(
      UserError("No <i>remember_user_token</i> found in the cookies file.".to_owned()).into(),
    );
  }

  config::set_cookies(&session_cookie, &remember_user_token)?;

  emit_json!("{}", json!({ "imported": true }));
  Ok(())
}
