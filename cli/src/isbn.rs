use std::io::prelude::*;
use std::sync::LazyLock;
use std::{fs::File, path::Path};

use anyhow::{Context, Result, bail};
use itertools::Itertools;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use zip::ZipArchive;

use crate::config::CONFIG;
use crate::log;
use crate::utils::book_not_found;

fn isbn_13_check_digit(digits: &[u32]) -> u32 {
  let mut sum = 0;
  for i in 0..6 {
    sum += digits[i * 2] + 3 * digits[i * 2 + 1];
  }
  let sum_m = sum % 10;
  if sum_m == 0 { 0 } else { 10 - sum_m }
}

fn isbn_10_check_digit(digits: &[u32]) -> u32 {
  let mut sum = 0;
  for (i, d) in digits.iter().take(9).enumerate() {
    sum += d * (10 - i as u32);
  }

  let sum_m = sum % 11;
  if sum_m == 0 { 0 } else { 11 - sum_m }
}

fn digits_to_string(digits: &[u32]) -> String {
  digits
    .iter()
    .map(|d| {
      if *d == 10 {
        'X'
      } else {
        char::from_digit(*d, 10).unwrap()
      }
    })
    .collect()
}

fn string_to_digits(str: &str) -> Vec<u32> {
  str
    .chars()
    .filter_map(|c| {
      if c == 'X' || c == 'x' {
        Some(10)
      } else {
        c.to_digit(10)
      }
    })
    .collect()
}

fn normalize_isbn(isbn: &str) -> Option<Vec<String>> {
  let mut isbn = isbn.to_ascii_uppercase();
  isbn.retain(char::is_alphanumeric);

  if isbn.len() == 10 && isbn.starts_with("B") {
    return Some(vec![isbn]);
  }

  let digits = string_to_digits(&isbn);

  if digits.len() == 13
    && (digits[..3] == [9, 7, 8] || digits[..3] == [9, 7, 9])
    && isbn_13_check_digit(&digits) == digits[12]
  {
    if digits[..3] == [9, 7, 8] {
      let mut isbn_10 = [0; 10];
      isbn_10[..9].clone_from_slice(&digits[3..12]);
      isbn_10[9] = isbn_10_check_digit(&isbn_10);

      Some(vec![isbn, digits_to_string(&isbn_10)])
    } else {
      Some(vec![isbn])
    }
  } else if digits.len() == 10 && isbn_10_check_digit(&digits) == digits[9] {
    let mut isbn_13 = [0; 13];
    isbn_13[0] = 9;
    isbn_13[1] = 7;
    isbn_13[2] = 8;
    isbn_13[3..12].clone_from_slice(&digits[..9]);
    isbn_13[12] = isbn_13_check_digit(&isbn_13);

    Some(vec![isbn, digits_to_string(&isbn_13)])
  } else {
    None
  }
}

fn get_opf_path(manifest: &str) -> Result<String> {
  let mut reader = Reader::from_str(manifest);
  let mut xml_version = XmlVersion::Implicit1_0;

  loop {
    match reader.read_event() {
      Err(e) => {
        Err(e).context(format!(
          "Error reading container manifest at position {}",
          reader.error_position()
        ))?;
      }
      Ok(Event::Eof) => break,
      Ok(Event::Decl(e)) => {
        if let Ok(version) = e.xml_version() {
          xml_version = version;
        }
      }
      Ok(Event::Empty(e)) if e.name().as_ref() == b"rootfile" => {
        let media_type = e
          .try_get_attribute("media-type")
          .context("Failed to decode <i>media-type</i> attribute")?;
        if let Some(media_type) = media_type
          && media_type
            .normalized_value(xml_version)
            .context("Failed to decode <i>media-type</i> attribute value")?
            == "application/oebps-package+xml"
        {
          return Ok(
            e.try_get_attribute("full-path")
              .context("Failed to decode <i>full-path</i> attribute")?
              .context("Failed to find <i>full-path</i> attribute")?
              .normalized_value(xml_version)
              .context("Failed to decode <i>full-path</i> attribute value")?
              .to_string(),
          );
        }
      }
      _ => (),
    }
  }

  bail!("Failed to find OEBPS root file path")
}

