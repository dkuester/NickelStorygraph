//! Batch sync for books whose progress moved forward without the plugin seeing
//! it: books read in another reader (Kobalt) or while offline. Nickel's database
//! holds the progress either way, so it is read from there, not from an open
//! ReadingView.

use std::collections::HashMap;
use std::io::Read;

use anyhow::{Context, Result};
use argh::FromArgs;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{Value, json};

use crate::commands::update::{Resolution, resolve_book};
use crate::config::CONFIG;
use crate::emit_json;
use crate::isbn::try_get_isbn;
use crate::log;
use crate::storygraph;
use crate::utils::{UserError, VERSION};

/// Nickel's ReadStatus for a book that is being read.
const READ_STATUS_READING: i64 = 1;

/// Sync the progress of every book that moved forward since its last sync.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "sync-all")]
pub struct SyncAll {
  /// also sync store books; off where the store syncs StoryGraph natively (Kobo)
  #[argh(switch)]
  pub include_store: bool,

  /// also link books that aren't linked yet (otherwise only linked books sync)
  #[argh(switch)]
  pub link_new: bool,

  /// only report what would be synced
  #[argh(switch)]
  pub dry_run: bool,
}

/// What the hook knows about a book, keyed like its settings.ini group.
#[derive(Debug, Default, Clone, PartialEq)]
struct Known {
  linked_id: Option<String>,
  last_progress: u32,
}

