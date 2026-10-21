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
- [x] Search text without the series: Kobo titles like "Good Boy (The Northern Weird Project)" made both auto-link and the prefilled manual search return nothing; bracketed segments are now dropped (found in the Tolino device test)
- [x] Journal: "Paused reading of book" / "Resumed reading of book" badges get their own events instead of showing as "Marked as Read"
- [x] Expired login is a user-facing message (it used to surface as "unexpected error, please report this"), and it and the Cloudflare message point to the cookie import; README covers `cookies.ini` and the real log location
- [x] "Log in" row removed from Settings: email/password login always fails behind Cloudflare's bot check; the dialog code stays for when that changes
- [x] Shop books link again: the Tolino shop leaves Nickel's `ISBN` column empty (0 of 362 shop books in a real library), and a missing ISBN aborted auto-link before the title search. The ISBN is now read from the shop's cover URL (`…/9781068563119_A62207078/cover`, 350 of 362 with a valid check digit), with the title search as fallback (found in the Tolino device test)
- [x] The in-book "Manually link book" entry also uses the bracket-free search text (missed in the first fix)
- [x] "Sync all books" in the home menu: syncs every book whose progress in `KoboReader.sqlite` moved forward since its last sync — books read in Kobalt or offline (e.g. on a train without Wi-Fi). Previews the titles and asks first. Only linked books by default; with `sync_all_link_new = true` it also links books Nickel shows as currently reading. Sideloaded books always; store books per `sync_all_store_books` (default off on Kobo, which syncs store books natively, on on Tolino)
- [x] Upstream sync (Oct 2026): private books, paused-audiobook progress, fixed 4-book paging in Currently Reading, wrapped feed titles, the `nickelstoregraph` → `nickelstorygraph` rename (the CLI log is now `nickelstorygraph.log`) and NickelHardcover coexistence, the last adapted to the single Qt 6 `ReadingMenuView` constructor. NickelHardcover itself doesn't support firmware 5.x yet, so that path is untested on device
- [x] Release workflow: a pushed `v*` tag builds and tests in the toolchain image and publishes the pre-release with `Kobo.tgz` and the `.adds` zip; notes live in `.github/release-notes/<tag>.md`
- [x] Hook rebuilds when `git describe` changes (version stamp), so a local build never ships a stale version string
- [x] `SMOKE-TEST.md` covers the Tolino: `.kobo/version` format, `.txt`→EPUB conversion, shop-book ISBNs, log locations, USB hang

## Quick wins

Low effort, clear value — shippable in a single sitting.

### Warn before linking a store book on Kobo
On a Kobo, store books already sync with StoryGraph natively. Syncing or
(manually) linking one through the plugin as well risks double updates, so the
in-book "Sync now", "Update book status" and "Manually link book" actions
should first ask "This is a store book Kobo already syncs — link it anyway?".

- **Scope:** store book = content ID without `file://`; reuse
  `sync_all_store_books` (when it's false, warn). No warning on Tolino.
- **Effort:** small.

## High priority

Higher effort, highest value — the product's health and the most painful UX.

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

### Manual mapping of sideloaded books
As a user I want to map all sideloaded books that have reading progress to
StoryGraph by hand, in one list — for the books "Sync all books" can't link
automatically (no ISBN, ambiguous title) and as a review of what is linked.

- **Scope:** a dialog listing sideloaded books with progress and their link
  state; tapping one opens the existing search/edition picker.
- **Effort:** medium (UI).

## Low priority (optional)

Nice-to-have, not essential — safe to defer.

### Started / finished dates: manual correction *(optional)*
Clarified, but low value — deferred. StoryGraph already creates the
"Started reading" / "Finished reading" journal entries with today's date when
the status changes, so setting "date = today" from the device adds nothing.
The only useful version would set the *real* date, and the Tolino doesn't
record one: `LastTimeStartedReading`, `LastTimeFinishedReading`,
`FirstTimeReading` and `TimesStartedReading` are empty for all 364 books on a
5.18 Tolino, and the `Event` table holds no history (a Kobo may fill these —
unverified). Dates can always be fixed on the website.

What remains is a manual correction in the on-device journal (tap a
started/finished entry, adjust the date with Nickel's date-up/down arrows,
already in `files.h`). The request is known, captured from the website:

- Form page: `GET /edit-journal-entry-from-book?book_id=<id>&journal_entry_id=<id>`
  (logged in) — read `authenticity_token` from it, like the review form.
- Save: `POST /update-journal-entry-from-book`, form-encoded:
  `_method=patch`, `authenticity_token`, `journal_entry[day]`,
  `journal_entry[month]`, `journal_entry[year]` (no zero padding, e.g. `7`),
  `book_id`, `journal_entry_id`, `commit=Update`. On success it redirects to
  `/journal?book_id=<id>`.
- `parse_journal` would need to keep each entry's `journal_entry_id` (it
  currently only reads the badge, date and progress).

- **Why:** fixes dates when a book was linked days after starting it, or
  finished offline and synced later.
- **Effort:** medium (mostly the date-picker UI); verify against a real
  session before shipping.

### Reading challenges: per-prompt tracking *(optional, needs official API)*
The challenge overview already ships: `get-goals` reads
`/reading_challenges/dashboard/<slug>` and the Goals dialog renders it. What is
left is per-prompt tracking (which books fulfil which prompts).

- **Why:** the page is authenticated and scrapable, and the slug is cached.
- **Scope:** per-prompt tracking only — do not rebuild the overview.
- **Blocker:** wait for an official StoryGraph API. Per-prompt tracking would
  scrape a deep, frequently changing part of the site, and a breakage would
  stay silent; that fragility isn't worth it for an optional feature. Pick
  this up only once StoryGraph publishes an API that exposes challenges.
- **Effort:** medium, once the API exists.

### Kobalt compatibility *(optional)*
[Kobalt](https://github.com/nicoverbruggen/kobalt) is an alternative EPUB
reader (CREngine) that runs inside Nickel (Qt 5 build for 4.38+, Qt 6 build for
5.18+) and can be set as the default reader. Books it opens bypass Nickel's
reading stack, which is exactly where the plugin hooks in:
`ReadingController::setVolume` (book open, auto-link, sync triggers),
`ReadingMenuView` (in-book StoryGraph menu) and
`ReadingView::getCalculatedReadProgress` (live progress). DRM books, PDFs and
other unsupported formats are handed back to Nickel's reader and sync as usual.

**Verified on a Kobo Clara BW (4.46.23836) with Kobalt installed:**
- Coexistence works: both mods load, the home bar and Nickel's reader are
  unaffected, and Kobalt-opened books get no StoryGraph menu (as expected).
- **Kobalt writes reading progress back to Nickel's database.** Reading ~30
  pages of a DRM-free shop book in Kobalt moved `content.___PercentRead` from 1
  to 2, advanced `ChapterIDBookmarked`, updated `DateLastRead` and
  `TimeSpentReading`; Kobalt's own per-book state carries a `nickel_progress`
  field. So no Kobalt fork is needed for syncing.

- **Done:** "Sync all books" (home menu) syncs Kobalt-read books from the
  database on demand.
- **Left:** run it automatically (on Wi-Fi connect or at the daily-sync hour),
  and an in-book StoryGraph menu inside Kobalt, which needs an extension point
  in Kobalt itself — propose it upstream to Kobalt rather than forking.
- **Effort:** small for the automatic trigger.

