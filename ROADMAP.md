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
- [x] Session-expiry detection: a redirect to `/users/sign_in` becomes a "session expired" message in `get`/`post_form`/`post_form_browser`
- [x] Cloudflare bot challenge (403 "Just a moment…") mapped to an actionable cookie message instead of "unexpected error"
- [x] Link-dialog metadata: pages, release year, series (`name` + `position`) and cover URL scraped from `/browse` and rendered by the existing `BookRow`
- [x] Audiobook results filtered via the edition line (the old `format: audio` check never matched on `/browse`)
- [x] Double-encoded bullet/star in `BookRow::getMeta` replaced with `QChar` code points

## Quick wins

Low effort, clear value — shippable in a single sitting.

### Search result paging
`search` accepts `--page` but ignores it ("kept for hook compatibility"), while
the hook computes rows-per-page and passes a page number — so the link dialog
only ever shows the first screenful and never offers "next". The endpoint is
verified: `/browse?search_term=X&page=2` returns the next page and the result
HTML carries the next-page link.

- **Wrinkle found while scoping:** a StoryGraph page holds ~10 results, a dialog
  screen holds `limit` rows (height-dependent, ~4–6). Screen pages therefore do
  not map 1:1 onto `&page=N`; a row offset is needed (fetch the StoryGraph page
  covering the offset, then slice), or the screen itself must scroll.
- **Scope:** fix the offset math in the CLI and report whether more rows exist
  after the slice; the hook turns that into `PagedStack::setTotal`.
- **Effort:** small–medium (the mapping, not the plumbing, is the work).

### Firmware self-check (log + skip, no UI)
The `calloc` object-size hack is 5.18-specific. Read the firmware version at
init and, on a mismatch, log it and skip hooking instead of corrupting the heap.
Getting the version (`.kobo/version`, or the exported
`N3SettingsDeviceInformationController::viewFirmwareVersion`) is trivial; a
*visible* message is the expensive part (UI during boot) and stays a follow-up.

- **Blocker:** needs a device to verify. Keep the change to "no hooking" so a
  wrong guess cannot break anything.
- **Effort:** small.

## High priority

Higher effort, highest value — the product's health and the most painful UX.

### Fixture-based parser tests
Capture real StoryGraph HTML snapshots and assert the parsing selectors
(shelf scraping, edition + journal parsing) against them. `search` now shows the
pattern to copy: a pure `parse_search_results(&str)` plus a trimmed
`BROWSE_FIXTURE` snippet in the unit tests. Extending it to the remaining
parsers makes a StoryGraph UI change break CI, not a user's device.

- **Why:** the highest-leverage guard against the core fragility.
- **Blocker:** needs logged-in HTML snapshots as committed test fixtures (the
  public `/browse` page is reachable without a session, shelf/edition/journal
  pages are not).
- **Effort:** medium.

### Started / finished dates
Status changes set "started"/"finished"; StoryGraph also tracks the concrete
dates. `update-progress-with-note` already posts
`progress_update_date[day|month|year]`, so the field convention exists — only
`/update-status.js` still needs verifying against the book page's own form.

- **Note:** the hook knows only "now", so the feature is "date = today"; verify
  the parameter names first.
- **Effort:** small, but blocked on verification with a live session.

### Cookie import (login friction)
The direct HTTP login is blocked by Cloudflare and stays dead, so cookies come
from the browser. A "paste the cookie string" dialog is *not* viable as
previously described: libnickel 5.18 exports no `QClipboard`, so there is no
paste source on the device — the string would have to be typed on an e-ink
keyboard.

- **Scope:** import a cookie header (`name=value; name=value`) from a file in
  `.adds/NickelStorygraph/`, or a desktop helper that writes `config.ini`.
  Needs a UX decision before code.
- **Why:** the most painful point in the whole flow.
- **Effort:** small for the file import, medium for the desktop helper.

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

### Reading challenges: per-prompt tracking *(optional)*
The challenge overview already ships: `get-goals` reads
`/reading_challenges/dashboard/<slug>` and the Goals dialog renders it. What is
left is per-prompt tracking (which books fulfil which prompts).

- **Why:** the page is authenticated and scrapable, and the slug is cached.
- **Scope:** per-prompt tracking only — do not rebuild the overview.
- **Effort:** medium.

### Tolino smoke tests
The 5.18 firmware runs on Tolinos too; installation paths and Tolino-specific
behaviour are the least-tested path. `SMOKE-TEST.md` covers the Qt6 port but is
Kobo-centric.

- **Effort:** small–medium.
