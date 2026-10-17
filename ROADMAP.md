# Roadmap

NickelStorygraph has no public API — it scrapes the StoryGraph website, so any
UI change can silently break it. Priorities therefore lean toward robustness
over new features.

## Recently completed

- [x] Password over stdin (never in the process argv)
- [x] stdout/stderr protocol: stdout carries a single JSON document, no `BEGIN_JSON` sentinel
- [x] Expected errors via `Result`/`UserError` instead of `panic!`
- [x] Deduplicated StoryGraph progress parsing (`percent_from_style`)
- [x] `ReadingStatus` enum + reading-format constants (Rust + `storygraph_status.h`)
- [x] Unit tests for pure functions; committed `Cargo.lock`
- [x] clippy + rustfmt clean (`rustfmt.toml`, `just lint` wired into CI)
- [x] Config INI robustness (line-based `[General]` stripping, not global replace)
- [x] Error propagation in `update` (ISBN/search errors no longer swallowed)
- [x] German + abbreviated month names; HTML-escaped review text
- [x] Slug auto-caching, magic-static singletons, `calloc`-hack documented

## Priority 1 — Robustness

The scraper is the whole product. These make StoryGraph UI changes fail loudly
(in CI) or fail clearly (on device) instead of silently.

### Fixture-based parser tests
Capture real StoryGraph HTML snapshots and assert the parsing selectors
(shelf scraping, edition + journal parsing) against them. A StoryGraph UI
change then breaks CI, not a user's device.

- **Why:** the highest-leverage guard against the core fragility; parsing has
  no tests today (only pure functions do).
- **Blocker:** needs logged-in HTML snapshots as committed test fixtures.
- **Effort:** medium.

### Session-expiry detection
Detect an expired session (redirect to `/users/sign_in`, or the Cloudflare
"Just a moment…" page) and surface a clear "session expired — log in again"
prompt instead of a generic credentials error.

- **Why:** cookie expiry is the most common support issue.
- **Effort:** small–medium.

### Login friction (Cloudflare)
The direct HTTP login is blocked by Cloudflare; today the workaround is copying
cookies out of the browser by hand. Options: a paste-the-cookie-string dialog
(instead of two separate fields) or a small desktop helper that reads the
cookies and writes them to the device.

- **Why:** the most painful point in the whole flow.
- **Effort:** medium.

## Priority 2 — Features

### Annual reading challenge
StoryGraph has an "N books this year" challenge. The plugin already shows goals
and the streak; showing challenge progress is consistent with that.

- **Effort:** small–medium.

### Started / finished dates
Status changes set "started"/"finished"; StoryGraph also tracks the concrete
dates. Sync those instead of only flipping the status.

- **Note:** verify exactly what StoryGraph expects before implementing.
- **Effort:** small.

### More metadata in the link dialog
The manual-link search shows only title/format. Adding page count, rating, and
series would reduce wrong-edition links.

- **Effort:** small.

## Priority 3 — Maintenance / ops

### Firmware self-check
The `calloc` object-size hack is 5.18-specific. A boot check that validates the
plugin against the firmware version and shows a clear message (instead of a
silent heap crash) on mismatch.

- **Effort:** small.

### Tolino smoke tests
The 5.18 firmware runs on Tolinos too; installation paths and Tolino-specific
behaviour are the least-tested path.

- **Effort:** small–medium.
