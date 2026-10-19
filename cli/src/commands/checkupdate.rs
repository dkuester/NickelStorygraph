use anyhow::Result;
use argh::FromArgs;

use crate::emit_json;
use crate::log;
use crate::updater;
use crate::utils::VERSION;

/// Check for a newer release of NickelStorygraph on GitHub.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "check-update")]
pub struct CheckUpdate {}

pub fn run(_args: CheckUpdate) -> Result<()> {
  log!("{} check-update", *VERSION);
  emit_json!("{}", updater::check_update()?);
  Ok(())
}
