use anyhow::Result;
use argh::FromArgs;

use crate::emit_json;
use crate::log;
use crate::storygraph;
use crate::utils::VERSION;

/// Fetch the user's current reading streak from StoryGraph.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "get-streak")]
pub struct GetStreak {}

pub fn run(_args: GetStreak) -> Result<()> {
  log!("{} get-streak", &*VERSION);
  let streak = storygraph::get_streak()?;
  emit_json!("{}", serde_json::to_string(&streak).unwrap());
  Ok(())
}
