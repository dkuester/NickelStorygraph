use anyhow::Result;
use argh::FromArgs;
use serde_json::json;

use crate::emit_json;
use crate::log;
use crate::storygraph;
use crate::utils::VERSION;

/// Search for books on StoryGraph.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "search")]
pub struct Search {
  /// search query (title, author, or ISBN)
  #[argh(option)]
  pub query: String,

  /// max results to return (default 20)
  #[argh(option, default = "20")]
  pub limit: usize,

  /// screen page (1-based); the hook computes rows-per-screen via `--limit`
  #[argh(option, default = "1")]
  pub page: usize,
}

pub fn run(args: Search) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);

  let result = storygraph::search_paged(&args.query, args.limit, args.page)?;
  emit_json!(
    "{}",
    json!({ "results": result.books, "has_more": result.has_more })
  );
  Ok(())
}
