use anyhow::Result;
use argh::FromArgs;
use rusqlite::{Connection, OpenFlags};
use serde_json::json;

use crate::config::CONFIG;
use crate::emit_json;
use crate::log;
use crate::storygraph::{self, Book, title_match_score};
use crate::utils::{VERSION, book_not_found, normalize_identifiers, strip_bracketed};

/// Update reading progress for a book.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "update")]
pub struct Update {
  /// storygraph book slug
  #[argh(option)]
  pub linked_id: Option<String>,

  /// kobo content ID
  #[argh(option)]
  pub content_id: Option<String>,

  /// progress percentage (0–100)
  #[argh(option)]
  pub value: u32,
}

pub fn run(args: Update) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);

  let content_id = args.content_id.clone();
  let (book_id, isbns) = normalize_identifiers(args.linked_id, content_id.as_deref())?;

  let book_id = if !book_id.is_empty() {
    book_id
  } else {
    match resolve_book(content_id.as_deref(), &isbns)? {
      Resolution::Found(book_id) => book_id,
      // Already on the currently-reading shelf: link to that edition and let the
      // hook ask whether to keep StoryGraph's progress or push Kobo's.
      Resolution::OnShelf { book_id, progress } => {
        emit_json!(
          "{}",
          serde_json::json!({
            "resolved_book_id": book_id,
            "needs_shelf_confirmation": true,
            "progress_percent": progress,
          })
        );
        return Ok(());
      }
      Resolution::NotFound => {
        book_not_found("Couldn't find this book on StoryGraph. Please link it manually.")
      }
    }
  };

  let user_book = storygraph::update_progress(&book_id, "percentage", args.value)?;
  let mut resp = serde_json::to_value(&user_book)?;
  resp["resolved_book_id"] = json!(book_id);
  emit_json!("{}", resp);
  Ok(())
}

/// How a Kobo book without a known StoryGraph id was matched.
pub enum Resolution {
  /// Found by ISBN or title search; the digital edition is preferred.
  Found(String),
  /// Already on the user's currently-reading shelf, with StoryGraph's progress.
  OnShelf {
    book_id: String,
    progress: u32,
  },
  NotFound,
}

/// Matches a Kobo book to a StoryGraph edition: the currently-reading shelf
/// first, then each ISBN, then a title/author search. Real errors (network,
/// expired session) propagate; only a genuine "no match" is `NotFound`.
pub fn resolve_book(content_id: Option<&str>, isbns: &[String]) -> Result<Resolution> {
  let meta = book_title_query(content_id);

  if let Some((ref title, _)) = meta
    && let Ok(Some((book_id, progress))) = storygraph::find_on_shelf(title)
  {
    return Ok(Resolution::OnShelf { book_id, progress });
  }

  // Try each ISBN first. A genuine "no result" (Ok(None)) falls through to the
  // title search, but a real error (network, expired session) must propagate —
  // swallowing it here as "no match" could silently link the wrong book.
  let mut found = None;
  for isbn in isbns {
    match storygraph::find_by_isbn(isbn)? {
      Some(result) => {
        // Reject ISBN results that don't match the Kobo title — the EPUB's ISBN metadata
        // sometimes points to a different book (e.g. a sampler or wrong edition).
        if let Some((ref title, _)) = meta
          && title_match_score(title, &result.title) < 0.3
        {
          continue;
        }
        found = Some(result);
        break;
      }
      None => continue,
    }
  }

  if found.is_none()
    && let Some((title, author)) = meta.as_ref()
  {
    // Same rule: a failed search is a real error, not "no results".
    // Kobo titles often carry the series in parentheses, which makes
    // StoryGraph's search return nothing.
    let title = strip_bracketed(title);
    let results = storygraph::search(&format!("{title} {author}"))?;
    found = best_title_match(results, &title);
  }

  match found {
    Some(book) => Ok(Resolution::Found(storygraph::prefer_digital_edition(
      &book.book_id,
    )?)),
    None => Ok(Resolution::NotFound),
  }
}

/// Picks the best match from `results` for `title`, requiring a near-exact match (≥ 90%
/// of meaningful query words present). This runs with no confirmation step, so it must be
/// strict enough to reject other volumes/editions of the same series (which can otherwise
/// share every word but a volume number) rather than silently linking the wrong book.
/// Returns None if nothing scores well enough.
fn best_title_match(results: Vec<Book>, title: &str) -> Option<Book> {
  results
    .into_iter()
    .filter_map(|book| {
      let score = title_match_score(title, &book.title);
      if score >= 0.9 {
        Some((book, score))
      } else {
        None
      }
    })
    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    .map(|(book, _)| book)
}

fn book_title_query(content_id: Option<&str>) -> Option<(String, String)> {
  let content_id = content_id?;
  let conn =
    Connection::open_with_flags(&CONFIG.sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
  conn
    .query_row(
      "SELECT Title, Attribution FROM content WHERE ContentID = ? LIMIT 1",
      [content_id],
      |row| {
        Ok((
          row.get::<_, String>(0)?,
          row.get::<_, String>(1).unwrap_or_default(),
        ))
      },
    )
    .ok()
}
