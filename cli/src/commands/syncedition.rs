use anyhow::Result;
use argh::FromArgs;
use serde_json::json;

use crate::emit_json;
use crate::log;
use crate::storygraph;
use crate::utils::{UserError, VERSION, normalize_identifiers};

/// Sync the linked edition from the user's StoryGraph currently-reading shelf.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "sync-edition")]
pub struct SyncEdition {
  /// storygraph book id (linked edition)
  #[argh(option)]
  pub linked_id: Option<String>,

  /// kobo content ID
  #[argh(option)]
  pub content_id: Option<String>,
}

pub fn run(args: SyncEdition) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);
  let (book_id, _) = normalize_identifiers(args.linked_id, args.content_id.as_deref())?;
  if book_id.is_empty() {
    return Err(UserError("No linked book found. Please link a book first.".to_owned()).into());
  }
  let found_id = storygraph::find_current_edition(&book_id).map_err(|_| {
    UserError("This book isn't on your currently reading shelf on StoryGraph. Add it there to sync the edition.".to_owned())
  })?;
  emit_json!("{}", json!({ "book_id": found_id }));
  Ok(())
}
