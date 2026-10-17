use anyhow::{Context, Result};
use argh::FromArgs;

use crate::emit_json;
use crate::log;
use crate::storygraph;

/// Log in with email and password, returning the session cookies StoryGraph
/// issued so they can be written to config.ini.
///
/// The password is read from stdin (a single line) rather than a positional
/// argument so it never appears in the process list on the device.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "login")]
pub struct Login {
  /// storygraph account email address
  #[argh(positional)]
  pub email: String,
}

pub fn run(args: Login) -> Result<()> {
  // Log the email for diagnostics, but never the password.
  log!("login: {}", args.email);

  let mut password = String::new();
  std::io::stdin()
    .read_line(&mut password)
    .context("Failed to read password from stdin")?;
  let password = password.trim_end_matches(['\r', '\n']);

  let result = storygraph::login(&args.email, password)?;
  emit_json!("{}", serde_json::to_string(&result)?);
  Ok(())
}
