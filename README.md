# NickelStorygraph

## About

A Kobo eReader plugin that integrates with [StoryGraph](https://app.thestorygraph.com). Syncs your reading progress automatically, logs highlights and notes as journal entries, and lets you view common StoryGraph features directly from your device.

> **Note:** Wi-Fi is required for all StoryGraph features. Because StoryGraph has no public API, this plugin works by scraping the website directly — it may break if StoryGraph makes changes to their UI.

## Contents

- [Why this fork](#why-this-fork)
- [Features](#features)
- [Screenshots](#screenshots)
- [Installation](#installation)
- [Uninstall](#uninstall)
- [Configuration](#configuration)
- [Usage](#usage)
- [Troubleshooting](#troubleshooting)
- [Building - Developer Info](#building---developer-info)
- [Architecture - Developer Info](#architecture---developer-info)
- [Roadmap](ROADMAP.md)
- [Acknowledgements](#acknowledgements)

## Why this fork

Upstream NickelStorygraph targets Kobo firmware 4.x, which runs Qt 5.2.1. Kobo/Tolino firmware **5.18** rebased Nickel onto **Qt 6.5.2**, which broke every native plugin. This fork builds the plugin from one source tree for **both** firmware lines — Qt 5 for 4.x and Qt 6 for 5.18 and 6.0 — so it stays mergeable with upstream. For Qt 6:

- three `libnickel` symbols changed ABI (verified against `libnickel.so.1.0.0`; see `test/libnickel-5.18.syms.gz`),
- Qt 6 turned `Qt::WindowType` into an `enum class`, breaking a constructor call,
- NickelHook needed a Qt 6 port (Failsafe fix and hook chaining), and
- packaging and the toolchain changed (`Kobo.tgz` instead of `KoboRoot.tgz`; a Qt 6.5.2 cross toolchain instead of NickelTC).

The Qt 5 build keeps upstream's toolchain (`.forgejo/nickeltc`, NickelTC) and `KoboRoot.tgz` packaging; the handful of ABI differences are compile-time switches in `hook/src/nickelstorygraph.cc`. See [Acknowledgements](#acknowledgements) for the people whose work this fork builds on.

> **Note:** This fork and its changes (code, documentation, and tests) were developed with the assistance of AI tools (large language models).

## Features

### Syncing & book linking

- **Progress sync** — Automatically updates your StoryGraph reading progress as you read, configurable by threshold or on book close
- **Auto-link** — Identifies the correct StoryGraph digital edition via ISBN or title/author search when you open a book for the first time
- **Edition management** — Manually link to any edition, switch editions, or sync your edition from changes made on the StoryGraph website
- **Book status** — Set Want to Read, Currently Reading, Paused, Read, or Did Not Finish from the device; rereading is detected automatically when you re-open a finished book

### Journal & reviews

- **Reading journal** — View your StoryGraph journal and add new entries with notes
- **Annotations** — Sync Kobo highlights and notes to your StoryGraph journal, each highlight as its own journal entry
- **Ratings & reviews** — Rate books (quarter-star), write reviews, tag moods, and answer book property questions; prompted automatically when you finish a book on your Kobo. Enable **Simple review** in Settings to show only the rating and thoughts fields

### Home screen menu

Accessible from the Kobo home screen without opening a book (works alongside [NickelMenu](https://pgaskin.net/NickelMenu/) if it's also installed):

- **Currently Reading** — Current and paused books with progress, in a tabbed view
- **Community Feed** — Scrollable view of your friends' reading progress
- **Reading Goals** — Progress towards your Page / Minutes / Books goals set on StoryGraph, plus your reading streak (current streak, longest streak, and streak requirement) if you have one set up


## Screenshots

### Home screen menu

| | | |
|---|---|---|
| ![Home screen tab bar menu](screenshots/home-menu.png) | ![Currently Reading](screenshots/currently-reading.png) | ![Currently Reading — Paused tab](screenshots/currently-reading-paused.png) |
| Tab bar menu | Currently Reading | Currently Reading — Paused |
| ![Community Feed](screenshots/community-feed.png) | ![Reading Goals and streak](screenshots/reading-goals.png) | |
| Community Feed | Reading Goals & streak | |

### In-book menu

| | |
|---|---|
| ![In-book reading toolbar menu](screenshots/in-book-menu.png) | ![Update book status](screenshots/book-status.png) |
| Reading toolbar menu | Update book status |

### Book linking

| | |
|---|---|
| ![Manually link book search results](screenshots/search-results.png) | ![Edition selection](screenshots/editions.png) |
| Manually link book | Edition selection |

### Journal & reviews

| | |
|---|---|
| ![Reading journal](screenshots/reading-journal.png) | ![Write a review — rating and thoughts](screenshots/review-ratings.png) |
| Reading journal | Write a review |
| ![Write a review — moods and book properties](screenshots/review-moods.png) | ![Write a review — Simple review mode](screenshots/review-simple.png) |
| Moods & book properties | Simple review mode |

### Settings

| | |
|---|---|
| ![Settings dialog](screenshots/settings.png) | ![Settings — book information and advanced](screenshots/settings-book-info.png) |
| Settings | Book information & advanced |

## Installation

Each release has a download for each firmware line. Check your firmware under **Settings → Device information** (Kobo) or **Settings → About** (Tolino). The `.kobo` and `.adds` folders may be hidden — on Windows, enable "Show hidden items" in File Explorer; on macOS, press `Cmd+Shift+.` in Finder.

### Firmware 5.18 and newer (Qt 6) — Kobo and Tolino

Download `Kobo.tgz` and `NickelStorygraph-adds.zip` from the [releases page](../../releases).

1. Connect the device to your computer via USB.
2. Unzip `NickelStorygraph-adds.zip` and copy the `NickelStorygraph` folder into the `.adds/` folder on the device root, so that you end up with `.adds/NickelStorygraph/`. Do this **first**: if that folder is missing when the plugin starts, it uninstalls itself.
3. Copy `Kobo.tgz` into the `.kobo/` folder on the device root.
4. Safely eject the device — it installs the plugin and restarts.

### Firmware 4.x (Qt 5) — Kobo

Download `KoboRoot.tgz` from the [releases page](../../releases). It contains the plugin and the `.adds/NickelStorygraph/` folder.

1. Connect the Kobo to your computer via USB.
2. Copy `KoboRoot.tgz` into the `.kobo/` folder on the device root.
3. Safely eject the Kobo — it installs the plugin and restarts.

If you pick the wrong download, the plugin notices at start-up: it checks the firmware and stays inactive on a firmware line it wasn't built for (see the log). Install the matching file instead.

### After installing

Add your StoryGraph cookies as described in [StoryGraph cookies](#storygraph-cookies).

## Uninstall

1. Connect your Kobo via USB.
2. Create an empty file named `NickelStorygraph_uninstall` in the root of the Kobo's internal storage (the same level as the `KOBOeReader` folder you see when you plug it in).
3. Safely eject the Kobo — the plugin will remove itself on reconnect and the file will be deleted automatically.

Alternatively, delete `.adds/NickelStorygraph/` from the device's internal storage manually and restart the Kobo.

## Configuration

The config file lives at `.adds/NickelStorygraph/config.ini` on the device's internal storage. `config_example.ini` in the same folder is only a reference with the full list of options and descriptions; the plugin never reads it, so editing it does nothing. The only required settings are your StoryGraph cookies.

### StoryGraph cookies

StoryGraph has no public API or login screen the plugin can use, so you log in by copying two cookies from your web browser. You only need to do this once, and again whenever they expire.

1. On a computer, log in to [app.thestorygraph.com](https://app.thestorygraph.com) and press `F12` to open the developer tools. Open the cookies list:
   - **Chrome / Edge:** **Application** tab → **Storage** → **Cookies** → `https://app.thestorygraph.com`
   - **Firefox:** **Storage** tab → **Cookies** → `https://app.thestorygraph.com`

   Copy the **Value** of `_storygraph_session` and of `remember_user_token`. If you don't see `remember_user_token`, log out of StoryGraph and log back in with **Remember me** ticked.
2. Create a file `.adds/NickelStorygraph/cookies.ini` on the device with a plain text editor (Notepad, TextEdit, VS Code; not Word), containing these two lines:

   ```ini
   session_cookie=<value of _storygraph_session>
   remember_user_token=<value of remember_user_token>
   ```

3. Eject the device, open the StoryGraph **Settings** and tap **Import cookies**. The cookies are written to `config.ini` and `cookies.ini` is deleted.

> **Don't name the file `cookies.txt`:** the firmware turns every `.txt` file on the device into a book in your library, which would show your login as a readable "book". **Windows users:** File Explorer hides file extensions by default, so a file you name `cookies.ini` may really be saved as `cookies.ini.txt`. Turn on **View → File name extensions** to check.

You can also put the same two lines into `config.ini` directly (remove the leading `;` from the commented-out lines). `user_slug` (your profile name) is cached automatically on first use.

`remember_user_token` is what keeps you logged in; StoryGraph renews the session cookie from it on its own. When the plugin reports that your login has expired, repeat the steps above with fresh cookies.

### Key options

| Option | Default | Description |
|---|---|---|
| `auto_sync_default` | `false` | Whether auto-sync is enabled for new books by default |
| `sync_on_close` | `always` | When to auto-sync: `always`, `never`, or a percentage threshold (e.g. `5`) |
| `threshold` | `20` | Sync while reading if progress changes by more than this many percentage points |
| `sync_bookmarks` | `always` | When to sync highlights/notes: `always`, `manual` (only on manual sync), `finished` (only on book completion), or `never` |
| `sync_daily` | off | Hour of day to sync even if asleep (1–24, e.g. `2` for 2am) |
| `home_menu` | `true` | Show the StoryGraph home screen menu (see below) |
| `home_menu_reading` / `home_menu_feed` / `home_menu_goals` | `true` | Show/hide individual items in the home screen menu |

## Usage

Open any book and tap the StoryGraph icon in the reading toolbar at the top of the screen to access the menu. **Auto-sync is disabled per book by default** — use **Enable auto-sync** from the menu the first time you open a book, or set `auto_sync_default = true` in `config.ini` to enable it automatically for all books.

- **Sync now** — Immediately push your current reading progress
- **Enable / Disable auto-sync** — Toggle automatic syncing for this book
- **Book management**
  - **Manually link book** / **Unlink book** — Search StoryGraph to link this book to a specific edition, or clear the link
  - **Sync edition from web** — Re-checks your StoryGraph currently-reading shelf and updates the linked edition if you changed it on the website
- **Update book status** — Set your reading status (Want to Read, Currently Reading, Paused, Read, Did Not Finish), and mark the book private or public with **Private: Yes / No**
- **Open reading journal** — Browse journal entries and add new ones
- **Write a review** — Rate and review the book
- **Settings** — Configure sync behaviour and other options (login cookies can only be set in `config.ini`)

### Home screen menu

A **StoryGraph** button in the home screen's tab bar (alongside Home/Discover/My Books/etc.) opens a menu with Currently Reading, Community Feed, Reading Goals, and Settings, without needing to open a book first. This works the same way whether or not [NickelMenu](https://pgaskin.net/NickelMenu/) is also installed — both mods add their own tab bar button side by side.

Disable this entirely with `home_menu = false` in `config.ini`, or hide individual items with `home_menu_reading`, `home_menu_feed`, `home_menu_goals`.

Your current reading streak (current streak, longest streak, and your streak requirement) shows at the bottom of the **Reading Goals** panel if you have one set up on StoryGraph.

### How auto-linking works

The first time a book is synced, NickelStorygraph looks up the correct StoryGraph edition automatically:

1. Extracts ISBNs from the EPUB metadata and searches StoryGraph
2. Falls back to a title + author search if no ISBN match is found
3. Prefers digital editions to avoid linking to the paperback or hardcover record
4. Saves the resolved edition ID so all subsequent syncs are instant

If a book can't be found automatically, a prompt appears to link it manually. Use **Book management → Sync edition from web** if you later change editions on StoryGraph's website.

## Troubleshooting

**The StoryGraph icon doesn't appear when I open a book.**
The plugin may not have installed correctly. Reconnect the Kobo via USB and confirm that the `.adds/NickelStorygraph/` folder exists and contains the `cli` file. If it's missing, repeat the installation steps.

**Syncing does nothing / I get an error about my login or a Cloudflare check.**
Your StoryGraph cookies have expired. Get fresh ones and import them as described in [StoryGraph cookies](#storygraph-cookies).

**Import cookies says "No cookies file found", or nothing changes after editing `config.ini`.**
The file name or location is off: it must be exactly `.adds/NickelStorygraph/cookies.ini` (on Windows, check that it isn't `cookies.ini.txt`). When editing `config.ini` directly, make sure the lines don't still start with `;` and that you edited `config.ini`, not `config_example.ini`.

**A book can't be found or links to the wrong edition.**
Use **Book management → Manually link book** to search for the correct edition and link it. Once linked, all future syncs use that edition directly.

**Something else is broken.**
Go to **Settings → Save system logs**, then reconnect the device via USB. The system log is saved in the root of the device's storage as `NickelStorygraph_<date>_<time>.log`. The CLI writes its own log to `.adds/NickelStorygraph/nickelstorygraph.log` on errors, and on every call with `debug = true` in `config.ini`. Remove any cookie values before sharing a log.

## Building - Developer Info

The full build runs inside a Docker container, one toolchain per firmware line,
selected with `QT=5` or `QT=6` (default):

- **QT=6** (firmware 5.18+): a Qt 6.5.2 cross toolchain (the Qt 6 counterpart of
  [NickelTC](https://github.com/pgaskin/NickelTC)) that builds Qt 6.5.2 from
  Kobo's published source against Ubuntu 22.04 armhf multiarch as the sysroot —
  see `.forgejo/nickeltc-qt6/`. Produces `hook/Kobo.tgz` and
  `mnt/onboard/.adds/NickelStorygraph/`.
- **QT=5** (firmware 4.x): upstream's NickelTC image (`.forgejo/nickeltc/`).
  Produces `KoboRoot.tgz`.

**Prerequisites:** Docker, [just](https://github.com/casey/just), [resvg](https://github.com/RazrFalcon/resvg)

```sh
# First time — build the toolchain image (Qt 6.5.2 takes a while)
just build-tc
QT=5 just build-tc

# Build the package for firmware 5.18+ / 4.x
just build-package
QT=5 just build-package

# Build and copy directly to a USB-connected Kobo (requires sudo)
just copy-package
```

Before deploying to a device, work through the [smoke-test checklist](SMOKE-TEST.md) — it covers the two Qt6 ABI changes that need on-device verification.

Releases are built by CI: merge `.github/release-notes/vX.Y.Z.md` to `main`, then push the tag `vX.Y.Z`. See [.github/release-notes/README.md](.github/release-notes/README.md).

Other useful commands:

| Command | Description |
|---|---|
| `just build-res` | Re-render SVG icons to PNG |
| `just build-hook` | Compile the Qt/C++ Nickel plugin only |
| `just build-cli` | Cross-compile the Rust CLI only |
| `just format` | Format all source files |
| `just clean` | Remove build artifacts |
| `just lint` | Run rustfmt + clippy + unit tests (static checks) |
| `just test` | Check hook symbols against the 5.18 firmware + verify build artifacts |
| `just logs` | Stream logs from the Kobo over SSH (requires `KOBO_SERVER` and `KOBO_PASSWORD` env vars) |

## Architecture - Developer Info

NickelStorygraph has two components that communicate via subprocess calls and JSON:

- **Hook** (`hook/`) — A Qt6/C++ plugin that injects into Kobo's Nickel UI (Qt 5 on firmware 4.x — see the `main` branch). Handles all UI rendering, menu items, dialogs, and user input. Spawns the CLI as a child process and reads its JSON output.
- **CLI** (`cli/`) — A Rust binary cross-compiled for ARM Linux. Contains all business logic: StoryGraph web scraping (session auth, progress updates, journal, reviews, search), Kobo SQLite queries for reading position and annotations, ISBN extraction from EPUB metadata, and edition resolution.


## Acknowledgements

This fork builds on the work of the following people — without them, none of this would exist.

- **Victoria Kerr** — author of the original [NickelStorygraph](https://github.com/VictoriaKerr131/NickelStorygraph).
- **Ava Johnson (StrayRose)** — author of [NickelHardcover](https://codeberg.org/StrayRose/NickelHardcover), a Kobo plugin for [Hardcover](https://hardcover.app), from which NickelStorygraph was forked. Large portions of the hook and CLI infrastructure derive from that project.
- **Patrick Gaskin (pgaskin)** — author of [NickelHook](https://github.com/pgaskin/NickelHook) (plugin injection), [NickelMenu](https://pgaskin.net/NickelMenu/) (whose source was read to make the two mods share the home-screen tab bar correctly, and to size the tab-bar icon at runtime the same way NickelMenu does), and [NickelTC](https://github.com/pgaskin/NickelTC) (the Qt 5 cross toolchain this fork replaces).
- **Nico Verbruggen (nicoverbruggen)** — author of [Kobalt](https://github.com/nicoverbruggen/kobalt) and the [Qt 6 NickelMenu port](https://github.com/nicoverbruggen/NickelMenu) (branch `qt6-support`). This fork uses his [NickelHook Qt 6 port](https://github.com/nicoverbruggen/NickelHook) (Failsafe fix and hook chaining) and the Qt 6.5.2 toolchain definition under `.forgejo/nickeltc/qt6/`.

The 5.18 symbol reference (`test/libnickel-5.18.syms.gz`) was extracted from `libnickel.so.1.0.0` of an official Kobo/Tolino 5.18 firmware image.

Victoria's original announcement, retained for context:

> Hi Fellow Redditors! It's come to my attention that this project has been shared on both Mobileread and Reddit over the last few weeks and while I'm very happy that others are getting use out of one of my personal pet projects I just want to set some expectations around support and updates. I built this project for myself because I often forget to update my progress on Storygraph and I knew there had to be a way to build an integration despite the lack of public API.
>
> At this point I didn't intend for this to be used by the masses, just a few friends who I convinced to come to the Kobo / StoryGraph dark side from Kindle. While all are absolutely welcome to use it in its current state, there may still be some issues and bugs that I haven't come across yet and support for resolution or feature requests etc will be very limited.
>
> Thanks for your understanding!
>
> — Victoria