/// A book row from Nickel's database.
#[derive(Debug, Clone, PartialEq)]
struct Row {
  content_id: String,
  title: String,
  progress: u32,
  read_status: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Candidate {
  content_id: String,
  title: String,
  progress: u32,
  last_progress: u32,
  linked_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct Outcome {
  content_id: String,
  title: String,
  progress: u32,
  /// would_sync / would_link (dry run), synced / linked, already_ahead,
  /// not_found, error
  outcome: &'static str,
  #[serde(skip_serializing_if = "Option::is_none")]
  book_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  status_id: Option<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  message: Option<String>,
}

/// The hook stores per-book settings under the content ID with path separators
/// replaced (Settings::getPath); mirror that to match database rows to them.
fn settings_key(content_id: &str) -> String {
  content_id.replace(['/', '\\'], "-")
}

/// Parses the hook's list of known books from stdin:
/// `[{"key": "<settings group>", "linked_id": "…", "last_progress": 12}, …]`.
fn parse_known(input: &str) -> Result<HashMap<String, Known>> {
  if input.trim().is_empty() {
    return Ok(HashMap::new());
  }
  let entries: Vec<Value> = serde_json::from_str(input).context("Invalid book list on stdin")?;
  Ok(
    entries
      .iter()
      .filter_map(|e| {
        let key = e.get("key")?.as_str()?.to_owned();
        let linked_id = e
          .get("linked_id")
          .and_then(Value::as_str)
          .filter(|s| !s.is_empty())
          .map(str::to_owned);
        let last_progress = e.get("last_progress").and_then(Value::as_u64).unwrap_or(0) as u32;
        Some((
          key,
          Known {
            linked_id,
            last_progress,
          },
        ))
      })
      .collect(),
  )
}

/// Picks the books to sync: progress moved forward since the last sync, and
/// either already linked, or — only with `link_new` — unlinked but currently
/// being read (an unlinked finished book would otherwise land on StoryGraph as
/// "read" years later). Store books only with `include_store`.
fn select_candidates(
  rows: &[Row],
  known: &HashMap<String, Known>,
  include_store: bool,
  link_new: bool,
) -> Vec<Candidate> {
  rows
    .iter()
    .filter(|r| include_store || r.content_id.starts_with("file://"))
    .filter_map(|r| {
      let k = known
        .get(&settings_key(&r.content_id))
        .cloned()
        .unwrap_or_default();
      if r.progress == 0 || r.progress <= k.last_progress {
        return None;
      }
      if k.linked_id.is_none() && (!link_new || r.read_status != READ_STATUS_READING) {
        return None;
      }
      Some(Candidate {
        content_id: r.content_id.clone(),
        title: r.title.clone(),
        progress: r.progress,
        last_progress: k.last_progress,
        linked_id: k.linked_id,
      })
    })
    .collect()
}

fn read_rows() -> Result<Vec<Row>> {
  let conn = Connection::open_with_flags(&CONFIG.sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
    .context(format!(
      "Failed to connect to the database <i>{}</i>",
      CONFIG.sqlite_path
    ))?;
  let mut stmt = conn
    .prepare(
      "SELECT ContentID, Title, ___PercentRead, ReadStatus
      FROM content
      WHERE ContentType = 6 AND ___PercentRead > 0
      ORDER BY DateLastRead DESC;",
    )
    .context("Failed to prepare query")?;
  let rows = stmt
    .query_map([], |row| {
      Ok(Row {
        content_id: row.get(0)?,
        title: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        progress: row.get::<_, Option<i64>>(2)?.unwrap_or(0).clamp(0, 100) as u32,
        read_status: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
      })
    })
    .context("Failed to run query")?
    .collect::<rusqlite::Result<Vec<_>>>()
    .context("Failed to read book rows")?;
  Ok(rows)
}

/// Syncs one candidate. Expected account-wide errors (expired login,
/// Cloudflare) are returned so the whole run stops; anything else becomes an
/// `error` outcome for this book only.
fn sync_one(c: &Candidate) -> Result<Outcome> {
  let mut outcome = Outcome {
    content_id: c.content_id.clone(),
    title: c.title.clone(),
    progress: c.progress,
    outcome: "synced",
    book_id: None,
    status_id: None,
    message: None,
  };

  let result = (|| -> Result<()> {
    let book_id = match &c.linked_id {
      Some(id) => id.clone(),
      None => {
        let isbns = try_get_isbn(&c.content_id).unwrap_or_default();
        match resolve_book(Some(&c.content_id), &isbns)? {
          Resolution::Found(id) => {
            outcome.outcome = "linked";
            id
          }
          Resolution::OnShelf { book_id, progress } => {
            outcome.book_id = Some(book_id.clone());
            if c.progress <= progress {
              // StoryGraph is already at or past the device: link, don't push.
              outcome.outcome = "already_ahead";
              return Ok(());
            }
            outcome.outcome = "linked";
            book_id
          }
          Resolution::NotFound => {
            outcome.outcome = "not_found";
            return Ok(());
          }
        }
      }
    };
    outcome.book_id = Some(book_id.clone());
    let user_book = storygraph::update_progress(&book_id, "percentage", c.progress)?;
    outcome.status_id = user_book.status_id;
    Ok(())
  })();

  if let Err(err) = result {
    if err.downcast_ref::<UserError>().is_some() {
      return Err(err);
    }
    log!("sync-all: {} failed: {:#}", c.content_id, err);
    outcome.outcome = "error";
    outcome.message = Some(format!("{err:#}"));
  }
  Ok(outcome)
}

pub fn run(args: SyncAll) -> Result<()> {
  log!("{} {:?}", &*VERSION, args);

  let mut input = String::new();
  std::io::stdin()
    .read_to_string(&mut input)
    .context("Failed to read the book list from stdin")?;
  let known = parse_known(&input)?;
  let candidates = select_candidates(&read_rows()?, &known, args.include_store, args.link_new);
  log!("sync-all: {} candidate(s)", candidates.len());

  let books: Vec<Outcome> = if args.dry_run {
    candidates
      .iter()
      .map(|c| Outcome {
        content_id: c.content_id.clone(),
        title: c.title.clone(),
        progress: c.progress,
        outcome: if c.linked_id.is_some() {
          "would_sync"
        } else {
          "would_link"
        },
        book_id: c.linked_id.clone(),
        status_id: None,
        message: None,
      })
      .collect()
  } else {
    candidates.iter().map(sync_one).collect::<Result<_>>()?
  };

  emit_json!("{}", json!({ "dry_run": args.dry_run, "books": books }));
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn row(id: &str, progress: u32, read_status: i64) -> Row {
    Row {
      content_id: id.to_owned(),
      title: id.to_owned(),
      progress,
      read_status,
    }
  }

  fn known(entries: &[(&str, Option<&str>, u32)]) -> HashMap<String, Known> {
    entries
      .iter()
      .map(|(id, linked, last)| {
        (
          settings_key(id),
          Known {
            linked_id: linked.map(str::to_owned),
            last_progress: *last,
          },
        )
      })
      .collect()
  }

  #[test]
  fn settings_key_matches_hook_get_path() {
    assert_eq!(
      settings_key("file:///mnt/onboard/Books/Heat 2.kepub.epub"),
      "file:---mnt-onboard-Books-Heat 2.kepub.epub"
    );
    assert_eq!(settings_key("8d5bd68a-678e-4855"), "8d5bd68a-678e-4855");
  }

  #[test]
  fn parse_known_reads_hook_list() {
    let k = parse_known(
      r#"[{"key":"a","linked_id":"sg-1","last_progress":12},{"key":"b","linked_id":"","last_progress":0}]"#,
    )
    .unwrap();
    assert_eq!(k["a"].linked_id.as_deref(), Some("sg-1"));
    assert_eq!(k["a"].last_progress, 12);
    assert_eq!(k["b"].linked_id, None);
    assert!(parse_known("  ").unwrap().is_empty());
  }

  #[test]
  fn select_skips_store_books_unless_included() {
    let rows = vec![
      row("file:///mnt/onboard/a.epub", 10, 1),
      row("uuid-store", 10, 1),
    ];
    let ids = |c: Vec<Candidate>| c.into_iter().map(|c| c.content_id).collect::<Vec<_>>();
    assert_eq!(
      ids(select_candidates(&rows, &HashMap::new(), false, true)),
      vec!["file:///mnt/onboard/a.epub"]
    );
    assert_eq!(
      ids(select_candidates(&rows, &HashMap::new(), true, true)).len(),
      2
    );
  }

  #[test]
  fn select_only_forward_progress() {
    let rows = vec![
      row("file:///a", 40, 1),
      row("file:///b", 40, 1),
      row("file:///c", 30, 1),
    ];
    let k = known(&[
      ("file:///a", Some("sg-a"), 40), // unchanged
      ("file:///b", Some("sg-b"), 20), // forward
      ("file:///c", Some("sg-c"), 50), // backward
    ]);
    let c = select_candidates(&rows, &k, false, false);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].content_id, "file:///b");
    assert_eq!(c[0].last_progress, 20);
    assert_eq!(c[0].linked_id.as_deref(), Some("sg-b"));
  }

  #[test]
  fn select_unlinked_only_when_reading() {
    let rows = vec![
      row("file:///reading", 10, 1),
      row("file:///finished", 100, 2),
      row("file:///linked-finished", 100, 2),
    ];
    let k = known(&[("file:///linked-finished", Some("sg"), 80)]);
    let ids: Vec<_> = select_candidates(&rows, &k, false, true)
      .into_iter()
      .map(|c| c.content_id)
      .collect();
    assert_eq!(ids, vec!["file:///reading", "file:///linked-finished"]);
  }

  #[test]
  fn select_without_link_new_keeps_only_linked() {
    let rows = vec![row("file:///unlinked", 10, 1), row("file:///linked", 30, 1)];
    let k = known(&[("file:///linked", Some("sg"), 20)]);
    let ids: Vec<_> = select_candidates(&rows, &k, false, false)
      .into_iter()
      .map(|c| c.content_id)
      .collect();
    assert_eq!(ids, vec!["file:///linked"]);
  }
}