fn read_opf(opf: &str) -> Result<(Vec<String>, Vec<String>)> {
  let mut reader = Reader::from_str(opf);

  let mut xml_version = XmlVersion::Implicit1_0;
  let mut isbns = Vec::<String>::new();
  let mut items = Vec::<String>::new();

  #[derive(Debug)]
  enum State {
    Start,
    Metadata,
    Identifier(String),
    Manifest,
  }
  let mut state = State::Start;

  loop {
    let event = reader.read_event().context(format!(
      "Error reading OEBPS file at position {}",
      reader.error_position()
    ))?;
    state = match (state, event) {
      (State::Start, Event::Decl(e)) => {
        if let Ok(version) = e.xml_version() {
          xml_version = version;
        }

        State::Start
      }
      (State::Start, Event::Start(e)) if e.local_name().as_ref() == b"metadata" => State::Metadata,
      (State::Metadata, Event::Start(e)) if e.local_name().as_ref() == b"identifier" => {
        State::Identifier(String::new())
      }
      (State::Identifier(s), Event::End(e)) if e.local_name().as_ref() == b"identifier" => {
        let i = s.rfind(':').unwrap_or(0);
        if let Some(normalized) = normalize_isbn(&s[i..]) {
          isbns.extend(normalized);
        }

        State::Metadata
      }
      (State::Metadata, Event::End(e)) if e.local_name().as_ref() == b"metadata" => State::Start,
      (State::Identifier(s), Event::Text(e)) => State::Identifier(
        s + e
          .decode()
          .context("Failed to decode identifier text")?
          .as_ref(),
      ),
      (State::Start, Event::Start(e)) if e.local_name().as_ref() == b"manifest" => State::Manifest,
      (State::Manifest, Event::Empty(e)) if e.local_name().as_ref() == b"item" => {
        if let Some(media_type) = e
          .try_get_attribute("media-type")
          .context("Failed to decode <i>media-type</i> attribute")?
        {
          let media_type = media_type
            .normalized_value(xml_version)
            .context("Failed to decode <i>href</i> attribute value")?
            .to_string();

          if (media_type == "application/xhtml+xml" || media_type == "application/x-dtbook+xml")
            && let Some(href) = e
              .try_get_attribute("href")
              .context("Failed to decode <i>href</i> attribute")?
          {
            items.push(
              href
                .normalized_value(xml_version)
                .context("Failed to decode <i>href</i> attribute value")?
                .to_string(),
            );
          }
        }

        State::Manifest
      }
      (State::Manifest, Event::End(e)) if e.local_name().as_ref() == b"manifest" => State::Start,
      (State::Start, Event::Eof) => break,
      (state, Event::Eof) => bail!("Unexpected end of file in {:?}", state),
      (state, _) => state,
    };
  }

  Ok((isbns, items))
}

fn read_item(item: &str) -> Result<Vec<String>> {
  #[allow(clippy::invisible_characters)] // intentional soft hyphens/nbsp in extracted ISBN text
  static RE: LazyLock<Result<Regex, regex::Error>> = LazyLock::new(|| {
    Regex::new(
      r"(?:-10 |-13 )?((?:[‐-―\-\.\^ \t ­−]?[0-9]){13}|(?:[‐-―\-\.\^ \t ­−]?[0-9]){9}[‐-―\-\.\^ \t ­−]]?[0-9xX])",
    )
  });

  let mut reader = Reader::from_str(item);

  let mut text = String::new();

  enum State {
    Start,
    Body,
    Style,
  }
  let mut state = State::Start;

  loop {
    let event = reader.read_event().context(format!(
      "Error reading OEBPS file at position {}",
      reader.error_position()
    ))?;
    state = match (state, event) {
      (State::Start, Event::Start(e)) if e.local_name().as_ref() == b"body" => State::Body,
      (State::Body, Event::End(e)) if e.local_name().as_ref() == b"body" => break,
      (State::Body, Event::Start(e)) if e.local_name().as_ref() == b"style" => State::Style,
      (State::Style, Event::End(e)) if e.local_name().as_ref() == b"style" => State::Body,
      (State::Body, Event::Text(e)) => {
        text += e.decode().context("Failed to decode text")?.as_ref();
        State::Body
      }
      (_, Event::Eof) => break,
      (state, _) => state,
    };
  }

  Ok(
    RE.as_ref()?
      .captures_iter(&text)
      .filter_map(|c| c.get(0))
      .filter_map(|c| normalize_isbn(c.as_str()))
      .flatten()
      .collect(),
  )
}

