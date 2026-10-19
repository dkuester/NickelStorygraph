use anyhow::Result;
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

/// Extracts `(session_cookie, remember_user_token)` from bare `key=value` lines.
fn parse_cookies(content: &str) -> Result<(String, String)> {
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

  Ok((session_cookie, remember_user_token))
}

pub fn run(args: ImportCookies) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);

  let content = std::fs::read_to_string(&args.path).map_err(|_| {
    UserError(
      "No cookies file found. Put <i>cookies.ini</i> into <i>.adds/NickelStorygraph/</i> first."
        .to_owned(),
    )
  })?;

  let (session_cookie, remember_user_token) = parse_cookies(&content)?;
  config::set_cookies(&session_cookie, &remember_user_token)?;

  // The cookies now live in config.ini; don't leave a second plaintext copy of
  // the session token lying around on onboard storage.
  if let Err(err) = std::fs::remove_file(&args.path) {
    log!("could not delete cookies file {}: {}", args.path, err);
  }

  emit_json!("{}", json!({ "imported": true }));
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_cookies_reads_both_values() {
    let (session, token) =
      parse_cookies("session_cookie = abc%3D\nremember_user_token=eyXYZ\nother=1").unwrap();
    assert_eq!(session, "abc%3D");
    assert_eq!(token, "eyXYZ");
  }

  #[test]
  fn parse_cookies_requires_both_values() {
    assert!(parse_cookies("session_cookie=abc\n").is_err());
    assert!(parse_cookies("remember_user_token=xyz\n").is_err());
  }
}
