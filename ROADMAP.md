# Roadmap

NickelStorygraph has no public API — it scrapes the StoryGraph website, so any
UI change can silently break it. Priorities therefore lean toward robustness
over new features.

Items are grouped by effort vs. value: **quick wins** (ship fast), **high
priority** (highest value, worth the effort), and **low priority (optional)**
(nice-to-have, safe to defer).

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
- [x] Update check in the home menu (queries GitHub releases, compares against the git-describe version)

## Quick wins

Low effort, clear value — shippable in a single sitting.

### Started / finished dates
Status changes set "started"/"finished"; StoryGraph also tracks the concrete
dates. Sync those instead of only flipping the status.

- **Note:** verify exactly what StoryGraph expects before implementing.
- **Effort:** small.

### More metadata in the link dialog
The manual-link search shows only title/format. Adding page count, rating, and
series would reduce wrong-edition links.

- **Effort:** small.

### Firmware self-check
The `calloc` object-size hack is 5.18-specific. A boot check that validates the
plugin against the firmware version and shows a clear message (instead of a
silent heap crash) on mismatch.

- **Effort:** small.

## High priority

Higher effort, highest value — the product's health and the most painful UX.

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

### On-device OTA update
Update the plugin directly on the device. The install is split across two
filesystems, so "OTA" is inherently two-phase:
- CLI + config.ini live on /mnt/onboard (writable) — replace in place,
  preserving config.ini (it holds the session cookie).
- The hook .so lives on the read-only rootfs — stage the new Kobo.tgz to
  .kobo/ and let firmware 5.18+ extract it on next boot, then prompt a reboot.

- **Prereq:** the update check in the home menu (Story 1); the OTA flow reuses
  its release lookup and version comparison.
- **Why:** the manual copy-reboot-install dance is why users fall behind on
  versions; OTA removes it.
- **Blocker:** no rollback — a broken .so leaves the plugin dead until a manual
  fix. Back up the old .so (e.g. libstorygraph.so.bak on onboard) before
  staging as a minimal safety net.
- **Effort:** medium-high. Ship in two steps: (1) CLI/config in-place update
  (no reboot), (2) Kobo.tgz staging + reboot + post-boot version verification.

## Low priority (optional)

Nice-to-have, not essential — safe to defer.

### Reading challenges *(optional)*
Show the user's active StoryGraph reading challenges and their progress, from
the dashboard (`/reading_challenges/dashboard/<slug>`). This covers the official
yearly challenge ("N books this year") and any community challenges the user
joined.

- **Why:** the page is authenticated and scrapable, and the slug is already
  cached — the plugin only needs a new CLI command + a home-screen panel
  (like the existing Goals view).
- **Scope:** start with the overview (challenge name + books-read/goal
  progress); per-prompt tracking (which books fulfil which prompts) is a
  follow-up.
- **Effort:** medium.

### Tolino smoke tests
The 5.18 firmware runs on Tolinos too; installation paths and Tolino-specific
behaviour are the least-tested path.

- **Effort:** small–medium.
