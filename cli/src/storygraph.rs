// HTTP client for the StoryGraph unofficial web API.
//
// StoryGraph has no public API. All interactions mirror what a browser does:
// session-cookie auth, HTML-scraped responses for reads, and HTML form POSTs
// for writes. A CSRF token is extracted from the page HTML before each POST.
//
// Auth cookies come from browser DevTools and live in config.ini:
//   session_cookie     → _storygraph_session
//   remember_user_token → remember_user_token

use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, Response};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::{Certificate, StatusCode, Url};
use retry::delay::{Exponential, jitter};
use retry::retry;
use scraper::{Html, Selector};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::config::CONFIG;
use crate::debug_log;
use crate::utils::UserError;

pub const BASE_URL: &str = "https://app.thestorygraph.com";

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";

// ── Output types (serialized to JSON for the hook) ───────────────────────────

fn serialize_author_as_array<S: serde::Serializer>(val: &str, s: S) -> Result<S::Ok, S::Error> {
  use serde::ser::SerializeSeq;
  let mut seq = s.serialize_seq(Some(1))?;
  seq.serialize_element(val)?;
  seq.end()
}

/// A series reference as rendered in the link dialog (`BookRow::getSeries`
/// reads `series.name` and `series.position`).
#[derive(Serialize, Debug, PartialEq)]
pub struct SeriesRef {
  pub name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub position: Option<u32>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Book {
  #[serde(rename = "id")]
  pub book_id: String,
  pub title: String,
  #[serde(rename = "authors", serialize_with = "serialize_author_as_array")]
  pub author: String,
  #[serde(rename = "image")]
  pub cover_url: Option<String>,
  pub pages: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub series: Option<SeriesRef>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub release_year: Option<u32>,
}

#[derive(Serialize, Debug)]
pub struct JournalActivity {
  pub event: String,
  pub action_at: String,
  #[serde(skip_serializing_if = "String::is_empty")]
  pub entry: String,
  pub metadata: Value,
}

#[derive(Serialize, Debug)]
pub struct UserBook {
  pub book_id: String,
  pub status_id: Option<u8>,
  pub progress_percent: u32,
  pub progress_pages: u32,
  pub book_pages: u32,
  pub progress_type: String,
  pub edition_format: String,
}

#[derive(Serialize, Debug)]
pub struct Edition {
  pub book_id: String,
  pub title: String,
  #[serde(rename = "edition_format")]
  pub format: String,
  pub pages: Option<u32>,
  pub isbn: String,
  pub language: String,
  pub publisher: String,
  #[serde(rename = "release_date")]
  pub pub_date: String,
  #[serde(rename = "image")]
  pub cover_url: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct User {
  pub username: String,
  pub slug: String,
}

// Page-level state extracted from /books/:slug before any mutating POST.
struct PageData {
  csrf: String,
  book_pages: String,
  last_pages: String,
  last_percent: String,
}

// ── Singleton client ──────────────────────────────────────────────────────────

fn build_client(jar: Arc<Jar>) -> Result<Client, reqwest::Error> {
  Client::builder()
    .cookie_provider(jar)
    .user_agent(USER_AGENT)
    .tls_certs_only(
      webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|cert| Certificate::from_der(cert))
        .collect::<Result<Vec<_>, _>>()?,
    )
    .build()
}

static CLIENT: LazyLock<Result<Client, reqwest::Error>> = LazyLock::new(|| {
  let base: Url = BASE_URL.parse().expect("BASE_URL is valid");
  let jar = Arc::new(Jar::default());

  // Pre-seed auth cookies from config.
  jar.add_cookie_str(
    &format!("_storygraph_session={}", CONFIG.session_cookie),
    &base,
  );
  jar.add_cookie_str(
    &format!("remember_user_token={}", CONFIG.remember_user_token),
    &base,
  );
  jar.add_cookie_str("cookies_popup_seen=yes", &base);
  jar.add_cookie_str("plus_popup_seen=yes", &base);

  build_client(jar)
});

fn client() -> Result<&'static Client> {
  CLIENT.as_ref().context("Failed to construct HTTP client")
}

// ── Guards ────────────────────────────────────────────────────────────────────

fn require_credentials() -> Result<()> {
  if CONFIG.session_cookie.is_empty() {
    return Err(UserError(
      "Please set <i>session_cookie</i> in <i>.adds/NickelStorygraph/config.ini</i>. Obtain it from browser DevTools → Application → Cookies → app.thestorygraph.com.".to_owned(),
    ).into());
  }
  Ok(())
}

/// How to get fresh cookies onto the device; shared by the expired-login and
/// Cloudflare messages.
const REFRESH_COOKIES_HINT: &str = "Log in to StoryGraph in a browser, put fresh <i>session_cookie</i> and <i>remember_user_token</i> lines into <i>.adds/NickelStorygraph/cookies.ini</i>, then use <i>Settings → Import cookies</i>.";

/// Returns Err if the final response URL ended up on the sign-in page.
fn check_session(response: &Response) -> Result<()> {
  if response.url().path().contains("/users/sign_in") {
    // A stale session_cookie alone is renewed through remember_user_token, so
    // landing on the sign-in page means the remember token itself is dead.
    return Err(
      UserError(format!(
        "Your StoryGraph login has expired.<br><br>{REFRESH_COOKIES_HINT}"
      ))
      .into(),
    );
  }
  Ok(())
}

/// Cloudflare serves its bot challenge as a 403 page whose body is HTML
/// ("Just a moment..."), not JSON. Without this check that body would surface
/// as a generic "returned error status" and the user would be told to report an
/// unexpected error instead of refreshing the cookies.
fn is_cloudflare_challenge(status: &StatusCode, body: &str) -> bool {
  *status == StatusCode::FORBIDDEN
    && (body.contains("Just a moment")
      || body.contains("cf_chl")
      || body.contains("challenge-platform"))
}

/// Turns a Cloudflare challenge into an actionable error; rethrows any other
/// 403 as the plain HTTP error.
///
/// NOTE: this consumes the response body, so call it only on the path where the
/// body is not needed afterwards.
fn check_blocked(res: Response, context: &str) -> Result<Response> {
  let status = res.status();

  if status != StatusCode::FORBIDDEN {
    return Ok(res);
  }

  let body = res.text().unwrap_or_default();

  if is_cloudflare_challenge(&status, &body) {
    return Err(UserError(
      format!(
        "StoryGraph blocked the request with a Cloudflare bot check (HTTP 403).<br><br>If syncing stopped working, your cookies have probably expired. {REFRESH_COOKIES_HINT} If they are fresh, wait a few minutes and try again."
      ),
    )
    .into());
  }

  bail!("{context} returned error status: 403 Forbidden")
}

// ── Low-level HTTP ────────────────────────────────────────────────────────────

fn get(path: &str) -> Result<String> {
  let url = format!("{BASE_URL}{path}");
  debug_log!("GET {url}");

  let res = retry(Exponential::from_millis(500).map(jitter).take(3), || {
    client()?
      .get(&url)
      .send()
      .context(format!("GET {url} failed"))
  })
  .map_err(|e| e.error)?;

  let res = check_blocked(res, &format!("GET {url}"))?;
  check_session(&res)?;
  res
    .error_for_status_ref()
    .context(format!("GET {url} returned error status"))?;
  res
    .text()
    .context(format!("Failed to read body from GET {url}"))
}

/// Uses regular browser headers instead of XHR.
/// Needed for endpoints (like /reviews) that behave differently for AJAX.
fn post_form_browser(path: &str, fields: &[(String, String)]) -> Result<String> {
  let url = format!("{BASE_URL}{path}");
  debug_log!("POST {url} (browser)");

  let res = retry(Exponential::from_millis(500).map(jitter).take(3), || {
    client()?
      .post(&url)
      .header(
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      )
      .header("Origin", BASE_URL)
      .header("Referer", &url)
      .form(fields)
      .send()
      .context(format!("POST {url} failed"))
  })
  .map_err(|e| e.error)?;

  if res.status() == StatusCode::FOUND {
    let location = res
      .headers()
      .get("location")
      .and_then(|v| v.to_str().ok())
      .unwrap_or("");
    if location.contains("/users/sign_in") {
      bail!(
        "StoryGraph session has expired. Please update <i>session_cookie</i> and <i>remember_user_token</i> in <i>.adds/NickelStorygraph/config.ini</i>."
      );
    }
    return Ok(String::new());
  }

  let res = check_blocked(res, &format!("POST {url}"))?;
  check_session(&res)?;
  res
    .error_for_status_ref()
    .context(format!("POST {url} returned error status"))?;
  res
    .text()
    .context(format!("Failed to read body from POST {url}"))
}

fn post_form(path: &str, fields: &[(&str, &str)]) -> Result<String> {
  let url = format!("{BASE_URL}{path}");
  debug_log!("POST {url}");

  let csrf = fields
    .iter()
    .find(|(k, _)| *k == "authenticity_token")
    .map(|(_, v)| *v)
    .unwrap_or("");

  let res = retry(Exponential::from_millis(500).map(jitter).take(3), || {
    client()?
      .post(&url)
      .header("X-Requested-With", "XMLHttpRequest")
      .header(
        "Accept",
        "text/javascript, application/javascript, application/ecmascript, */*; q=0.01",
      )
      .header("Origin", BASE_URL)
      .header("Referer", &url)
      .header("X-CSRF-Token", csrf)
      .form(fields)
      .send()
      .context(format!("POST {url} failed"))
  })
  .map_err(|e| e.error)?;

  // 302 redirect back to the book page is a success for StoryGraph mutations.
  if res.status() == StatusCode::FOUND {
    let location = res
      .headers()
      .get("location")
      .and_then(|v| v.to_str().ok())
      .unwrap_or("");
    if location.contains("/users/sign_in") {
      bail!(
        "StoryGraph session has expired. Please update <i>session_cookie</i> and <i>remember_user_token</i> in <i>.adds/NickelStorygraph/config.ini</i>."
      );
    }
    return Ok(String::new());
  }

  let res = check_blocked(res, &format!("POST {url}"))?;
  check_session(&res)?;
  res
    .error_for_status_ref()
    .context(format!("POST {url} returned error status"))?;
  res
    .text()
    .context(format!("Failed to read body from POST {url}"))
}

// ── HTML helpers ──────────────────────────────────────────────────────────────

