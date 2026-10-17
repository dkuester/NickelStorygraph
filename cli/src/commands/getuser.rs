use anyhow::Result;
use argh::FromArgs;
use serde_json::json;

use crate::emit_json;
use crate::storygraph;
use crate::utils::VERSION;
use crate::{debug_log, log};

/// Verify session and return basic user info.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "get-user")]
pub struct GetUser {}

pub fn run(args: GetUser) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);
  match storygraph::get_user() {
    Ok(user) => emit_json!("{}", serde_json::to_string(&user)?),
    Err(e) => {
      // Non-fatal: settings dialog shows "Unknown" instead of crashing.
      // Most likely cause is user_slug not set in config.ini.
      debug_log!("get_user failed (set user_slug in config.ini to avoid this): {e:#}");
      emit_json!("{}", json!({ "username": "", "slug": "" }));
    }
  }
  Ok(())
}
