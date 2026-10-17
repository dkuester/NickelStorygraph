use anyhow::Result;
use argh::FromArgs;
use serde_json::json;

use crate::emit_json;
use crate::log;
use crate::storygraph;
use crate::utils::VERSION;

/// Switch the linked edition of a book.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "switch-edition")]
pub struct SwitchEdition {
  /// storygraph slug of the currently linked edition
  #[argh(option)]
  pub from_id: String,

  /// storygraph slug of the edition to switch to
  #[argh(option)]
  pub to_id: String,
}

pub fn run(args: SwitchEdition) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);
  storygraph::switch_edition(&args.from_id, &args.to_id)?;
  emit_json!("{}", json!({ "success": true }));
  Ok(())
}