fn csrf_token(html: &str) -> Option<String> {
  let doc = Html::parse_document(html);

  let meta_sel = Selector::parse(r#"meta[name="csrf-token"]"#).expect("valid selector");
  if let Some(el) = doc.select(&meta_sel).next()
    && let Some(content) = el.value().attr("content")
  {
    return Some(content.to_owned());
  }

  let input_sel = Selector::parse(r#"input[name="authenticity_token"]"#).expect("valid selector");
  doc
    .select(&input_sel)
    .next()
    .and_then(|el| el.value().attr("value"))
    .map(str::to_owned)
}

fn text_of(doc: &Html, selector: &str) -> String {
  Selector::parse(selector)
    .ok()
    .and_then(|sel| doc.select(&sel).next())
    .map(|el| el.text().collect::<String>().trim().to_owned())
    .unwrap_or_default()
}

fn extract_value_near_name(html: &str, needle: &str) -> Option<String> {
  let i = html.find(needle)?;
  let after = &html[i..];
  let v_pos = after.find("value=\"")?;
  // Ensure value= is within the same tag (no > between)
  if after[..v_pos].contains('>') {
    return None;
  }
  let start = v_pos + 7;
  let end = after[start..].find('"')?;
  Some(after[start..start + end].to_owned())
}

fn hidden_value(html: &str, name: &str) -> String {
  let name_needle = format!("name=\"{name}\"");
  // Try name=...value=... order first, then value=...name=... order.
  extract_value_near_name(html, &name_needle)
    .or_else(|| {
      // Reverse: find value= that appears just before name=
      let mut pos = 0;
      while let Some(i) = html[pos..].find("value=\"") {
        let abs = pos + i;
        let tag_start = html[..abs].rfind('<').unwrap_or(0);
        let tag_slice = &html[tag_start..abs + 100.min(html.len() - abs)];
        if tag_slice.contains(&name_needle) {
          let start = abs + 7;
          if let Some(end) = html[start..].find('"') {
            return Some(html[start..start + end].to_owned());
          }
        }
        pos = abs + 1;
      }
      None
    })
    .unwrap_or_default()
}

/// Parses a `style="...width: N%"` attribute value into a rounded percentage.
fn percent_from_style(style: &str) -> Option<u32> {
  let i = style.find("width:")?;
  let after = style[i + 6..].trim_start();
  let end = after.find('%')?;
  after[..end]
    .trim()
    .parse::<f32>()
    .ok()
    .map(|f| f.round() as u32)
}

/// Extracts the progress percentage from the `edit-progress` progress bar's
/// inline `width` style. StoryGraph renders this bar for both page- and
/// time-tracked books, unlike the hidden percent input which only exists for
/// page-tracked books.
fn progress_percent_from_edit_progress(html: &str) -> Option<u32> {
  let i = html.find("edit-progress")?;
  percent_from_style(&html[i..])
}

/// StoryGraph's internal reading-status IDs (stable across the site).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ReadingStatus {
  ToRead = 1,
  CurrentlyReading = 2,
  Read = 3,
  Paused = 4,
  DidNotFinish = 5,
  Rereading = 6,
}

impl ReadingStatus {
  pub fn from_id(id: u8) -> Option<Self> {
    match id {
      1 => Some(Self::ToRead),
      2 => Some(Self::CurrentlyReading),
      3 => Some(Self::Read),
      4 => Some(Self::Paused),
      5 => Some(Self::DidNotFinish),
      6 => Some(Self::Rereading),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::ToRead => "to-read",
      Self::CurrentlyReading => "currently-reading",
      Self::Read => "read",
      Self::Paused => "paused",
      Self::DidNotFinish => "did-not-finish",
      Self::Rereading => "rereading",
    }
  }

  pub fn parse(text: &str) -> Option<Self> {
    let lower = text.to_lowercase();
    if lower.contains("rereading") {
      Some(Self::Rereading)
    } else if lower.contains("currently reading") {
      Some(Self::CurrentlyReading)
    } else if lower.contains("to-read") || lower.contains("to read") {
      Some(Self::ToRead)
    } else if lower.contains("paused") {
      Some(Self::Paused)
    } else if lower.contains("did not finish") {
      Some(Self::DidNotFinish)
    } else if lower.trim() == "read" || lower.ends_with(" read") || lower.starts_with("read ") {
      Some(Self::Read)
    } else {
      None
    }
  }
}

/// StoryGraph's reading-format filter values for the editions page.
pub const READING_FORMAT_PRINT: i32 = 1;
pub const READING_FORMAT_EBOOK: i32 = 4;

fn parse_status_text(text: &str) -> Option<u8> {
  ReadingStatus::parse(text).map(|s| s as u8)
}

fn status_to_str(status_id: u8) -> &'static str {
  ReadingStatus::from_id(status_id)
    .map(ReadingStatus::as_str)
    .unwrap_or("currently-reading")
}

/// Fetch /books/:slug and extract mutable form state needed for POST calls.
fn fetch_page_data(book_id: &str) -> Result<(String, PageData)> {
  let html = get(&format!("/books/{book_id}"))?;
  let csrf = csrf_token(&html).context("Could not find CSRF token on book page")?;
  let book_pages = hidden_value(&html, "read_status[book_num_of_pages]");
  let last_pages = hidden_value(&html, "read_status[last_reached_pages]");
  let last_percent = hidden_value(&html, "read_status[last_reached_percent]");

  Ok((
    html,
    PageData {
      csrf,
      book_pages,
      last_pages,
      last_percent,
    },
  ))
}

// ── Public API ────────────────────────────────────────────────────────────────

/// A single reading goal (books, pages, or hours) from the challenges dashboard.
#[derive(Serialize, Debug)]
pub struct GoalEntry {
  pub label: String,   // "Books", "Pages", "Hours"
  pub current: String, // raw string: "10", "2,458", "27.98"
  pub target: String,  // raw string: "26", "10,000", "50"
  pub unit: String,    // "books", "pages", "hours"
  pub percent: u32,    // 0–100
  pub status: String,  // status message text
}

/// Reading goals summary from the StoryGraph challenges dashboard.
#[derive(Serialize, Debug)]
pub struct ReadingGoals {
  pub year: String,
  pub books_read: u32,
  pub goals: Vec<GoalEntry>,
}