fn read_epub_isbn(content_id: &str) -> Result<Vec<String>> {
  let path = content_id.strip_prefix("file://").unwrap_or(content_id);
  let file = File::open(Path::new(path)).context("Failed to open file")?;
  let mut archive = ZipArchive::new(file).context("Failed to parse file as archive")?;

  let mut buf = String::new();
  archive
    .by_name("META-INF/container.xml")
    .context("Failed to open container manifest")?
    .read_to_string(&mut buf)
    .context("Failed to read container manifest")?;
  let opf_path = get_opf_path(&buf)?;

  buf.clear();
  archive
    .by_name(&opf_path)
    .context(format!("Failed to open OEBPS root file <i>{opf_path}</i>"))?
    .read_to_string(&mut buf)
    .context("Failed to read OEBPS root file")?;

  let (mut isbn, items) = read_opf(&buf)?;

  if isbn.is_empty() {
    let opf_dir = Path::new(&opf_path).parent().unwrap_or(Path::new("/"));

    for n in 0..items.len() {
      let i = if n < 3 {
        n
      } else if n < 6 {
        items.len() - 1 - (n - 3)
      } else {
        n - 3
      };

      buf.clear();
      archive
        .by_path(opf_dir.join(&items[i]))
        .context(format!(
          "Failed to open OEBPS root file <i>{}</i>",
          items[i]
        ))?
        .read_to_string(&mut buf)
        .context("Failed to read OEBPS root file")?;
      isbn = read_item(&buf)?;

      if !isbn.is_empty() {
        break;
      }
    }
  }

  isbn.sort();
  isbn.dedup();

  log!("ISBN from EPUB `{}`", isbn.join(", "));

  Ok(isbn)
}

/// 13-digit runs starting with 978/979, not part of a longer number.
static ISBN_13_IN_TEXT: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?:^|\D)(97[89]\d{10})(?:\D|$)").expect("valid regex"));

/// Finds an ISBN-13 inside a shop cover URL. The Tolino shop leaves Nickel's
/// `ISBN` column empty, but its cover URLs carry the ISBN, e.g.
/// `https://cdp.pageplace.de/cdp/public/publications/DT0400/9781068563119_A62207078/cover`.
/// Only candidates with a valid check digit are accepted.
fn isbn_from_cover_url(url: &str) -> Option<Vec<String>> {
  ISBN_13_IN_TEXT
    .captures_iter(url)
    .find_map(|c| normalize_isbn(&c[1]))
}

/// Reads a column that Nickel stores as TEXT or BLOB depending on the source.
fn text_or_blob(value: rusqlite::types::ValueRef<'_>) -> Option<String> {
  match value {
    rusqlite::types::ValueRef::Text(t) | rusqlite::types::ValueRef::Blob(t) => {
      Some(String::from_utf8_lossy(t).into_owned())
    }
    _ => None,
  }
}

