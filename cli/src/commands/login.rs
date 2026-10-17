use anyhow::Result;
use argh::FromArgs;

use crate::log;
use crate::storygraph;

/// Log in with email and password, returning the session cookies StoryGraph
/// issued so they can be written to config.ini.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "login")]
pub struct Login {
  /// storygraph account email address
  #[argh(positional)]
  pub email: String,

  /// storygraph account password
  #[argh(positional)]
  pub password: String,
}

pub fn run(args: Login) -> Result<()> {
  // Log the email for diagnostics, but never the password.
  log!("login: {}", args.email);
  let result = storygraph::login(&args.email, &args.password)?;
  log!("BEGIN_JSON\n{}", serde_json::to_string(&result)?);
  Ok(())
}