/// Fetches the user's reading goals from `/reading_challenges/dashboard/{slug}`.
pub fn get_goals() -> Result<ReadingGoals> {
  require_credentials()?;
  let user = get_user()?;
  let html = get(&format!("/reading_challenges/dashboard/{}", user.slug))?;
  let doc = Html::parse_document(&html);

  // Find a 4-digit "20xx" year in any heading; fall back to the current calendar year.
  let heading_sel = Selector::parse("h1, h2, h3").expect("valid");
  let year = doc
    .select(&heading_sel)
    .find_map(|el| {
      el.text()
        .collect::<String>()
        .split_whitespace()
        .find(|w| w.len() == 4 && w.starts_with("20") && w.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_owned)
    })
    .unwrap_or_else(|| {
      use chrono::Datelike;
      chrono::Local::now().year().to_string()
    });

  // "You have read 10 books this year." — count comes from the link text
  let books_link_sel = Selector::parse(r#"a[href*="/books-read/"]"#).expect("valid");
  let books_read: u32 = doc
    .select(&books_link_sel)
    .next()
    .and_then(|el| {
      el.text()
        .collect::<String>()
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
    })
    .unwrap_or(0);

  // h6 labels ("Books", "Pages", "Hours") — each appears twice (mobile + desktop); deduplicate
  let h6_sel = Selector::parse("h6").expect("valid");
  let mut labels: Vec<String> = Vec::new();
  let mut seen_labels = std::collections::HashSet::new();
  for el in doc.select(&h6_sel) {
    let text = el
      .text()
      .find(|s| !s.trim().is_empty())
      .map(|s| s.trim().to_owned())
      .unwrap_or_default();
    if !text.is_empty() && seen_labels.insert(text.clone()) {
      labels.push(text);
    }
  }

  // Mobile count paragraphs: "10/26 books", "2,458/10,000 pages", "27.98/50 hours"
  let count_sel = Selector::parse("p.flex-shrink-0").expect("valid");
  let counts: Vec<String> = doc
    .select(&count_sel)
    .map(|el| el.text().collect::<String>().trim().to_owned())
    .collect();

  // Progress fill div style="width:X%" — one per goal in DOM order
  let fill_sel = Selector::parse(r#"div[style*="width:"]"#).expect("valid");
  let percents: Vec<u32> = doc
    .select(&fill_sel)
    .filter_map(|el| el.value().attr("style").and_then(percent_from_style))
    .collect();

  // Status messages from italic mobile divs (shorter single-line versions)
  let status_sel = Selector::parse("div.italic p").expect("valid");
  let statuses: Vec<String> = doc
    .select(&status_sel)
    .map(|el| el.text().collect::<String>().trim().to_owned())
    .collect();

  debug_log!(
    "get_goals(): year={:?}, books_read={}, labels={:?}, counts={:?}, percents={:?}, statuses={:?}",
    year,
    books_read,
    labels,
    counts,
    percents,
    statuses
  );

  let n = labels.len().min(counts.len()).min(percents.len());
  let mut goals = Vec::new();
  for i in 0..n {
    let label = labels[i].clone();
    let percent = percents[i];
    let status = statuses.get(i).cloned().unwrap_or_default();

    // "10/26 books" → current="10", target="26", unit="books"
    let (current, target, unit) = if let Some((left, right)) = counts[i].split_once('/') {
      let current = left.trim().to_owned();
      let mut parts = right.trim().splitn(2, ' ');
      let target = parts.next().unwrap_or("").to_owned();
      let unit = parts.next().unwrap_or("").to_owned();
      (current, target, unit)
    } else {
      (String::new(), String::new(), String::new())
    };

    goals.push(GoalEntry {
      label,
      current,
      target,
      unit,
      percent,
      status,
    });
  }

  Ok(ReadingGoals {
    year,
    books_read,
    goals,
  })
}

/// The user's current StoryGraph reading streak.
#[derive(Serialize, Debug, Default)]
pub struct Streak {
  pub requirement: String,  // e.g. "1 page, or 1 minute, every day"
  pub current: String,      // e.g. "1"
  pub longest: String,      // e.g. "46"
  pub period: String,       // e.g. "Jul 18"
  pub reading_left: String, // e.g. "Reading left: 1 page or 1 minute"
}

/// Fetches the user's reading streak from `/streak/{slug}`.
pub fn get_streak() -> Result<Streak> {
  require_credentials()?;
  let user = get_user()?;
  let html = get(&format!("/streak/{}", user.slug))?;
  let doc = Html::parse_document(&html);

  // "font-medium" and bare "dl" are generic Tailwind utility classes/tags
  // that could easily match unrelated elements elsewhere on a real
  // (non-fragment) page, so scope searches as tightly as the known markup
  // allows rather than matching anywhere in the document.

  // The <p class="font-medium"> holding the requirement text is a direct
  // sibling of the "Streak" <h1> under the same parent div.
  let heading_sel = Selector::parse("h1.page-heading").expect("valid");
  let requirement_container = doc
    .select(&heading_sel)
    .find(|el| el.text().collect::<String>().trim() == "Streak")
    .and_then(|h1| h1.parent())
    .and_then(scraper::ElementRef::wrap);

  // The streak requirement ("1 page, or 1 minute, every day", "5 pages,
  // every day", etc.) is shown verbatim rather than parsed, since the
  // wording/units vary per user's streak settings.
  let requirement_sel = Selector::parse("p.font-medium span").expect("valid");
  let requirement = requirement_container
    .and_then(|c| c.select(&requirement_sel).next())
    .map(|el| el.text().collect::<String>().trim().to_owned())
    .unwrap_or_default();

  // Current/longest streak and the current-period note each live in a
  // dt/dd pair under a shared parent div inside the streak stats <dl>
  // (identified by its specific classes rather than just "dl") — walk
  // dt's parent to find its matching dd rather than assuming array
  // alignment across all pairs.
  let dt_sel = Selector::parse("dl.font-semibold dt").expect("valid");
  let dd_sel = Selector::parse("dl.font-semibold dd").expect("valid");
  let italic_sel = Selector::parse("span.italic").expect("valid");

  let mut streak = Streak {
    requirement,
    ..Default::default()
  };

  for dt in doc.select(&dt_sel) {
    let label = dt.text().collect::<String>().trim().to_owned();
    let Some(parent) = dt.parent().and_then(scraper::ElementRef::wrap) else {
      continue;
    };
    let dd_text = parent
      .select(&dd_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .unwrap_or_default();

    if label == "Current streak" {
      streak.current = dd_text;
    } else if label == "Longest streak" {
      streak.longest = dd_text;
    } else if label.starts_with("Current period") {
      streak.period = dt
        .select(&italic_sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .unwrap_or_default();
      streak.reading_left = dd_text;
    }
  }

  debug_log!("get_streak(): {streak:?}");
  Ok(streak)
}

/// A book on the user's currently-reading or paused shelf.
#[derive(Serialize, Debug)]
pub struct ReadingBook {
  pub id: String,
  pub title: String,
  pub author: String,
  pub progress_percent: u32,
  pub pages: u32,
  pub format: String,
  pub year: u32,
  pub status: String,            // "reading" or "paused"
  pub cover_url: Option<String>, // absolute URL or None
}

/// A single item in the community activity feed.
#[derive(Serialize, Debug)]
pub struct FeedItem {
  pub username: String,
  pub action: String,   // "started reading", "finished reading and reviewed", etc.
  pub time_ago: String, // "1d", "2h", "5m"
  pub book_title: String,
  pub book_author: String,
  pub cover_url: Option<String>,
  pub genres: Vec<String>,
  pub moods: Vec<String>,
  pub rating: Option<f32>, // present on "finished reading and reviewed" items
  pub review_snippet: Option<String>, // truncated review text, if present
}

/// Fetches the community activity feed from `/community`.
/// Items live inside `.news-feed-item-panes` as `[data-news-feed-item-id]` elements.
pub fn get_feed() -> Result<Vec<FeedItem>> {
  require_credentials()?;
  let html = get("/community")?;
  let doc = Html::parse_document(&html);

  let item_sel = Selector::parse("[data-news-feed-item-id]").expect("valid");
  let h3_sel = Selector::parse("h3").expect("valid");
  let profile_sel = Selector::parse(r#"a[href^="/profile/"]"#).expect("valid");
  let cover_sel = Selector::parse(".book-cover img").expect("valid");
  let title_sel = Selector::parse("h4 a").expect("valid");
  let author_sel = Selector::parse(r#"p a[href^="/authors/"]"#).expect("valid");
  let time_sel = Selector::parse(r#"span[class*="absolute"]"#).expect("valid");
  let teal_sel = Selector::parse(r#"span[class*="text-teal-800"]"#).expect("valid");
  let pink_sel = Selector::parse(r#"span[class*="text-pink-700"]"#).expect("valid");
  let rating_sel = Selector::parse(r#"a[aria-label*="Rating:"]"#).expect("valid");
  let review_sel = Selector::parse(r#"a[class*="clamp-3"]"#).expect("valid");

  let mut items = Vec::new();

  for item in doc.select(&item_sel) {
    // Time: the absolute-positioned span in the top-right corner
    let time_ago = item
      .select(&time_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .unwrap_or_default();

    // Username + action from the first h3 (mobile header)
    let (username, action) = if let Some(h3) = item.select(&h3_sel).next() {
      let username = h3
        .select(&profile_sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .unwrap_or_default();

      // Collect all text nodes; strip username prefix to get the action verb
      let full: String = h3
        .text()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
      let action = if username.is_empty() {
        full.trim().to_owned()
      } else {
        full
          .trim()
          .replacen(&username, "", 1)
          .trim()
          .trim_end_matches(':')
          .trim()
          .to_owned()
      };
      (username, action)
    } else {
      (String::new(), String::new())
    };

    if username.is_empty() {
      continue;
    }

    // Book cover
    let cover_url = item
      .select(&cover_sel)
      .next()
      .and_then(|el| {
        el.value()
          .attr("src")
          .or_else(|| el.value().attr("data-src"))
      })
      .map(|src| {
        if src.starts_with("http") {
          src.to_owned()
        } else {
          format!("{BASE_URL}{src}")
        }
      });

    let book_title = item
      .select(&title_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .unwrap_or_default();

    let book_author = item
      .select(&author_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .unwrap_or_default();

    // Genres (teal) and moods (pink) — deduplicated (each tag appears twice in mobile+desktop)
    let mut seen_g = std::collections::HashSet::new();
    let genres: Vec<String> = item
      .select(&teal_sel)
      .filter_map(|el| {
        let t = el.text().collect::<String>().trim().to_owned();
        if t.is_empty() || !seen_g.insert(t.clone()) {
          None
        } else {
          Some(t)
        }
      })
      .collect();

    let mut seen_m = std::collections::HashSet::new();
    let moods: Vec<String> = item
      .select(&pink_sel)
      .filter_map(|el| {
        let t = el.text().collect::<String>().trim().to_owned();
        if t.is_empty() || !seen_m.insert(t.clone()) {
          None
        } else {
          Some(t)
        }
      })
      .collect();

    // Rating: from aria-label="Rating: 2.0 out of 5"
    let rating = item
      .select(&rating_sel)
      .next()
      .and_then(|el| el.value().attr("aria-label"))
      .and_then(|label| {
        label
          .strip_prefix("Rating: ")
          .and_then(|s| s.split(" out of").next())
          .and_then(|s| s.trim().parse::<f32>().ok())
      });

    // Review snippet from line-clamped link
    let review_snippet = item
      .select(&review_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .filter(|s| !s.is_empty());

    items.push(FeedItem {
      username,
      action,
      time_ago,
      book_title,
      book_author,
      cover_url,
      genres,
      moods,
      rating,
      review_snippet,
    });
  }

  debug_log!("get_feed(): {} items", items.len());
  Ok(items)
}

/// Parses one shelf section of the `/currently-reading/{slug}` page into
/// books. `container_selector` is `.read-books` (currently reading) or
/// `.paused-books-panes` (paused); `status` is the label attached to each
/// result.
fn parse_shelf_section(html: &str, container_selector: &str, status: &str) -> Vec<ReadingBook> {
  let doc = Html::parse_document(html);
  let container_sel = Selector::parse(container_selector).expect("valid");
  let pane_sel = Selector::parse(".book-pane").expect("valid");
  let title_sel = Selector::parse("h3.font-bold a").expect("valid");
  let author_sel = Selector::parse(r#"a[href^="/authors/"]"#).expect("valid");
  let progress_sel = Selector::parse("input.read-status-last-reached-percent").expect("valid");
  let edition_sel = Selector::parse(".toggle-edition-info-link").expect("valid");
  let cover_sel = Selector::parse(".book-cover img").expect("valid");

  let mut books = Vec::new();

  for container in doc.select(&container_sel) {
    for pane in container.select(&pane_sel) {
      let book_id = match pane.value().attr("data-book-id") {
        Some(id) if !id.is_empty() => id.to_owned(),
        _ => continue,
      };

      let title = pane
        .select(&title_sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .unwrap_or_default();

      if title.is_empty() {
        continue;
      }

      let author = pane
        .select(&author_sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .unwrap_or_default();

      // Progress bar width first, same as parse_user_book()/find_on_shelf(): audiobooks
      // are tracked by time on StoryGraph and don't render the hidden percent input at
      // all, so that selector alone silently reads back 0 for them.
      let pane_html = pane.html();
      let progress: u32 = progress_percent_from_edit_progress(&pane_html)
        .or_else(|| {
          pane
            .select(&progress_sel)
            .next()
            .and_then(|el| el.value().attr("value"))
            .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0);

      let edition_text = pane
        .select(&edition_sel)
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default();

      let parts: Vec<&str> = edition_text.split(" \u{2022} ").collect();
      let pages: u32 = parts
        .first()
        .and_then(|s| s.trim().strip_suffix(" pages"))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0);
      let format = parts
        .get(1)
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
      let year: u32 = parts
        .get(2)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

      let cover_url = pane
        .select(&cover_sel)
        .next()
        .and_then(|el| {
          el.value()
            .attr("src")
            .or_else(|| el.value().attr("data-src"))
        })
        .map(|src| {
          if src.starts_with("http") {
            src.to_owned()
          } else {
            format!("{BASE_URL}{src}")
          }
        });

      books.push(ReadingBook {
        id: book_id,
        title,
        author,
        progress_percent: progress,
        pages,
        format,
        year,
        status: status.to_owned(),
        cover_url,
      });
    }
  }

  books
}

/// Returns books on the currently-reading shelf followed by books on the paused shelf.
/// Both sections live on the same `/currently-reading/{slug}` page:
///   .read-books         → currently reading
///   .paused-books-panes → paused
pub fn list_reading() -> Result<Vec<ReadingBook>> {
  require_credentials()?;
  let user = get_user()?;
  let html = get(&format!("/currently-reading/{}", user.slug))?;

  let mut books = parse_shelf_section(&html, ".read-books", "reading");
  books.extend(parse_shelf_section(&html, ".paused-books-panes", "paused"));

  debug_log!("list_reading(): {} books total", books.len());
  Ok(books)
}

/// Returns the logged-in user's slug. Uses `user_slug` from config if set,
/// otherwise scrapes the homepage nav for a `/profile/:slug` link.
pub fn get_user() -> Result<User> {
  require_credentials()?;

  if !CONFIG.user_slug.is_empty() {
    return Ok(User {
      username: CONFIG.user_slug.clone(),
      slug: CONFIG.user_slug.clone(),
    });
  }

  let html = get("/")?;
  let doc = Html::parse_document(&html);

  // The nav links to the user's profile as /profile/:slug; account actions like
  // /profile/edit/:slug are sub-paths and must not match.
  let sel = Selector::parse(r#"a[href^="/profile/"]"#).expect("valid selector");
  let slug = doc
    .select(&sel)
    .filter_map(|el| el.value().attr("href"))
    .filter_map(|href| href.strip_prefix("/profile/"))
    .map(str::to_owned)
    .find(|s| !s.is_empty() && !s.contains('/'))
    .context("Could not find user profile link — session may be expired")?;

  Ok(User {
    username: slug.clone(),
    slug,
  })
}

/// Searches StoryGraph by title/author/ISBN and returns matching books.
/// Audio editions are filtered out.
pub fn search(query: &str) -> Result<Vec<Book>> {
  require_credentials()?;
  let html = get(&format!(
    "/browse?search_term={}",
    urlencoding::encode(query)
  ))?;
  Ok(parse_search_results(&html))
}

/// One dialog screen of search results plus whether more rows follow the slice.
#[derive(Debug)]
pub struct SearchResult {
  pub books: Vec<Book>,
  pub has_more: bool,
}

/// Number of raw `.book-pane` elements in a `/browse` response. Used to tell a
/// genuinely empty StoryGraph page from one whose panes were all filtered out
/// (audiobooks): zero panes means the result set is exhausted.
fn count_book_panes(html: &str) -> usize {
  let doc = Html::parse_document(html);
  let sel = Selector::parse(".book-pane").expect("valid selector");
  doc.select(&sel).count()
}

/// Fetches StoryGraph `/browse` pages as needed and returns the rows for one
/// dialog screen plus whether more rows follow the slice.
///
/// StoryGraph serves a fixed number of hits per `&page=N` (about 10) while a
/// dialog screen holds `limit` rows (height-dependent, ~4–6), so screen pages
/// don't map 1:1 onto `&page=N`. `page` is the 1-based screen page; this walks
/// StoryGraph pages from the start, accumulating non-audio results (deduplicated
/// across the pagination's page-boundary overlap) until it holds at least one
/// row past the end of the requested slice, then returns the slice and reports
/// `has_more` (whether any row exists after it).
pub fn search_paged(query: &str, limit: usize, page: usize) -> Result<SearchResult> {
  require_credentials()?;
  let start = page.saturating_sub(1).saturating_mul(limit);
  let end = start.saturating_add(limit);

  // Safety cap: 50 StoryGraph pages (~500 hits) is far beyond any realistic
  // manual link search; it bounds the loop if a query returns many audio-only
  // pages that all parse to zero rows.
  const MAX_PAGES: usize = 50;

  let mut acc: Vec<Book> = Vec::new();
  let mut seen = std::collections::HashSet::new();
  let mut sg_page = 1usize;

  while acc.len() <= end && sg_page <= MAX_PAGES {
    let html = get(&format!(
      "/browse?search_term={}&page={sg_page}",
      urlencoding::encode(query)
    ))?;

    if count_book_panes(&html) == 0 {
      break;
    }

    for book in parse_search_results(&html) {
      if seen.insert(book.book_id.clone()) {
        acc.push(book);
      }
    }
    sg_page += 1;
  }

  let has_more = acc.len() > end;
  let books: Vec<Book> = acc.into_iter().skip(start).take(limit).collect();

  debug_log!(
    "search_paged({query:?}, limit={limit}, page={page}) → {} rows, has_more={has_more}",
    books.len()
  );
  Ok(SearchResult { books, has_more })
}

/// Parses the `/browse` result panes.
///
/// One `.book-pane` per hit. The title block is
/// `.book-title-author-and-series` (title, author, series link); the edition
/// line right after it carries pages/format/year as "541 pages • hardcover •
/// 2006", and the cover is `.book-cover img`.
fn parse_search_results(html: &str) -> Vec<Book> {
  let doc = Html::parse_document(html);

  let pane_sel = Selector::parse(".book-pane").expect("valid selector");
  let container_sel = Selector::parse(".book-title-author-and-series").expect("valid selector");
  let title_sel = Selector::parse(r#"a[href^="/books/"]"#).expect("valid selector");
  let author_sel = Selector::parse(r#"a[href^="/authors/"]"#).expect("valid selector");
  let series_sel = Selector::parse(r#"a[href^="/series/"]"#).expect("valid selector");
  let cover_sel = Selector::parse(".book-cover img").expect("valid selector");

  let mut results = Vec::new();
  let mut seen = std::collections::HashSet::new();

  for pane in doc.select(&pane_sel) {
    let Some(container) = pane.select(&container_sel).next() else {
      continue;
    };
    let Some(title_el) = container.select(&title_sel).next() else {
      continue;
    };
    let Some(book_id) = title_el
      .value()
      .attr("href")
      .and_then(|href| href.strip_prefix("/books/"))
      .map(str::to_owned)
    else {
      continue;
    };

    if seen.contains(&book_id) {
      continue;
    }

    let (pages, format, release_year) = edition_line(pane);

    // Skip audiobook editions: StoryGraph tracks them by listening time, so a
    // page-based sync would write nonsense. The edition line of an audio result
    // reads "11 hours • audiobook • 2020", so no " pages" is present either way.
    if format.to_lowercase().contains("audio") {
      continue;
    }

    let title = title_el.text().collect::<String>().trim().to_owned();
    let author = container
      .select(&author_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .unwrap_or_else(|| "Unknown Author".to_owned());

    let cover_url = pane.select(&cover_sel).next().and_then(|img| {
      img.value().attr("src").map(|src| {
        if src.starts_with("http") {
          src.to_owned()
        } else {
          format!("{BASE_URL}{src}")
        }
      })
    });

    seen.insert(book_id.clone());
    results.push(Book {
      book_id,
      title,
      author,
      cover_url,
      pages,
      series: parse_series(container, &series_sel),
      release_year,
    });
  }

  debug_log!("search → {} results", results.len());
  results
}

/// Reads the edition line of a result pane ("541 pages • hardcover • 2006 • 308
/// editions") into `(pages, format, release_year)`. Audiobook panes carry a
/// duration instead of a page count ("29 hours • audiobook • 2020") and are
/// reported with `pages = 0`.
fn edition_line(pane: scraper::ElementRef) -> (u32, String, Option<u32>) {
  let p_sel = Selector::parse("p").expect("valid selector");

  for p in pane.select(&p_sel) {
    let text = p.text().collect::<String>();
    let parts: Vec<&str> = text.split('\u{2022}').map(str::trim).collect();

    // Needs at least "<count> pages|hours • <format>"; this skips the series
    // line ("Mistborn #1") and other single-part paragraphs.
    let Some(first) = parts.first() else {
      continue;
    };
    let is_edition_line = [" pages", " hours", " mins"]
      .iter()
      .any(|suffix| first.ends_with(suffix));
    if !is_edition_line || parts.len() < 2 {
      continue;
    }

    let pages = first
      .strip_suffix(" pages")
      .and_then(|n| n.trim().parse().ok())
      .unwrap_or(0);
    let format = parts[1].to_owned();
    let year = parts.get(2).and_then(|s| s.parse().ok());

    return (pages, format, year);
  }

  (0, String::new(), None)
}

/// Reads the series link of the title block ("Mistborn #1") into a series
/// reference. Returns None for standalones.
fn parse_series(container: scraper::ElementRef, series_sel: &Selector) -> Option<SeriesRef> {
  let mut links = container.select(series_sel);

  let name = links
    .next()
    .map(|el| el.text().collect::<String>().trim().to_owned())
    .filter(|name| !name.is_empty())?;

  let position = links
    .next()
    .map(|el| el.text().collect::<String>())
    .and_then(|pos| pos.trim().trim_start_matches('#').parse().ok());

  Some(SeriesRef { name, position })
}

/// Search by ISBN — returns the first non-audio match, or None.
pub fn find_by_isbn(isbn: &str) -> Result<Option<Book>> {
  let results = search(isbn)?;
  Ok(results.into_iter().next())
}

/// Fetches the current user's reading status and progress for a book.
/// Returns zeroed progress fields if the book has no page count on StoryGraph
/// (common for Kindle/digital editions).
pub fn get_user_book(book_id: &str) -> Result<UserBook> {
  require_credentials()?;
  let html = get(&format!("/books/{book_id}"))?;
  parse_user_book(book_id, &html)
}

fn parse_user_book(book_id: &str, html: &str) -> Result<UserBook> {
  let doc = Html::parse_document(html);

  let status_text = text_of(&doc, ".read-status-label");
  let status_id = parse_status_text(&status_text);

  // Try progress bar width first (most reliable), then hidden inputs.
  let progress_percent = progress_percent_from_edit_progress(html).unwrap_or_else(|| {
    hidden_value(html, "read_status[last_reached_percent]")
      .parse()
      .unwrap_or(0)
  });

  let progress_pages: u32 = hidden_value(html, "read_status[last_reached_pages]")
    .parse()
    .unwrap_or_else(|_| {
      extract_value_near_name(html, "read-status-last-reached-pages")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
    });
  let book_pages: u32 = hidden_value(html, "read_status[book_num_of_pages]")
    .parse()
    .unwrap_or_else(|_| {
      extract_value_near_name(html, "read-status-book-num-of-pages")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
    });

  let progress_type = Selector::parse(".read-status-progress-type option[selected]")
    .ok()
    .and_then(|sel| doc.select(&sel).next())
    .and_then(|el| el.value().attr("value"))
    .unwrap_or("percentage")
    .to_owned();

  let edition_format = {
    let sel = Selector::parse(".edition-info p").expect("valid selector");
    doc
      .select(&sel)
      .find(|el| el.text().collect::<String>().contains("Format:"))
      .map(|el| {
        el.text()
          .collect::<String>()
          .replacen("Format:", "", 1)
          .trim()
          .to_owned()
      })
      .unwrap_or_default()
  };

  Ok(UserBook {
    book_id: book_id.to_owned(),
    status_id,
    progress_percent,
    progress_pages,
    book_pages,
    progress_type,
    edition_format,
  })
}

/// Sets the reading status of a book (to-read, reading, read, paused, dnf).
/// POSTs to `/update-status.js` and re-fetches the book page to confirm.
pub fn update_status(book_id: &str, status_id: u8) -> Result<UserBook> {
  require_credentials()?;
  let (_html, data) = fetch_page_data(book_id)?;
  let status_str = status_to_str(status_id);

  let path = format!("/update-status.js?book_id={book_id}&status={status_str}");
  post_form(&path, &[("authenticity_token", data.csrf.as_str())])?;

  // Re-fetch to return current state.
  parse_user_book(book_id, &get(&format!("/books/{book_id}"))?)
}

/// Updates reading progress for a book. `progress_type` is `"percentage"` or `"pages"`.
/// Fetches the book page first to extract the CSRF token and current page count.
/// If the book has no reading status, automatically sets it to "Currently Reading" first.
pub fn update_progress(book_id: &str, progress_type: &str, value: u32) -> Result<UserBook> {
  require_credentials()?;
  let (html, mut data) = fetch_page_data(book_id)?;

  let current_status = parse_user_book(book_id, &html)?.status_id;
  if current_status.is_none() {
    update_status(book_id, ReadingStatus::CurrentlyReading as u8)?;
    let (_, fresh) = fetch_page_data(book_id)?;
    data = fresh;
  } else if current_status == Some(ReadingStatus::Read as u8) {
    update_status(book_id, ReadingStatus::Rereading as u8)?;
    let (_, fresh) = fetch_page_data(book_id)?;
    data = fresh;
  }

  let value_str = value.to_string();

  post_form(
    "/update-progress",
    &[
      ("authenticity_token", &data.csrf),
      ("read_status[progress_number]", &value_str),
      ("read_status[progress_type]", progress_type),
      ("read_status[book_num_of_pages]", &data.book_pages),
      ("book_id", book_id),
      ("on_book_page", "true"),
    ],
  )?;

  parse_user_book(book_id, &get(&format!("/books/{book_id}"))?)
}

/// Creates a reading journal entry (note + progress snapshot) via `/update-progress-with-note`.
/// StoryGraph records this as a progress update with an attached note in the reading journal.
pub fn insert_journal(book_id: &str, note: &str, progress: u32, progress_type: &str) -> Result<()> {
  require_credentials()?;
  let (_html, data) = fetch_page_data(book_id)?;
  let progress_str = progress.to_string();

  let now = chrono::Local::now();
  let day = now.format("%d").to_string();
  let month = now.format("%m").to_string();
  let year = now.format("%Y").to_string();

  post_form(
    "/update-progress-with-note",
    &[
      ("authenticity_token", &data.csrf),
      ("progress_update_date[day]", &day),
      ("progress_update_date[month]", &month),
      ("progress_update_date[year]", &year),
      ("progress_minutes", ""),
      ("progress_number", &progress_str),
      ("progress_type", progress_type),
      ("last_reached_pages", &data.last_pages),
      ("book_num_of_pages", &data.book_pages),
      ("last_reached_percent", &data.last_percent),
      ("note", note),
      ("book_id", book_id),
      ("return_to", ""),
      ("button", ""),
    ],
  )?;

  Ok(())
}

/// HTML-escapes `&`, `<`, `>` so plain text can be embedded in the review form
/// body without being interpreted as markup.
fn escape_html(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
}

/// Submits a new review to StoryGraph via POST /reviews.
/// Rating is split: 4.75 → stars_integer=4, stars_decimal=75.
/// mood_ids are the StoryGraph integer IDs 1–14.
#[allow(clippy::too_many_arguments)] // maps a form with 10 semantically-distinct fields
pub fn submit_review(
  book_id: &str,
  rating: Option<f32>,
  explanation: &str,
  mood_ids: &[u8],
  pace: &str,
  character_or_plot_driven: &str,
  strong_character_development: &str,
  loveable_characters: &str,
  diverse_characters: &str,
  flawed_characters: &str,
) -> Result<()> {
  require_credentials()?;

  let book_uuid = book_id;

  // Fetch /reviews/new for a CSRF token scoped to the review form.
  let form_html = get(&format!(
    "/reviews/new?book_id={book_uuid}&return_to=%2Fbooks%2F{book_uuid}"
  ))?;
  let csrf = csrf_token(&form_html).context("Could not find CSRF token on reviews/new page")?;

  let (stars_int, stars_dec) = match rating {
    Some(r) if r > 0.0 => {
      let int_part = r.floor() as u8;
      let frac = r - int_part as f32;
      let dec = if frac < 0.13 {
        String::new()
      } else if frac < 0.38 {
        "25".into()
      } else if frac < 0.63 {
        "5".into()
      } else {
        "75".into()
      };
      (int_part.to_string(), dec)
    }
    _ => (String::new(), String::new()),
  };

  let explanation_html = if explanation.is_empty() {
    String::new()
  } else if explanation.starts_with('<') {
    // User supplied their own markup — pass through untouched.
    explanation.to_owned()
  } else {
    format!("<div>{}</div>", escape_html(explanation))
  };

  let return_to = format!("/books/{book_uuid}");
  let mut fields: Vec<(String, String)> = vec![
    ("authenticity_token".into(), csrf),
    ("stars_integer".into(), stars_int),
    ("stars_decimal".into(), stars_dec),
    ("review[explanation]".into(), explanation_html),
    ("review[book_id]".into(), book_uuid.to_string()),
    ("return_to".into(), return_to),
    ("review[pace]".into(), pace.to_owned()),
    (
      "review[character_or_plot_driven]".into(),
      character_or_plot_driven.to_owned(),
    ),
    (
      "review[strong_character_development]".into(),
      strong_character_development.to_owned(),
    ),
    (
      "review[loveable_characters]".into(),
      loveable_characters.to_owned(),
    ),
    (
      "review[diverse_characters]".into(),
      diverse_characters.to_owned(),
    ),
    (
      "review[flawed_characters]".into(),
      flawed_characters.to_owned(),
    ),
    ("review[themes]".into(), String::new()),
    ("review[content_warning_description]".into(), String::new()),
    ("button".into(), String::new()),
  ];
  for &id in mood_ids {
    fields.push(("review[mood_ids][]".into(), id.to_string()));
  }

  post_form_browser("/reviews", &fields)?;
  Ok(())
}

/// Returns all non-audio editions of a book from its `/editions` page.
/// Used by the hook's edition picker so the user can switch to the correct
/// physical or digital edition for accurate page tracking.
pub fn list_editions(book_id: &str, reading_format: i32, language: &str) -> Result<Vec<Edition>> {
  require_credentials()?;
  let html = get(&format!("/books/{book_id}/editions"))?;
  debug_log!("list_editions({book_id}): HTML len={}", html.len());

  let editions = parse_editions(&html);

  let is_digital = |f: &str| {
    let f = f.to_lowercase();
    f.contains("ebook")
      || f.contains("e-book")
      || f.contains("epub")
      || f.contains("kindle")
      || f.contains("digital")
      || f.contains("mobi")
      || f.contains("pdf")
  };

  let editions: Vec<Edition> = editions
    .into_iter()
    .filter(|e| {
      if reading_format == READING_FORMAT_PRINT {
        !is_digital(&e.format)
      } else if reading_format == READING_FORMAT_EBOOK {
        is_digital(&e.format)
      } else {
        true
      }
    })
    .filter(|e| language.is_empty() || e.language.eq_ignore_ascii_case(language))
    .collect();

  debug_log!(
    "list_editions({book_id}): returning {} editions (format={reading_format}, lang={language:?})",
    editions.len()
  );
  Ok(editions)
}

/// Parses the `/books/:id/editions` page into editions, skipping audiobook
/// editions (tracked by listening time, not pages).
fn parse_editions(html: &str) -> Vec<Edition> {
  let doc = Html::parse_document(html);

  let pane_sel = Selector::parse(".book-pane").expect("valid selector");
  let info_sel = Selector::parse(".edition-info p").expect("valid selector");
  let img_sel = Selector::parse(".book-cover img").expect("valid selector");
  let title_sel = Selector::parse(".book-title-author-and-series h3 a").expect("valid selector");
  let link_sel = Selector::parse(r#"a[href^="/books/"]"#).expect("valid selector");

  let mut editions = Vec::new();

  for pane in doc.select(&pane_sel) {
    // Prefer data-book-id attribute; fall back to first /books/ link in the pane.
    let book_id = pane
      .value()
      .attr("data-book-id")
      .map(str::to_owned)
      .or_else(|| {
        pane
          .select(&link_sel)
          .filter_map(|el| el.value().attr("href"))
          .filter_map(|href| href.strip_prefix("/books/"))
          .map(str::to_owned)
          .next()
      });

    let book_id = match book_id {
      Some(id) => id,
      None => continue,
    };

    let title = pane
      .select(&title_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .unwrap_or_default();

    let mut isbn = String::new();
    let mut format = String::new();
    let mut language = String::new();
    let mut publisher = String::new();
    let mut pub_date = String::new();
    let mut pages: Option<u32> = None;

    for p in pane.select(&info_sel) {
      let text = p.text().collect::<String>();
      if text.contains("ISBN/UID:") {
        isbn = text.replacen("ISBN/UID:", "", 1).trim().to_owned();
      } else if text.contains("Format:") {
        format = text.replacen("Format:", "", 1).trim().to_owned();
      } else if text.contains("Language:") {
        language = text.replacen("Language:", "", 1).trim().to_owned();
      } else if text.contains("Publisher:") {
        publisher = text.replacen("Publisher:", "", 1).trim().to_owned();
      } else if text.contains("Edition Pub Date:") {
        pub_date = text.replacen("Edition Pub Date:", "", 1).trim().to_owned();
      }
    }

    // Page count from "X pages" text
    let pane_text = pane.text().collect::<String>();
    for (a, b) in pane_text
      .split_whitespace()
      .zip(pane_text.split_whitespace().skip(1))
    {
      if b == "pages"
        && let Ok(n) = a.parse::<u32>()
      {
        pages = Some(n);
        break;
      }
    }

    // Skip audio editions
    if format.to_lowercase().contains("audio") {
      continue;
    }

    let cover_url = pane
      .select(&img_sel)
      .next()
      .and_then(|el| el.value().attr("src"))
      .map(str::to_owned);

    editions.push(Edition {
      book_id,
      title,
      format,
      pages,
      isbn,
      language,
      publisher,
      pub_date,
      cover_url,
    });
  }

  debug_log!("parse_editions(): {} editions", editions.len());
  editions
}

/// Returns what fraction of the meaningful words in `query_title` appear in `result_title`.
pub fn title_match_score(query_title: &str, result_title: &str) -> f32 {
  const STOPS: &[&str] = &[
    "the", "a", "an", "of", "in", "by", "to", "and", "or", "for", "at", "its", "is",
  ];

  let tokenize = |s: &str| -> Vec<String> {
    s.to_lowercase()
      // Split on non-alphanumeric so volume/edition numbers ("Vol. 1" vs "Vol. 2")
      // survive as their own tokens instead of vanishing as delimiter characters -
      // otherwise titles that only differ by a number score as identical matches.
      .split(|c: char| !c.is_alphanumeric())
      .filter(|w| {
        !w.is_empty()
          && !STOPS.contains(w)
          && (w.len() > 1 || w.chars().all(|c| c.is_ascii_digit()))
      })
      .map(str::to_owned)
      .collect()
  };

  let query_words = tokenize(query_title);
  if query_words.is_empty() {
    return 0.0;
  }

  let result_words: std::collections::HashSet<String> =
    tokenize(result_title).into_iter().collect();
  let matches = query_words
    .iter()
    .filter(|w| result_words.contains(*w))
    .count();
  matches as f32 / query_words.len() as f32
}

/// Searches the user's currently-reading shelf for a book matching `query_title` without
/// needing a book_id first. Parses shelf HTML once; returns `(book_id, progress_percent)`
/// for the best title match (score ≥ 0.9 - deliberately strict, see below), or `None`
/// if nothing matches.
pub fn find_on_shelf(query_title: &str) -> Result<Option<(String, u32)>> {
  require_credentials()?;
  let user = get_user()?;
  let html = get(&format!("/currently-reading/{}", user.slug))?;
  let doc = Html::parse_document(&html);

  let pane_sel = Selector::parse(".book-pane").expect("valid");
  let title_sel = Selector::parse("h3.font-bold a").expect("valid");
  let progress_sel = Selector::parse("input.read-status-last-reached-percent").expect("valid");

  let mut best: Option<(String, f32, u32)> = None;

  for pane in doc.select(&pane_sel) {
    let book_id = match pane.value().attr("data-book-id") {
      Some(id) if !id.is_empty() => id.to_owned(),
      _ => continue,
    };

    let title: String = pane
      .select(&title_sel)
      .next()
      .map(|el| el.text().collect::<String>().trim().to_owned())
      .unwrap_or_default();

    if title.is_empty() {
      continue;
    }

    let score = title_match_score(query_title, &title);
    // High bar deliberately: this is meant to find the *exact* book already on the
    // shelf with no confirmation step, so a near-duplicate title (e.g. another volume
    // of the same series, differing by only one word) must not qualify. 0.5 let a
    // 5/6-word match through, which is exactly the "Vol. 1" vs "Vol. 2" case.
    if score >= 0.9 {
      let pane_html = pane.html();

      // Progress bar width first, same as parse_user_book(): audiobooks are
      // tracked by time on StoryGraph and don't render the hidden percent
      // input at all, so that selector alone silently reads back 0 for them.
      // The rendered progress bar's width is present regardless of tracking
      // method and is what the site itself shows the user.
      let progress: u32 = progress_percent_from_edit_progress(&pane_html)
        .or_else(|| {
          pane
            .select(&progress_sel)
            .next()
            .and_then(|el| el.value().attr("value"))
            .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0);

      if best.as_ref().is_none_or(|(_, s, _)| score > *s) {
        best = Some((book_id, score, progress));
      }
    }
  }

  debug_log!(
    "find_on_shelf({query_title:?}): {:?}",
    best.as_ref().map(|(id, s, p)| (id, s, p))
  );
  Ok(best.map(|(id, _, progress)| (id, progress)))
}

/// Finds which edition of this book the user currently has on their StoryGraph
/// currently-reading shelf. Fetches the shelf page and looks for any edition UUID
/// that matches a known edition of the same work.
pub fn find_current_edition(book_id: &str) -> Result<String> {
  require_credentials()?;

  let editions = list_editions(book_id, 0, "")?;
  let user = get_user()?;
  let shelf_html = get(&format!("/currently-reading/{}", user.slug))?;

  // Check the stored book_id first — it may already be the right one.
  if shelf_html.contains(book_id) {
    return Ok(book_id.to_owned());
  }

  // Check each edition UUID against the shelf HTML.
  for edition in &editions {
    if shelf_html.contains(&edition.book_id) {
      return Ok(edition.book_id.clone());
    }
  }

  anyhow::bail!("Could not find this book on your currently reading shelf on StoryGraph.")
}

/// If `book_id` is not a digital edition, returns the book_id of a digital edition
/// of the same work. Falls back to the original book_id if no digital edition is found.
pub fn prefer_digital_edition(book_id: &str) -> Result<String> {
  require_credentials()?;
  let html = get(&format!("/books/{book_id}"))?;
  let user_book = parse_user_book(book_id, &html)?;

  let fmt = user_book.edition_format.to_lowercase();
  if fmt.is_empty() || fmt.contains("digital") {
    return Ok(book_id.to_owned());
  }

  let editions = list_editions(book_id, 0, "")?;
  Ok(
    editions
      .into_iter()
      .find(|e| e.format.to_lowercase().contains("digital"))
      .map(|e| e.book_id)
      .unwrap_or_else(|| book_id.to_owned()),
  )
}

/// Switches the user's linked edition from `from_id` to `to_id`.
/// Finds the switch form on the editions page and POSTs it, so StoryGraph
/// transfers reading progress to the new edition.
pub fn switch_edition(from_id: &str, to_id: &str) -> Result<bool> {
  require_credentials()?;
  let html = get(&format!("/books/{from_id}/editions"))?;
  let doc = Html::parse_document(&html);
  let csrf = csrf_token(&html).context("Could not find CSRF token on editions page")?;

  // Find the switch form for the target edition.
  let form_sel = Selector::parse("form[action='/switch-editions']").expect("valid selector");
  let input_sel = Selector::parse("input").expect("valid selector");

  let mut form_csrf = csrf.clone();
  let mut true_from_id = from_id.to_owned();

  for form in doc.select(&form_sel) {
    let inputs: std::collections::HashMap<String, String> = form
      .select(&input_sel)
      .filter_map(|el| {
        let name = el.value().attr("name")?.to_owned();
        let value = el.value().attr("value").unwrap_or("").to_owned();
        Some((name, value))
      })
      .collect();

    if inputs.get("to_book_id").map(String::as_str) == Some(to_id) {
      if let Some(v) = inputs.get("authenticity_token") {
        form_csrf = v.clone();
      }
      if let Some(v) = inputs.get("from_book_id") {
        true_from_id = v.clone();
      }
      break;
    }
  }

  post_form(
    "/switch-editions",
    &[
      ("authenticity_token", &form_csrf),
      ("from_book_id", &true_from_id),
      ("to_book_id", to_id),
      ("button", ""),
    ],
  )?;

  Ok(true)
}

/// Fetches the reading journal entries for a book from `/journal?book_id={uuid}`.
/// StoryGraph uses UUIDs (not slugs) in /books/ URLs, so book_id is already the UUID
/// that the journal endpoint expects.
pub fn list_journal(book_id: &str, limit: usize, offset: usize) -> Result<Vec<JournalActivity>> {
  require_credentials()?;

  let html = get(&format!("/journal?book_id={book_id}"))?;
  Ok(parse_journal(&html, limit, offset))
}

/// Parses `/journal?book_id={uuid}` entries into activities.
fn parse_journal(html: &str, limit: usize, offset: usize) -> Vec<JournalActivity> {
  let doc = Html::parse_document(html);

  let entry_sel = Selector::parse(".journal-entry-panes > div").expect("valid selector");
  let date_sel = Selector::parse("p.font-semibold").expect("valid selector");
  let badge_sel = Selector::parse("span[title]").expect("valid selector");
  let progress_sel = Selector::parse(r#"div[style*="width:"]"#).expect("valid selector");
  let trix_sel = Selector::parse("div.trix-content").expect("valid selector");
  let blockquote_sel = Selector::parse("blockquote").expect("valid selector");
  let note_div_sel =
    Selector::parse("div.trix-content > div:not(.trix-content)").expect("valid selector");

  let all_entries: Vec<_> = doc.select(&entry_sel).collect();
  debug_log!("parse_journal(): found {} entries", all_entries.len());

  let mut activities = Vec::new();
  let mut prev_progress: u32 = 0;

  for el in all_entries.iter().skip(offset).take(limit) {
    let date_text = el
      .select(&date_sel)
      .next()
      .and_then(|p| p.text().next())
      .map(|t| t.trim().to_owned())
      .unwrap_or_default();
    let action_at = parse_storygraph_date(&date_text);

    // Status badge ("Started reading", "Want to read", etc.)
    if let Some(badge) = el.select(&badge_sel).next() {
      let title = badge.value().attr("title").unwrap_or("").to_lowercase();
      let event = if title.contains("start") {
        "user_book_read_started"
      } else if title.contains("finish") {
        "user_book_read_finished"
      } else if title.contains("want to read") {
        "status_want_to_read"
      } else if title.contains("stop") || title.contains("dnf") || title.contains("did not finish")
      {
        "status_stopped"
      } else if title.contains("paused") {
        // "Paused reading of book"
        "status_paused"
      } else if title.contains("resumed") {
        // "Resumed reading of book"
        "status_resumed"
      } else {
        "status_read"
      };
      activities.push(JournalActivity {
        event: event.to_owned(),
        action_at,
        entry: String::new(),
        metadata: Value::Null,
      });
      continue;
    }

    // Progress percentage from progress bar width style
    let progress: u32 = el
      .select(&progress_sel)
      .next()
      .and_then(|div| div.value().attr("style"))
      .and_then(percent_from_style)
      .unwrap_or(0);

    // Text content from the inner trix editor div.
    // If all the text lives inside <blockquote> descendants, it's a book quote.
    // We compare flat text of the whole trix div vs flat text of blockquote
    // descendants — equal means everything is quoted.
    let (entry_text, all_blockquote, quote_text) = if let Some(trix) = el.select(&trix_sel).next() {
      let flat: String = trix
        .text()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

      if flat.is_empty() {
        (String::new(), false, None)
      } else {
        let bq: String = trix
          .select(&blockquote_sel)
          .flat_map(|b| b.text())
          .map(|t| t.trim())
          .filter(|t| !t.is_empty())
          .collect::<Vec<_>>()
          .join(" ");

        if bq.is_empty() {
          // Pure note, no blockquote
          (flat, false, None)
        } else if bq == flat {
          // Everything is a blockquote — pure quote
          (bq, true, None)
        } else {
          // Mixed: blockquote + annotation — put quote in metadata, annotation in entry
          let non_bq: String = trix
            .select(&note_div_sel)
            .flat_map(|d| d.text())
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

          if non_bq.is_empty() {
            (flat, false, None)
          } else {
            (non_bq, false, Some(bq))
          }
        }
      }
    } else {
      (String::new(), false, None)
    };

    if entry_text.is_empty() {
      activities.push(JournalActivity {
        event: "progress_updated".to_owned(),
        action_at,
        entry: String::new(),
        metadata: json!({ "progress_was": prev_progress, "progress": progress }),
      });
    } else {
      activities.push(JournalActivity {
        event: if all_blockquote { "quote" } else { "note" }.to_owned(),
        action_at,
        entry: entry_text,
        metadata: match quote_text {
          Some(q) => json!({ "quote": q }),
          None => Value::Null,
        },
      });
    }

    prev_progress = progress;
  }

  debug_log!("parse_journal(): {} activities", activities.len());
  activities
}

fn parse_storygraph_date(date_text: &str) -> String {
  let parts: Vec<&str> = date_text.split_whitespace().collect();
  if parts.len() == 3 {
    let day: u32 = parts[0].parse().unwrap_or(1);
    let month: u32 = match parts[1].to_lowercase().as_str() {
      "january" | "januar" | "jan" => 1,
      "february" | "februar" | "feb" => 2,
      "march" | "märz" | "maerz" | "mar" => 3,
      "april" | "apr" => 4,
      "may" | "mai" => 5,
      "june" | "juni" | "jun" => 6,
      "july" | "juli" | "jul" => 7,
      "august" | "aug" => 8,
      "september" | "sep" | "sept" => 9,
      "october" | "oktober" | "okt" | "oct" => 10,
      "november" | "nov" => 11,
      "december" | "dezember" | "dez" | "dec" => 12,
      _ => 0,
    };
    let year: i32 = parts[2].parse().unwrap_or(2024);
    if month > 0
      && let Some(d) = chrono::NaiveDate::from_ymd_opt(year, month, day)
    {
      return format!("{}T12:00:00Z", d.format("%Y-%m-%d"));
    }
  }
  date_text.to_owned()
}

/// Result of a successful email/password login.
#[derive(Serialize, Debug)]
pub struct LoginResult {
  pub session_cookie: String,
  pub remember_user_token: String,
}

/// Logs in with email + password and returns the session cookies StoryGraph
/// issued. Uses a fresh cookie jar (not the config-seeded singleton), mirrors
/// the browser's sign-in form POST, and reads the cookies back from the jar.
///
/// NOTE: as of late 2026 `/users/sign_in` sits behind a Cloudflare bot
/// challenge (HTTP 403 "Just a moment...") for non-browser clients, so this
/// currently fails with "Could not find CSRF token" unless the challenge is
/// lifted or a valid `cf_clearance` cookie is already present in the jar
/// (e.g. pre-seeded from a browser session).
pub fn login(email: &str, password: &str) -> Result<LoginResult> {
  let base: Url = BASE_URL.parse().expect("BASE_URL is valid");
  let jar = Arc::new(Jar::default());
  let client = build_client(jar.clone()).context("Failed to build login client")?;

  let sign_in_url = format!("{BASE_URL}/users/sign_in");

  // 1. Load the sign-in page to obtain a CSRF token.
  let res = client
    .get(&sign_in_url)
    .send()
    .context("Failed to load sign-in page")?;
  let html = res.text().context("Failed to read sign-in page body")?;
  let csrf = csrf_token(&html).context("Could not find CSRF token on sign-in page")?;

  // 2. Submit credentials, mirroring the browser form.
  let res = client
    .post(&sign_in_url)
    .header("Origin", BASE_URL)
    .header("Referer", &sign_in_url)
    .form(&[
      ("authenticity_token", csrf.as_str()),
      ("user[email]", email),
      ("user[password]", password),
      ("commit", "Sign in"),
    ])
    .send()
    .context("Failed to submit sign-in form")?;

  // 3. On success StoryGraph redirects away from the sign-in page.
  if res.url().path().contains("/users/sign_in") {
    bail!("Login failed — check your email address and password.");
  }

  // 4. Read the session cookies back from the jar (a "name=value; name=value"
  //    header string).
  let cookies_header = jar.cookies(&base).context("Login produced no cookies")?;
  let cookies_str = cookies_header.to_str().unwrap_or("");
  let mut session_cookie = String::new();
  let mut remember_user_token = String::new();
  for pair in cookies_str.split(';') {
    if let Some((name, value)) = pair.trim().split_once('=') {
      match name.trim() {
        "_storygraph_session" => session_cookie = value.trim().to_owned(),
        "remember_user_token" => remember_user_token = value.trim().to_owned(),
        _ => {}
      }
    }
  }

  if session_cookie.is_empty() {
    bail!("Login did not return a session cookie.");
  }

  Ok(LoginResult {
    session_cookie,
    remember_user_token,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn title_match_score_identical_is_one() {
    assert!((title_match_score("The Great Gatsby", "The Great Gatsby") - 1.0).abs() < f32::EPSILON);
  }

  #[test]
  fn title_match_score_empty_query_is_zero() {
    assert_eq!(title_match_score("", "anything"), 0.0);
  }

  #[test]
  fn title_match_score_volume_differs() {
    // "Vol. 1" vs "Vol. 2" must not score as identical (3/4 words match).
    let s = title_match_score("Harry Potter Vol. 1", "Harry Potter Vol. 2");
    assert!((s - 0.75).abs() < f32::EPSILON, "got {s}");
  }

  #[test]
  fn parse_status_text_cases() {
    assert_eq!(parse_status_text("Currently reading"), Some(2));
    assert_eq!(parse_status_text("Rereading"), Some(6));
    assert_eq!(parse_status_text("Want to Read"), Some(1));
    assert_eq!(parse_status_text("To-read"), Some(1));
    assert_eq!(parse_status_text("Paused"), Some(4));
    assert_eq!(parse_status_text("Did not finish"), Some(5));
    assert_eq!(parse_status_text("Read"), Some(3));
    assert_eq!(parse_status_text("banana"), None);
  }

  #[test]
  fn status_to_str_roundtrip() {
    assert_eq!(status_to_str(1), "to-read");
    assert_eq!(status_to_str(2), "currently-reading");
    assert_eq!(status_to_str(3), "read");
    assert_eq!(status_to_str(4), "paused");
    assert_eq!(status_to_str(5), "did-not-finish");
    assert_eq!(status_to_str(6), "rereading");
    assert_eq!(status_to_str(99), "currently-reading"); // unknown falls back
  }

  #[test]
  fn parse_storygraph_date_full_english_month() {
    assert_eq!(
      parse_storygraph_date("17 October 2026"),
      "2026-10-17T12:00:00Z"
    );
    assert_eq!(
      parse_storygraph_date("1 January 2024"),
      "2024-01-01T12:00:00Z"
    );
  }

  #[test]
  fn parse_storygraph_date_garbage_passthrough() {
    assert_eq!(parse_storygraph_date("some day ago"), "some day ago");
  }

  #[test]
  fn parse_storygraph_date_german_month_and_abbreviation() {
    assert_eq!(
      parse_storygraph_date("17 Oktober 2026"),
      "2026-10-17T12:00:00Z"
    );
    assert_eq!(parse_storygraph_date("5 Dec 2020"), "2020-12-05T12:00:00Z");
  }

  #[test]
  fn escape_html_escapes_markup_chars() {
    assert_eq!(escape_html("<3 & >"), "&lt;3 &amp; &gt;");
    assert_eq!(escape_html("plain"), "plain");
  }

  #[test]
  fn percent_from_style_parses_width() {
    assert_eq!(percent_from_style("width: 47%"), Some(47));
    assert_eq!(percent_from_style("width:47.5%"), Some(48)); // rounds
    assert_eq!(percent_from_style("width: 100%;"), Some(100));
    assert_eq!(percent_from_style("color: red"), None);
  }

  #[test]
  fn progress_percent_from_edit_progress_finds_bar() {
    assert_eq!(
      progress_percent_from_edit_progress("<div class='edit-progress' style='width: 33%'></div>"),
      Some(33)
    );
    assert_eq!(progress_percent_from_edit_progress("no bar here"), None);
  }

  // Trimmed markup from a real /browse response (captured 2026-10-19): classes,
  // nesting and text structure preserved, page chrome removed. One versioned
  // print edition, one standalone print edition, one audiobook edition.
  const BROWSE_FIXTURE: &str = r#"
<div class="search-results-books"><span class="search-results-books-panes">
  <div class="book-pane" data-book-id=99860520-9842-41bd-866b-4e5f12558d20>
    <div class="book-pane-content">
      <div class="col-span-2 cover-image-column">
        <div class="book-cover">
          <a href="/books/99860520-9842-41bd-866b-4e5f12558d20">
            <img alt="The Final Empire by Brandon Sanderson" class="rounded-sm" src="https://cdn.thestorygraph.com/s1hm0zy7tchue3u0rquctj5i2vv0" />
          </a>
        </div>
      </div>
      <div class="col-span-6 w-[93%]">
        <div class="book-title-author-and-series">
          <p class="font-semibold tracking-tight"><a href="/series/2">Mistborn</a> <a href="/series/2">#1</a></p>
          <h3 class="font-bold text-xl"><a href="/books/99860520-9842-41bd-866b-4e5f12558d20">The Final Empire</a></h3>
          <p class="font-body mb-1 mt-1 text-sm"><a href="/authors/2ee3fed7-9bff-4a19-868c-d194ca72ce26">Brandon Sanderson</a></p>
        </div>
        <p class="text-xs font-light text-darkestGrey dark:text-grey mt-1">
          <span class="toggle-edition-info-link">541 pages<span class="text-darkerGrey"> • </span>hardcover<span class="text-darkerGrey"> • </span>2006<svg class="inline ml-0.5 w-3 h-3"><path d="M9 5l7 7-7 7"></path></svg></span><span class="text-darkerGrey"> • </span><a href="/books/99860520-9842-41bd-866b-4e5f12558d20/editions">308 editions</a>
        </p>
      </div>
    </div>
  </div>

  <div class="book-pane" data-book-id=11111111-2222-3333-4444-555555555555>
    <div class="book-pane-content">
      <div class="col-span-2 cover-image-column">
        <div class="book-cover">
          <a href="/books/11111111-2222-3333-4444-555555555555"><img alt="Elantris" src="https://cdn.thestorygraph.com/elantris" /></a>
        </div>
      </div>
      <div class="col-span-6 w-[93%]">
        <div class="book-title-author-and-series">
          <h3 class="font-bold text-xl"><a href="/books/11111111-2222-3333-4444-555555555555">Elantris</a></h3>
          <p class="font-body mb-1 mt-1 text-sm"><a href="/authors/2ee3fed7-9bff-4a19-868c-d194ca72ce26">Brandon Sanderson</a></p>
        </div>
        <p class="text-xs font-light text-darkestGrey dark:text-grey mt-1">
          <span class="toggle-edition-info-link">622 pages<span class="text-darkerGrey"> • </span>paperback<span class="text-darkerGrey"> • </span>2019</span><span class="text-darkerGrey"> • </span><a href="/books/11111111-2222-3333-4444-555555555555/editions">90 editions</a>
        </p>
      </div>
    </div>
  </div>

  <div class="book-pane" data-book-id=99999999-8888-7777-6666-555555555555>
    <div class="book-pane-content">
      <div class="col-span-2 cover-image-column">
        <div class="book-cover">
          <a href="/books/99999999-8888-7777-6666-555555555555"><img alt="The Final Empire audiobook" src="https://cdn.thestorygraph.com/audio" /></a>
        </div>
      </div>
      <div class="col-span-6 w-[93%]">
        <div class="book-title-author-and-series">
          <h3 class="font-bold text-xl"><a href="/books/99999999-8888-7777-6666-555555555555">The Final Empire</a></h3>
          <p class="font-body mb-1 mt-1 text-sm"><a href="/authors/2ee3fed7-9bff-4a19-868c-d194ca72ce26">Brandon Sanderson</a></p>
        </div>
        <p class="text-xs font-light text-darkestGrey dark:text-grey mt-1">
          <span class="toggle-edition-info-link">29 hours<span class="text-darkerGrey"> • </span>audiobook<span class="text-darkerGrey"> • </span>2020</span>
        </p>
      </div>
    </div>
  </div>
</span></div>
"#;

  #[test]
  fn parse_search_results_extracts_edition_metadata() {
    let results = parse_search_results(BROWSE_FIXTURE);

    // The audiobook pane is filtered out.
    assert_eq!(results.len(), 2);

    let first = &results[0];
    assert_eq!(first.book_id, "99860520-9842-41bd-866b-4e5f12558d20");
    assert_eq!(first.title, "The Final Empire");
    assert_eq!(first.author, "Brandon Sanderson");
    assert_eq!(first.pages, 541);
    assert_eq!(first.release_year, Some(2006));
    assert_eq!(
      first.series,
      Some(SeriesRef {
        name: "Mistborn".to_owned(),
        position: Some(1)
      })
    );
    assert_eq!(
      first.cover_url.as_deref(),
      Some("https://cdn.thestorygraph.com/s1hm0zy7tchue3u0rquctj5i2vv0")
    );
  }

  #[test]
  fn parse_search_results_handles_standalone() {
    let results = parse_search_results(BROWSE_FIXTURE);
    let standalone = &results[1];

    assert_eq!(standalone.title, "Elantris");
    assert_eq!(standalone.pages, 622);
    assert_eq!(standalone.release_year, Some(2019));
    assert_eq!(standalone.series, None);
  }

  #[test]
  fn parse_search_results_serializes_series_for_the_hook() {
    let results = parse_search_results(BROWSE_FIXTURE);
    let value = serde_json::to_value(&results[0]).expect("serializable");

    // BookRow::getMeta/getSeries read exactly these keys.
    assert_eq!(value["pages"], 541);
    assert_eq!(value["release_year"], 2006);
    assert_eq!(value["series"]["name"], "Mistborn");
    assert_eq!(value["series"]["position"], 1);
    assert_eq!(value["authors"][0], "Brandon Sanderson");
    // Standalones omit the key entirely instead of sending null.
    let standalone = serde_json::to_value(&results[1]).expect("serializable");
    assert!(standalone.get("series").is_none());
  }

  #[test]
  fn is_cloudflare_challenge_detects_challenge_page() {
    // Markers taken from a real 403 body served to non-browser clients
    // (captured 2026-10-19): title "Just a moment...", window._cf_chl_opt,
    // and scripts from /cdn-cgi/challenge-platform/.
    assert!(is_cloudflare_challenge(
      &StatusCode::FORBIDDEN,
      "<html lang=\"en-US\"><head><title>Just a moment...</title></head></html>"
    ));
    assert!(is_cloudflare_challenge(
      &StatusCode::FORBIDDEN,
      "<script>window._cf_chl_opt={cvId:'3'};</script>"
    ));
    assert!(is_cloudflare_challenge(
      &StatusCode::FORBIDDEN,
      "<script src=\"/cdn-cgi/challenge-platform/h/b/orchestrate/jsch/v1\"></script>"
    ));
  }

  #[test]
  fn is_cloudflare_challenge_ignores_other_responses() {
    // A plain 403 (e.g. a permission error) must stay a generic HTTP error.
    assert!(!is_cloudflare_challenge(
      &StatusCode::FORBIDDEN,
      "<html><body>You are not allowed to view this page.</body></html>"
    ));
    // The markers only count together with 403.
    assert!(!is_cloudflare_challenge(&StatusCode::OK, "Just a moment"));
    assert!(!is_cloudflare_challenge(
      &StatusCode::INTERNAL_SERVER_ERROR,
      "cf_chl"
    ));
  }

  // ── Shelf / edition / journal parser fixtures ──────────────────────────────
  //
  // EDITIONS_FIXTURE, SHELF_FIXTURE and JOURNAL_FIXTURE are real trimmed
  // snapshots captured 2026-10-19 against the live site and anonymized (titles,
  // authors, the user slug, and opaque UUIDs replaced with placeholders; class
  // names and nesting are verbatim). The shelf and journal pages sit behind a
  // login, so these replace the earlier synthetic fixtures and now act as drift
  // guards. JOURNAL_NOTE_QUOTE_FIXTURE is the one remaining synthetic fixture:
  // the test account has no journal notes or quotes to capture, so that parser
  // path is still locked by a hand-built snippet.

  const SHELF_FIXTURE: &str = r#"
<div class="read-books">
  <span class="read-books-panes">
    <div class="max-w-3xl mb-4 md:mb-6 text-darkestGrey dark:text-grey book-pane break-words" id="book_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa" data-book-id=aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa data-author-ids=11111111-1111-1111-1111-111111111111>
      <div class="book-cover"><img alt="Book Title 1 by Author 1" class="rounded-sm shadow-lg dark:shadow-darkerGrey/40" src="https://cdn.thestorygraph.com/fixture-cover-1" /></div>
      <h3 class="font-bold text-xl"><span class="text-red-700 dark:text-red-200"></span><a href="/books/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa">Book Title 1</a></h3>
      <a class="hover:text-cyan-700, dark:hover:text-cyan-500" href="/authors/11111111-1111-1111-1111-111111111111">Author 1</a>
      <div class="progress-bar cursor-pointer edit-progress overflow-hidden h-5 w-9 text-xs flex rounded-md bg-[#EBEBEB] dark:bg-[#4C4C4C]" data-book-id=aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa>
  <div class="relative h-full bg-[#1FB784]" style="width: 57%">
    <span class="absolute text-[11px] ml-0.5 font-semibold left-1 top-1/2 -translate-y-1/2  text-white">
      57%
    </span>
  </div>
</div>
      <span class="toggle-edition-info-link cursor-pointer transition-colors duration-200 group" data-book-id=aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa>453 pages<span class="text-darkerGrey dark:text-lightGrey"> • </span>paperback<span class="text-darkerGrey dark:text-lightGrey"> • </span>1667
              <svg class="inline ml-0.5 w-3 h-3 transition-all duration-200 edition-chevron group-hover:text-cyan-700 dark:group-hover:text-cyan-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"></path>
              </svg></span>
    </div>
    <div class="max-w-3xl mb-4 md:mb-6 text-darkestGrey dark:text-grey book-pane break-words" id="book_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb" data-book-id=bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb data-author-ids=22222222-2222-2222-2222-222222222222>
      <div class="book-cover"><img alt="Book Title 2 by Author 2" class="rounded-sm shadow-lg dark:shadow-darkerGrey/40" src="https://cdn.thestorygraph.com/fixture-cover-2" /></div>
      <h3 class="font-bold text-xl"><span class="text-red-700 dark:text-red-200"></span><a href="/books/bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb">Book Title 2</a></h3>
      <a class="hover:text-cyan-700, dark:hover:text-cyan-500" href="/authors/22222222-2222-2222-2222-222222222222">Author 2</a>
      <div class="progress-bar cursor-pointer edit-progress overflow-hidden h-5 w-9 text-xs flex rounded-md bg-[#EBEBEB] dark:bg-[#4C4C4C]" data-book-id=bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb>
  <div class="relative h-full bg-[#1FB784]" style="width: 4%">
    <span class="absolute text-[11px] ml-0.5 font-semibold left-1 top-1/2 -translate-y-1/2  text-white">
      4%
    </span>
  </div>
</div>
      <span class="toggle-edition-info-link cursor-pointer transition-colors duration-200 group" data-book-id=bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb>80 pages<span class="text-darkerGrey dark:text-lightGrey"> • </span>digital<span class="text-darkerGrey dark:text-lightGrey"> • </span>1971
              <svg class="inline ml-0.5 w-3 h-3 transition-all duration-200 edition-chevron group-hover:text-cyan-700 dark:group-hover:text-cyan-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"></path>
              </svg></span>
    </div>
  </span>
</div>
<div class="paused-books-panes">
    <div class="max-w-3xl mb-4 md:mb-6 text-darkestGrey dark:text-grey book-pane break-words" id="book_cccccccc-cccc-cccc-cccc-cccccccccccc" data-book-id=cccccccc-cccc-cccc-cccc-cccccccccccc data-author-ids=33333333-3333-3333-3333-333333333333>
      <div class="book-cover"><img alt="Book Title 3 by Author 3" class="rounded-sm shadow-lg dark:shadow-darkerGrey/40" src="https://cdn.thestorygraph.com/fixture-cover-3" /></div>
      <h3 class="font-bold text-xl"><span class="text-red-700 dark:text-red-200"></span><a href="/books/cccccccc-cccc-cccc-cccc-cccccccccccc">Book Title 3</a></h3>
      <a class="hover:text-cyan-700, dark:hover:text-cyan-500" href="/authors/33333333-3333-3333-3333-333333333333">Author 3</a>
      <input value="77" class="read-status-last-reached-percent" data-book-id="cccccccc-cccc-cccc-cccc-cccccccccccc" autocomplete="off" type="hidden" name="read_status[last_reached_percent]" id="read_status_last_reached_percent" />
      <span class="toggle-edition-info-link cursor-pointer transition-colors duration-200 group" data-book-id=cccccccc-cccc-cccc-cccc-cccccccccccc>672 pages<span class="text-darkerGrey dark:text-lightGrey"> • </span>hardcover<span class="text-darkerGrey dark:text-lightGrey"> • </span>2016
              <svg class="inline ml-0.5 w-3 h-3 transition-all duration-200 edition-chevron group-hover:text-cyan-700 dark:group-hover:text-cyan-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"></path>
              </svg></span>
    </div>
</div>
"#;

  #[test]
  fn parse_shelf_section_reads_reading_books() {
    let books = parse_shelf_section(SHELF_FIXTURE, ".read-books", "reading");
    assert_eq!(books.len(), 2);

    let b = &books[0];
    assert_eq!(b.id, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
    assert_eq!(b.title, "Book Title 1");
    assert_eq!(b.author, "Author 1");
    assert_eq!(b.progress_percent, 57);
    assert_eq!(b.pages, 453);
    assert_eq!(b.format, "paperback");
    assert_eq!(b.year, 1667);
    assert_eq!(b.status, "reading");
    assert_eq!(
      b.cover_url.as_deref(),
      Some("https://cdn.thestorygraph.com/fixture-cover-1")
    );

    let b = &books[1];
    assert_eq!(b.id, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
    assert_eq!(b.title, "Book Title 2");
    assert_eq!(b.author, "Author 2");
    assert_eq!(b.progress_percent, 4);
    assert_eq!(b.pages, 80);
    assert_eq!(b.format, "digital");
    assert_eq!(b.year, 1971);
    assert_eq!(b.status, "reading");
  }

  #[test]
  fn parse_shelf_section_paused_book_uses_hidden_percent_input() {
    // Paused books do not render the `edit-progress` bar (that class only
    // exists on the active shelf), so progress must come from the hidden
    // percent input.
    let books = parse_shelf_section(SHELF_FIXTURE, ".paused-books-panes", "paused");
    assert_eq!(books.len(), 1);

    let b = &books[0];
    assert_eq!(b.id, "cccccccc-cccc-cccc-cccc-cccccccccccc");
    assert_eq!(b.title, "Book Title 3");
    assert_eq!(b.author, "Author 3");
    assert_eq!(b.progress_percent, 77);
    assert_eq!(b.pages, 672);
    assert_eq!(b.format, "hardcover");
    assert_eq!(b.year, 2016);
    assert_eq!(b.status, "paused");
  }

  const EDITIONS_FIXTURE: &str = r#"
<div class="max-w-3xl mb-4 md:mb-6 text-darkestGrey dark:text-grey book-pane break-words" id="book_99860520-9842-41bd-866b-4e5f12558d20" data-book-id=99860520-9842-41bd-866b-4e5f12558d20 data-author-ids=2ee3fed7-9bff-4a19-868c-d194ca72ce26>
  <div class="book-pane-content border border-darkGrey dark:border-darkerGrey rounded">
    <div class="book-cover">
      <a href="/books/99860520-9842-41bd-866b-4e5f12558d20">
        <img alt="The Final Empire by Brandon Sanderson" class="rounded-sm shadow-lg dark:shadow-darkerGrey/40" src="https://cdn.thestorygraph.com/s1hm0zy7tchue3u0rquctj5i2vv0" />
      </a>
    </div>
    <div class="book-title-author-and-series">
      <p class="font-semibold tracking-tight hover:text-cyan-700 dark:hover:text-cyan-500 text-sm"><a href="/series/2">Mistborn</a> <a href="/series/2">#1</a></p>
      <h3 class="font-bold text-xl"><span class="text-red-700 dark:text-red-200"></span><a href="/books/99860520-9842-41bd-866b-4e5f12558d20">The Final Empire</a></h3>
      <p class="font-body mb-1 mt-1 md:mt-0 text-sm font-medium">
        <a class="hover:text-cyan-700, dark:hover:text-cyan-500" href="/authors/2ee3fed7-9bff-4a19-868c-d194ca72ce26">Brandon Sanderson</a>
      </p>
    </div>
    <p class='text-xs font-light text-darkestGrey dark:text-grey mt-1'>
      541 pages<span class="text-darkerGrey dark:text-lightGrey"> • </span>hardcover<span class="text-darkerGrey dark:text-lightGrey"> • </span>2006
    </p>
    <div class="hidden edition-info mt-3" data-book-id=99860520-9842-41bd-866b-4e5f12558d20>
      <p class="text-xs"><span class="font-semibold">ISBN/UID:</span> 9780765311788</p>
      <p class="text-xs"><span class="font-semibold">Format:</span> Hardcover</p>
      <p class="text-xs"><span class="font-semibold">Language:</span> English</p>
      <p class="text-xs"><span class="font-semibold">Original Pub Year:</span> 2006</p>
      <p class="text-xs"><span class="font-semibold">Edition Pub Date:</span> 25 Jul 2006</p>
      <p class="text-xs"><span class="font-semibold">Publisher:</span> Tor Books</p>
    </div>
  </div>
</div>
<div class="max-w-3xl mb-4 md:mb-6 text-darkestGrey dark:text-grey book-pane break-words" id="book_730af4eb-cce0-44e6-baee-3cd222b8d3d3" data-book-id=730af4eb-cce0-44e6-baee-3cd222b8d3d3 data-author-ids=2ee3fed7-9bff-4a19-868c-d194ca72ce26>
  <div class="book-pane-content border border-darkGrey dark:border-darkerGrey rounded">
    <div class="book-cover">
      <a href="/books/730af4eb-cce0-44e6-baee-3cd222b8d3d3">
        <img alt="Mistborn by Brandon Sanderson" class="rounded-sm shadow-lg dark:shadow-darkerGrey/40" src="https://cdn.thestorygraph.com/ai3f5ml776l49feuzs1bxe6jiw20" />
      </a>
    </div>
    <div class="book-title-author-and-series">
      <p class="font-semibold tracking-tight hover:text-cyan-700 dark:hover:text-cyan-500 text-sm"><a href="/series/2">Mistborn</a> <a href="/series/2">#1</a></p>
      <h3 class="font-bold text-xl"><span class="text-red-700 dark:text-red-200"></span><a href="/books/730af4eb-cce0-44e6-baee-3cd222b8d3d3">Mistborn</a></h3>
      <p class="font-body mb-1 mt-1 md:mt-0 text-sm font-medium">
        <a class="hover:text-cyan-700, dark:hover:text-cyan-500" href="/authors/2ee3fed7-9bff-4a19-868c-d194ca72ce26">Brandon Sanderson</a>
      </p>
    </div>
    <p class='text-xs font-light text-darkestGrey dark:text-grey mt-1'>
      657 pages<span class="text-darkerGrey dark:text-lightGrey"> • </span>paperback<span class="text-darkerGrey dark:text-lightGrey"> • </span>2006
    </p>
    <div class="hidden edition-info mt-3" data-book-id=730af4eb-cce0-44e6-baee-3cd222b8d3d3>
      <p class="text-xs"><span class="font-semibold">ISBN/UID:</span> 9780765350381</p>
      <p class="text-xs"><span class="font-semibold">Format:</span> Paperback</p>
      <p class="text-xs"><span class="font-semibold">Language:</span> English</p>
      <p class="text-xs"><span class="font-semibold">Original Pub Year:</span> 2006</p>
      <p class="text-xs"><span class="font-semibold">Edition Pub Date:</span> 17 Jul 2006</p>
      <p class="text-xs"><span class="font-semibold">Publisher:</span> Tom Doherty Associates</p>
    </div>
  </div>
</div>
"#;

  #[test]
  fn parse_editions_extracts_fields_from_real_markup() {
    let editions = parse_editions(EDITIONS_FIXTURE);
    assert_eq!(editions.len(), 2);

    let hardcover = &editions[0];
    assert_eq!(hardcover.book_id, "99860520-9842-41bd-866b-4e5f12558d20");
    assert_eq!(hardcover.title, "The Final Empire");
    assert_eq!(hardcover.format, "Hardcover");
    assert_eq!(hardcover.pages, Some(541));
    assert_eq!(hardcover.isbn, "9780765311788");
    assert_eq!(hardcover.language, "English");
    assert_eq!(hardcover.publisher, "Tor Books");
    assert_eq!(hardcover.pub_date, "25 Jul 2006");
    assert_eq!(
      hardcover.cover_url.as_deref(),
      Some("https://cdn.thestorygraph.com/s1hm0zy7tchue3u0rquctj5i2vv0")
    );

    let paperback = &editions[1];
    assert_eq!(paperback.book_id, "730af4eb-cce0-44e6-baee-3cd222b8d3d3");
    assert_eq!(paperback.title, "Mistborn");
    assert_eq!(paperback.format, "Paperback");
    assert_eq!(paperback.pages, Some(657));
    assert_eq!(paperback.isbn, "9780765350381");
    assert_eq!(paperback.publisher, "Tom Doherty Associates");
    assert_eq!(paperback.pub_date, "17 Jul 2006");
  }

  const JOURNAL_FIXTURE: &str = r#"
<span class="journal-entry-panes">
<div class="mb-3 grid grid-cols-4 md:grid-cols-6">
  <div class="col-span-4 md:col-span-4 ml-2 md:ml-2">
  <div class="mb-4">
    <p class="font-semibold text-xs md:text-sm">20 February 2024
    <a class=" pb-4 text-xs md:text-sm standard-link float-right" href="/journal_entries/jjjjjjjj-0000-0000-0000-000000000016/edit?return_to=%2Fjournal%3Fbook_id%3Dcccccccc-cccc-cccc-cccc-cccccccccccc">Edit</a></p>
  </div>
    <div class="mt-2 mb-4">
     <span title="Starting reading this book" class="inline-flex items-center px-2 py-1 rounded text-xs font-semibold bg-blueGrey-300 dark:bg-blueGrey-600 text-white dark:text-darkestGrey">
        Started reading
      </span>
    </div>
      <hr class="text-darkGrey dark:text-darkerGrey mt-6 mb-6 clear-both">
  </div>
</div>
<div class="mb-3 grid grid-cols-4 md:grid-cols-6">
  <div class="col-span-4 md:col-span-4 ml-2 md:ml-2">
  <div class="mb-4">
    <p class="font-semibold text-xs md:text-sm">22 December 2024
    <a class=" pb-4 text-xs md:text-sm standard-link float-right" href="/journal_entries/jjjjjjjj-0000-0000-0000-000000000005/edit?return_to=%2Fjournal%3Fbook_id%3Dcccccccc-cccc-cccc-cccc-cccccccccccc">Edit</a></p>
  </div>
    <div class="mt-2 mb-4">
      <span title="Paused reading of book" class="inline-flex items-center px-2 py-1 rounded text-xs font-semibold bg-[#F6DEAD] dark:bg-[#F1CB7F] text-[#5D503E] dark:text-[#3D3223]">
        Paused
      </span>
        <div class="relative">
          <div class="overflow-hidden h-4 mt-3 mb-3 w-7/12 float-left text-xs flex rounded-md bg-darkGrey dark:bg-darkerGrey">
            <div style="width:26.000%" class="shadow-none flex flex-col whitespace-nowrap text-white dark:text-darkestGrey justify-center bg-teal-500 dark:bg-teal-400"></div>
          </div>
          <div class="inline text-teal-500 dark:text-teal-400 text-xs float-left ml-1 mt-3 font-semibold">26%</div>
        </div>
    </div>
      <hr class="text-darkGrey dark:text-darkerGrey mt-8 mb-4 clear-both">
  </div>
</div>
<div class="mb-3 grid grid-cols-4 md:grid-cols-6">
  <div class="col-span-4 md:col-span-4 ml-2 md:ml-2">
  <div class="mb-4">
    <p class="font-semibold text-xs md:text-sm">20 February 2024
    <a class=" pb-4 text-xs md:text-sm standard-link float-right" href="/journal_entries/jjjjjjjj-0000-0000-0000-000000000017/edit?return_to=%2Fjournal%3Fbook_id%3Dcccccccc-cccc-cccc-cccc-cccccccccccc">Edit</a></p>
  </div>
    <div class="mt-2 mb-4">
      <div class="relative">
        <div class="overflow-hidden h-4 mt-3 mb-3 w-7/12 float-left text-xs flex rounded-md bg-darkGrey dark:bg-darkerGrey">
          <div style="width:4.000%" class="shadow-none flex flex-col whitespace-nowrap text-white dark:text-darkestGrey justify-center bg-teal-500 dark:bg-teal-400"></div>
        </div>
        <div class="inline text-teal-500 dark:text-teal-400 text-xs float-left ml-1 mt-3 font-semibold">4%</div>
      </div>
      <p class="clear-both text-xs text-darkerGrey dark:text-lightGrey mb-2 font-medium">
          27 pages read
          <span class="font-normal">(27 pages out of 672)</span>
      </p>
    </div>
      <hr class="text-darkGrey dark:text-darkerGrey mt-8 mb-4 clear-both">
  </div>
</div>
</span>
"#;

  #[test]
  fn parse_journal_maps_badges_and_progress() {
    let activities = parse_journal(JOURNAL_FIXTURE, 10, 0);
    assert_eq!(activities.len(), 3);

    // "Starting reading this book" badge
    assert_eq!(activities[0].event, "user_book_read_started");
    assert_eq!(activities[0].action_at, "2024-02-20T12:00:00Z");

    // "Paused reading of book" badge
    assert_eq!(activities[1].event, "status_paused");
    assert_eq!(activities[1].action_at, "2024-12-22T12:00:00Z");

    // progress entry reads the width style off the nested bar div
    assert_eq!(activities[2].event, "progress_updated");
    assert_eq!(activities[2].metadata["progress"], json!(4));
    assert_eq!(activities[2].metadata["progress_was"], json!(0));
  }

  #[test]
  fn parse_journal_maps_resumed_badge() {
    // The real fixture only has a paused entry; a resumed one carries the same
    // markup with a different title.
    let html = JOURNAL_FIXTURE.replace("Paused reading of book", "Resumed reading of book");
    let activities = parse_journal(&html, 10, 0);
    assert_eq!(activities[1].event, "status_resumed");
  }

  #[test]
  fn parse_journal_applies_limit_and_offset() {
    let activities = parse_journal(JOURNAL_FIXTURE, 2, 1);
    assert_eq!(activities.len(), 2);
    assert_eq!(activities[0].event, "status_paused"); // first entry (started) skipped
    assert_eq!(activities[1].event, "progress_updated");
  }

  // Synthetic: the test account has no journal notes or quotes to capture, so
  // the note/quote parsing path is still locked against this hand-built snippet.
  const JOURNAL_NOTE_QUOTE_FIXTURE: &str = r#"
<span class="journal-entry-panes">
  <div>
    <p class="font-semibold">19 October 2026</p>
    <div class="trix-content"><div>Great world-building.</div></div>
  </div>
  <div>
    <p class="font-semibold">20 October 2026</p>
    <div class="trix-content"><blockquote>Honor is dead.</blockquote></div>
  </div>
</span>
"#;

  #[test]
  fn parse_journal_maps_notes_and_quotes() {
    let activities = parse_journal(JOURNAL_NOTE_QUOTE_FIXTURE, 10, 0);
    assert_eq!(activities.len(), 2);
    assert_eq!(activities[0].event, "note");
    assert_eq!(activities[0].entry, "Great world-building.");
    assert_eq!(activities[1].event, "quote");
    assert_eq!(activities[1].entry, "Honor is dead.");
  }
}
