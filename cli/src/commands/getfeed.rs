use anyhow::Result;
use argh::FromArgs;
use serde_json::json;

use crate::emit_json;
use crate::log;
use crate::storygraph;
use crate::utils::VERSION;

/// Fetch the community activity feed from StoryGraph.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "get-feed")]
pub struct GetFeed {}

pub fn run(_args: GetFeed) -> Result<()> {
  log!("{} get-feed", &*VERSION);
  let items = storygraph::get_feed()?;
  emit_json!("{}", json!({ "items": items }));
  Ok(())
}
