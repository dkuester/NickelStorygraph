// Update checking: queries the GitHub releases API for the latest release and
// compares it to the running version.
//
// This is deliberately separate from the StoryGraph client (storygraph.rs):
// the update check hits api.github.com unauthenticated and must not send the
// StoryGraph session cookies there.

use std::sync::LazyLock;

use anyhow::{Context, Result};
use reqwest::Certificate;
use reqwest::blocking::Client;
use serde_json::json;

use crate::utils::{UserError, VERSION};

const GITHUB_API: &str = "https://api.github.com";

// The repository this plugin is distributed from. Releases are published only
// on this fork, so update checks query it for the latest release.
const UPDATE_REPO: &str = "dkuester/NickelStorygraph";

const USER_AGENT: &str = concat!("NickelStorygraph/", env!("CARGO_PKG_VERSION"));

fn build_client() -> Result<Client, reqwest::Error> {
  Client::builder()
    .user_agent(USER_AGENT)
    .tls_certs_only(
      webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|cert| Certificate::from_der(cert))
        .collect::<Result<Vec<_>, _>>()?,
    )
    .build()
}

static CLIENT: LazyLock<Result<Client, reqwest::Error>> = LazyLock::new(build_client);

fn client() -> Result<&'static Client> {
  CLIENT.as_ref().context("Failed to construct HTTP client")
}

/// A three-part semantic version (major.minor.patch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
  major: u32,
  minor: u32,
  patch: u32,
}

/// Parses a version string into its numeric parts. Accepts the forms the
/// project actually produces: "0.3.1" (Cargo), "v0.3.1" (a release tag) and
/// "v0.3.1-1-gdc7bc74" / "v0.3.1-dirty" (git describe).
fn parse_version(s: &str) -> Option<Version> {
  let s = s.trim();
  let s = s.strip_prefix('v').unwrap_or(s);
  let s = s.split('-').next().unwrap_or(s);

  let mut parts = s.split('.');
  let major = parts.next()?.parse().ok()?;
  let minor = parts.next()?.parse().ok()?;
  let patch = parts.next().unwrap_or("0").parse().ok()?;

  Some(Version {
    major,
    minor,
    patch,
  })
}

fn format_version(v: Version) -> String {
  format!("v{}.{}.{}", v.major, v.minor, v.patch)
}

/// Queries GitHub for the latest release and compares it to the running
/// version. Returns a JSON object the hook renders into a result dialog.
pub fn check_update() -> Result<serde_json::Value> {
  let current = parse_version(*VERSION)
    .ok_or_else(|| UserError(format!("Could not parse current version '{}'", *VERSION)))?;

  let url = format!("{GITHUB_API}/repos/{UPDATE_REPO}/releases");
  let response = client()?
    .get(&url)
    .send()
    .context("Failed to check for updates")?;

  let status = response.status();
  if !status.is_success() {
    return Err(UserError(format!("Update check failed (HTTP {status})")).into());
  }

  let releases: serde_json::Value = response.json().context("Failed to parse update response")?;

  // All releases are candidates, including pre-releases — this project only
  // ships pre-releases so far, so filtering them out would never find an
  // update. Pick the highest version tag.
  let mut latest_ver: Option<Version> = None;
  let mut latest_tag: Option<&str> = None;
  let mut latest_url: Option<&str> = None;

  if let Some(array) = releases.as_array() {
    for release in array {
      let Some(tag) = release.get("tag_name").and_then(|v| v.as_str()) else {
        continue;
      };
      let Some(ver) = parse_version(tag) else {
        continue;
      };
      if latest_ver.is_none_or(|lv| ver > lv) {
        latest_ver = Some(ver);
        latest_tag = Some(tag);
        latest_url = release.get("html_url").and_then(|v| v.as_str());
      }
    }
  }

  let Some(latest_ver) = latest_ver else {
    return Err(UserError("No releases found".to_owned()).into());
  };

  Ok(json!({
    "up_to_date": current >= latest_ver,
    "current": format_version(current),
    "latest": latest_tag.unwrap_or(""),
    "url": latest_url.unwrap_or(""),
  }))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_release_tag() {
    assert_eq!(
      parse_version("v0.3.1"),
      Some(Version {
        major: 0,
        minor: 3,
        patch: 1
      })
    );
  }

  #[test]
  fn parses_cargo_version_without_v() {
    assert_eq!(
      parse_version("0.3.1"),
      Some(Version {
        major: 0,
        minor: 3,
        patch: 1
      })
    );
  }

  #[test]
  fn parses_git_describe_suffix() {
    assert_eq!(
      parse_version("v0.3.1-1-gdc7bc74"),
      Some(Version {
        major: 0,
        minor: 3,
        patch: 1
      })
    );
    assert_eq!(
      parse_version("v0.3.1-dirty"),
      Some(Version {
        major: 0,
        minor: 3,
        patch: 1
      })
    );
  }

  #[test]
  fn parses_two_part_version() {
    assert_eq!(
      parse_version("v1.2"),
      Some(Version {
        major: 1,
        minor: 2,
        patch: 0
      })
    );
  }

  #[test]
  fn rejects_garbage() {
    assert_eq!(parse_version(""), None);
    assert_eq!(parse_version("not-a-version"), None);
    assert_eq!(parse_version("v"), None);
  }

  #[test]
  fn orders_versions_semantically() {
    let a = parse_version("v0.3.1").unwrap();
    let b = parse_version("v0.3.2").unwrap();
    let c = parse_version("v0.10.0").unwrap();
    let d = parse_version("v1.0.0").unwrap();
    assert!(b > a);
    assert!(c > a);
    assert!(d > c);
  }
}
