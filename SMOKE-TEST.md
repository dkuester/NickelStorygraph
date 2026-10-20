# Smoke Test — NickelStorygraph (firmware 4.x and 5.18+)

Manual test checklist for verifying a build on a real device. It applies to
both firmware lines; where they differ, the step says so (4.x = `QT=5` build,
5.18+ = `QT=6` build). Work through
every item in order; for any failure, capture the `logread` output (section 8)
before continuing.

## 0. Prerequisites

- [ ] A device on the firmware line of the build: **4.x** (Kobo) for `QT=5`,
      **5.18.x** or 6.0 (Kobo/Tolino) for `QT=6`.
- [ ] Built artifacts: `hook/Kobo.tgz` and `mnt/onboard/.adds/NickelStorygraph/`
      (from `just build-package`; run `make clean` in `hook/` first if you
      build outside CI), or the two assets of a release.
- [ ] A backup of `.kobo/` and `.adds/` (or a recent full-device backup).
- [ ] StoryGraph cookies ready (`_storygraph_session`, `remember_user_token`).

## 1. Install

**4.x:** copy `KoboRoot.tgz` into `.kobo/`, then safely eject — the device
installs hook and `.adds` in one go and restarts. Continue with section 2.

**5.18+:**

- [ ] Connect the device via USB.
- [ ] Copy `mnt/onboard/.adds/NickelStorygraph/` into `.adds/` **first**, so
      that you get `.adds/NickelStorygraph/`. If that folder is missing when the
      plugin starts, it uninstalls itself.
- [ ] Then copy `hook/Kobo.tgz` into the device root's `.kobo/` folder.
- [ ] Safely eject — the device installs the plugin and restarts.

## 2. First boot — plugin loads (NickelHook Failsafe)

- [ ] After the restart, reconnect via USB.
- [ ] `.adds/NickelStorygraph/` still exists and contains `cli` and
      `config_example.ini` (it was not deleted).
- [ ] No `NickelStorygraph_uninstall` file appeared (nothing self-uninstalled).
- [ ] Nickel still boots normally (no brick, no boot loop).
- [ ] The log (section 8) shows `firmware check: version '…' -> supported`
      and no `refusing to hook` / crash lines. A plugin refused by the
      firmware check is parked as `/usr/local/Kobo/libstorygraph.so.failsafe`;
      installing a fixed `Kobo.tgz` replaces it.

## 3. Configuration

- [ ] Settings → **Import cookies** without a `cookies.ini` shows
      "No cookies file found".
- [ ] Create `.adds/NickelStorygraph/cookies.ini` with `session_cookie=…` and
      `remember_user_token=…` lines (never `.txt`, see section 9), eject, then
      **Import cookies** again: success message, `cookies.ini` is gone, and
      `config.ini` holds both values.
- [ ] Home → Currently Reading loads (no "login has expired" message), and
      `user_slug` gets cached in `config.ini`.

## 4. ABI switch points — verify explicitly

The hook switches these on the Qt version. On 5.18+ verify the Qt6 forms below;
on 4.x the same checks cover the Qt5 forms (`Content::getId()` without
argument, and the two `ReadingMenuView` constructors — the menu entry must
appear in both kinds of reading menu).

### 4a. `Content::getId(QString())` — book identification

The Qt6 ABI change added a `QString const&` argument to `Content::getId`; the
hook passes an empty `QString()`. Verify the book is still identified correctly:

- [ ] Open a book that is already linked to a StoryGraph edition.
- [ ] Check **Settings → Book information**: the detected title/author/ID look
      correct (not empty or garbage).
- [ ] Reading progress syncs to `app.thestorygraph.com` (check the website).
- [ ] If instead you get "book not found" / a mismatched edition, the empty
      `QString` is not the right argument — capture `logread` and report.

### 4b. `ReadingMenuView(QWidget*, bool, bool)` — reading-menu injection

The Qt6 ABI change merged the two ReadingMenuView constructors into one
`(QWidget*, bool, bool)`. Verify the menu still appears:

- [ ] Open a book (the reading screen).
- [ ] Open the reading menu — a **StoryGraph** entry is present.
- [ ] The entry opens the StoryGraph actions without crashing.

## 5. Functional checks

### Home screen
- [ ] The StoryGraph tab-bar button appears (next to NickelMenu if installed).
- [ ] Tapping it opens the StoryGraph home panel.

### Sync
- [ ] Reading progress updates on StoryGraph on book close / threshold.
- [ ] A highlight or note syncs as a journal entry (if `sync_bookmarks` on).
- [ ] Manual sync from the menu works.

### Linking
- [ ] Manually link book: covers, page count and series show; paging to
      pages 2 and 3 works (search for something common).
- [ ] A title with a series in parentheses is prefilled without it.
- [ ] A shop book (no ISBN in the database) auto-links; the CLI log shows
      `ISBN from cover URL …`.

### Book status
- [ ] **Private: No/Yes** toggles, matches the website, and shows the
      eye-slash icon in Currently Reading.
- [ ] Setting **Read** asks for a review exactly once.

### Other UI
- [ ] Journal, reviews, goals, feed, editions and settings dialogs all open
      without crashing; Currently Reading pages 4 books per tab; long feed
      titles wrap.
- [ ] Home menu → update check reports the installed version.

## 6. NickelMenu coexistence

- [ ] With NickelMenu installed, both buttons render in the home tab bar.
- [ ] Neither menu is missing; no duplicate/overlapping icon.

## 7. Uninstall

- [ ] Create an empty file `NickelStorygraph_uninstall` in the device root.
- [ ] Safely eject — the plugin removes itself and the file is deleted.
- [ ] Alternatively delete `.adds/NickelStorygraph/` and restart.

## 8. Collecting logs for a bug report

- Settings → **Save system logs** writes `NickelStorygraph_<date>_<time>.log`
  into the root of the device storage. It is a `logread` snapshot, so boot
  lines (the firmware check) are only in it if you save it soon after a
  restart.
- The CLI keeps `.adds/NickelStorygraph/nickelstorygraph.log` (every call with
  `debug = true`, errors always).
- With SSH/telnet (devmode on Kobo): `logread | grep -iE 'storygraph|nh\.'`.

Remove cookie values before sharing a log. Include the firmware version and
the exact step that failed.

## 9. Tolino notes

Found on a Tolino with firmware 5.18.264769:

- `.kobo/version` is one comma-separated line, `serial,kernel,firmware,…`
  (`T303440090606,4.9.77,5.18.264769,…`); the firmware is the third field.
- The firmware converts every `.txt` file on the device into an EPUB and adds
  it to the library. Never put secrets into a `.txt` (hence `cookies.ini`), and
  watch out for Windows saving `cookies.ini.txt`.
- Shop books leave the database `ISBN` column empty; the plugin reads the ISBN
  from the shop's `pageplace.de` cover URL instead.
- If the device shows "connected via USB" but macOS sees no drive at all (not
  even in `diskutil list`), restart the Tolino (hold power ~10 s) and
  reconnect.
- There is no SSH; use **Save system logs** (section 8).