fn read_sqlite_isbn(content_id: &str) -> Result<Vec<String>> {
  let (isbn, image_url) =
    Connection::open_with_flags(&CONFIG.sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
      .context(format!(
        "Failed to connect to the database <i>{}</i>",
        CONFIG.sqlite_path
      ))?
      .prepare(
        "SELECT ISBN, ImageUrl
      FROM content
      WHERE BookTitle is null
      AND ContentId is (?1)
      LIMIT 1;",
      )
      .context("Failed to prepare query")?
      .query_map([&content_id], |row| {
        Ok((
          row.get::<_, Option<String>>(0)?,
          text_or_blob(row.get_ref(1)?),
        ))
      })
      .context("Failed to run query")?
      .next()
      .context("Query returned no results")?
      .context("Failed to map query result")?;

  if let Some(isbn) = isbn.as_deref().and_then(normalize_isbn) {
    log!("ISBN from database `{}`", isbn.join(", "));
    return Ok(isbn);
  }

  if let Some(isbn) = image_url.as_deref().and_then(isbn_from_cover_url) {
    log!("ISBN from cover URL `{}`", isbn.join(", "));
    return Ok(isbn);
  }

  // No ISBN anywhere: return an empty list like the EPUB path does, so callers
  // can fall back to a title search instead of giving up here.
  log!("no ISBN in the database for `{}`", content_id);
  Ok(Vec::new())
}

/// Reads a book's ISBNs (EPUB metadata for sideloaded books, the database
/// otherwise). Errors are returned, not turned into a book-not-found exit.
pub fn try_get_isbn(content_id: &str) -> Result<Vec<String>> {
  if content_id.starts_with("file://") {
    read_epub_isbn(content_id)
  } else {
    read_sqlite_isbn(content_id)
  }
}

pub fn get_isbn(content_id: &str) -> Vec<String> {
  match try_get_isbn(content_id) {
    Ok(isbn) => isbn,
    Err(e) => book_not_found(&format!(
      "Encountered an unexpected error while finding ISBN. Please link book manually.<br><br>{:#}",
      e.chain().join("<br>> ")
    )),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn isbn_from_cover_url_reads_tolino_shop_urls() {
    let url = "https://cdp.pageplace.de/cdp/public/publications/DT0400/9781068563119_A62207078/cover?size=large";
    assert_eq!(
      isbn_from_cover_url(url),
      Some(vec!["9781068563119".to_owned(), "1068563117".to_owned()])
    );
  }

  #[test]
  fn isbn_from_cover_url_rejects_bad_check_digit_and_longer_numbers() {
    // Last digit changed: invalid check digit.
    assert_eq!(
      isbn_from_cover_url("https://x/DT0400/9781068563110_A1/cover"),
      None
    );
    // A 978... run embedded in a longer number is not an ISBN.
    assert_eq!(
      isbn_from_cover_url("https://x/12397810685631190/cover"),
      None
    );
    assert_eq!(isbn_from_cover_url("https://x/no-isbn/cover"), None);
  }

  #[test]
  fn isbn13_check_digit_known_values() {
    assert_eq!(
      isbn_13_check_digit(&[9, 7, 8, 0, 3, 0, 6, 4, 0, 6, 1, 5]),
      7
    );
    assert_eq!(isbn_10_check_digit(&[0, 3, 0, 6, 4, 0, 6, 1, 5]), 2);
  }

  #[test]
  fn normalize_isbn13_returns_10_equivalent() {
    assert_eq!(
      normalize_isbn("9780306406157"),
      Some(vec!["9780306406157".to_string(), "0306406152".to_string()])
    );
    assert_eq!(
      normalize_isbn("9780131103627"),
      Some(vec!["9780131103627".to_string(), "0131103628".to_string()])
    );
  }

  #[test]
  fn normalize_isbn10_returns_13_equivalent() {
    assert_eq!(
      normalize_isbn("0-306-40615-2"),
      Some(vec!["0306406152".to_string(), "9780306406157".to_string()])
    );
  }

  #[test]
  fn normalize_isbn_rejects_invalid() {
    assert_eq!(normalize_isbn("9780306406158"), None); // bad check digit
    assert_eq!(normalize_isbn("1234567890"), None); // invalid ISBN-10 check
    assert_eq!(normalize_isbn("notanisbn"), None);
  }
}
