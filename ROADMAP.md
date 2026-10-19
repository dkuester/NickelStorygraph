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
- [x] Search paging: offset math in the CLI (`search_paged` reports `has_more`) and the hook maps it to `PagedStack::setTotal`; screen pages no longer map 1:1 onto `&page=N`
- [x] Firmware self-check: read `.kobo/version` at init; refuse pre-5.18 (Qt5 ABI), accept 5.18, and hook 5.19+/6.x with a warning log; widget allocations oversized to a uniform 2048 bytes as layout headroom
- [x] Parser fixtures: real trimmed + anonymized snapshots for `parse_editions`, `parse_shelf_section` (both the `edit-progress` bar and the paused hidden-input path) and `parse_journal` (started/paused/progress badges); the note/quote path stays behind a synthetic fixture (the test account has no notes/quotes to capture)
- [x] Profile-URL fix: `get_user()` scraped `/users/:slug`, but StoryGraph moved profiles to `/profile/:slug` — would have broken sync on any device without a cached slug (caught by the live-session probe)
- [x] Cookie import: `import-cookies` CLI subcommand merges a `.adds/NickelStorygraph/cookies.ini` into `config.ini` line-by-line (no `[General]` header) and deletes the file afterwards; settings dialog got an "Import cookies" row. Not `.txt`: the firmware converts any onboard `.txt` into an EPUB and lists it in the library, exposing the token as a "book" (found in the Tolino device test)
- [x] Firmware check reads the third field of `.kobo/version` (`serial,kernel,firmware,...`); the first cut parsed the serial number and parked the plugin on every device (found in the Tolino device test)
- [x] Verified with a live session: a valid `remember_user_token` re-auths a stale/blank `session_cookie` transparently, so no session write-back is needed — only the remember token has to stay fresh

## Quick wins

Low effort, clear value — shippable in a single sitting.

_None open — both quick wins shipped; see "Recently completed" above._

## High priority

Higher effort, highest value — the product's health and the most painful UX.

### Started / finished dates
Status changes set "started"/"finished"; StoryGraph also tracks the concrete
dates. `update-progress-with-note` already posts
`progress_update_date[day|month|year]`, so the field convention exists — only
`/update-status.js` still needs verifying against the book page's own form.

- **Note:** the hook knows only "now", so the feature is "date = today"; verify
  the parameter names first.
- **Effort:** small, but blocked on verification with a live session.

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

### Kobalt compatibility *(optional)*
[Kobalt](https://github.com/nicoverbruggen/kobalt) is an alternative EPUB
reader (CREngine) that runs inside Nickel on firmware 5.18+/6.0 and can be set
as the default reader. Books it opens bypass Nickel's reading stack, which is
exactly where the plugin hooks in: `ReadingController::setVolume` (book open,
auto-link, sync triggers), `ReadingMenuView` (in-book StoryGraph menu) and
`ReadingView::getCalculatedReadProgress` (live progress). Expect no auto-sync
and no in-book menu for Kobalt-opened books; the home-screen menu should be
unaffected.

- **Why:** Kobalt is maintained by the author of the NickelHook Qt 6 port this
  fork builds on and is likely to gain users on 5.18+ — silent "sync stopped
  working" reports would land here.
- **Scope:** (1) coexistence first: both plugins load, NickelHook hooks don't
  collide, Kobo's reader still syncs for books Kobalt hands back (DRM, PDF);
  (2) then a Kobalt-aware sync path — Kobalt leaves reading progress to Nickel,
  so read it from `KoboReader.sqlite` (`content.___PercentRead`) instead of the
  live `ReadingView`, triggered from the home menu / daily sync or on book close.
- **Blocker:** Kobalt is not Tolino-ready yet (it is on its roadmap). Until
  then, verification needs a Kobo on 5.18+, or waits for Kobalt's Tolino
  release. Also verify that Kobalt really writes `___PercentRead` back.
- **Effort:** small for the coexistence check (device test only); medium for
  the sqlite-based sync path.

### Tolino smoke tests
The 5.18 firmware runs on Tolinos too; installation paths and Tolino-specific
behaviour are the least-tested path. `SMOKE-TEST.md` covers the Qt6 port but is
Kobo-centric.

- **Effort:** small–medium.
