#include <NickelHook.h>
#include <QJsonArray>

#include "cli.h"
#include "settings.h"
#include "syncall.h"
#include "translations.h"

void SyncAll::start() { new SyncAll(); }

// Automatic entry point (Wi-Fi connect). No-op unless the user opted in; the
// guard against sync storms lives in SyncController::wifiConnected().
void SyncAll::startAuto() {
  if (!Settings::getInstance()->getSyncAllAuto())
    return;
  new SyncAll(nullptr, true);
}

SyncAll::SyncAll(QObject *parent, bool autoSync) : QObject(parent), autoSync(autoSync) {
  includeStore = Settings::getInstance()->getSyncAllStoreBooks();
  // The automatic path never links new books: the manual flow asks before
  // linking precisely because it puts books on the shelves, and a background
  // run must not do that on its own.
  linkNew = autoSync ? false : Settings::getInstance()->getSyncAllLinkNew();
  nh_log("SyncAll: start (auto=%d includeStore=%d linkNew=%d)", autoSync, includeStore, linkNew);

  if (autoSync) {
    // No preview and no confirmation: `sync-all` only touches already-linked
    // books with forward progress and is a no-op when there is nothing to do.
    sync();
    return;
  }

  CLI *cli = CLI::syncAll(includeStore, linkNew, true);
  QObject::connect(cli, &CLI::response, this, &SyncAll::preview);
  QObject::connect(cli, &CLI::failure, this, &QObject::deleteLater);
}

// Titles shown in the confirmation before "+ N more".
static constexpr int kPreviewTitles = 8;

void SyncAll::preview(QJsonObject doc) {
  QJsonArray books = doc.value("books").toArray();
  int toLink = 0;
  QStringList titles;
  for (QJsonValue const &value : books) {
    QJsonObject book = value.toObject();
    bool isNew = book.value("outcome").toString() == "would_link";
    if (isNew)
      toLink++;
    if (titles.size() < kPreviewTitles) {
      QString line = QString(QChar(0x2022)) + " " + book.value("title").toString().toHtmlEscaped() +
                     QString(" (%1%)").arg(book.value("progress").toInt());
      if (isNew)
        line += " " + t("(new)");
      titles.append(line);
    }
  }
  if (books.size() > kPreviewTitles)
    titles.append(QString(t("+ %1 more")).arg(books.size() - kPreviewTitles));

  if (books.isEmpty()) {
    ConfirmationDialogFactory__showErrorDialog("StoryGraph", t("All books are up to date."));
    deleteLater();
    return;
  }

  QString text = QString(t("%1 book(s) with new reading progress.")).arg(books.size());
  if (toLink > 0)
    text += "<br><br>" + QString(t("%1 of them will be linked to StoryGraph for the first time.")).arg(toLink);
  text += "<br><br>" + titles.join("<br>");

  ConfirmationDialog *prompt = ConfirmationDialogFactory__getConfirmationDialog(nullptr);
  ConfirmationDialog__setTitle(prompt, t("Sync all books"));
  ConfirmationDialog__setText(prompt, text);
  ConfirmationDialog__setAcceptButtonText(prompt, t("Sync"));
  ConfirmationDialog__setRejectButtonText(prompt, t("Cancel"));
  ConfirmationDialog__showCloseButton(prompt, false);
  QObject::connect(prompt, &QDialog::accepted, this, [this, prompt]() {
    prompt->deleteLater();
    sync();
  });
  QObject::connect(prompt, &QDialog::rejected, this, [this, prompt]() {
    prompt->deleteLater();
    deleteLater();
  });
  prompt->open();
}

void SyncAll::sync() {
  nh_log("SyncAll: sync (auto=%d)", autoSync);
  if (!autoSync) {
    progress = ConfirmationDialogFactory__getConfirmationDialog(nullptr);
    ConfirmationDialog__showCloseButton(progress, false);
    ConfirmationDialog__setText(progress, t("Syncing with StoryGraph..."));
    progress->open();
  }

  CLI::Options options;
  // A silent sync does not raise a Wi-Fi icon or an error dialog, and — when
  // the network isn't up yet — waits for Nickel's own connection instead of
  // initiating one (see CLI::CLI).
  options.silent = autoSync;
  CLI *cli = CLI::syncAll(includeStore, linkNew, false, options);
  QObject::connect(cli, &CLI::response, this, &SyncAll::apply);
  QObject::connect(cli, &CLI::failure, this, [this]() {
    closeProgress();
    deleteLater();
  });
}

// Records the results the same way a single-book sync does (SyncQueue::run):
// linked id, last synced progress, status, and auto-sync off at 100%.
void SyncAll::apply(QJsonObject doc) {
  Settings *settings = Settings::getInstance();
  int synced = 0, ahead = 0, notFound = 0, errors = 0;

  for (QJsonValue const &value : doc.value("books").toArray()) {
    QJsonObject book = value.toObject();
    QString contentId = book.value("content_id").toString();
    QString outcome = book.value("outcome").toString();
    QString bookId = book.value("book_id").toString();
    int progressValue = book.value("progress").toInt();

    if (!bookId.isEmpty() && (outcome == "synced" || outcome == "linked" || outcome == "already_ahead"))
      settings->setLinkedId(contentId, bookId);

    if (outcome == "synced" || outcome == "linked") {
      synced++;
      settings->setLastProgress(contentId, progressValue);
      int statusId = book.value("status_id").toInt(0);
      if (statusId > 0)
        settings->setStatus(contentId, statusId);
      if (progressValue == 100)
        settings->setEnabled(contentId, false);
    } else if (outcome == "already_ahead") {
      ahead++;
    } else if (outcome == "not_found") {
      notFound++;
    } else {
      errors++;
    }
  }
  nh_log("SyncAll: synced=%d ahead=%d notFound=%d errors=%d", synced, ahead, notFound, errors);

  QString summary = QString(t("Synced: %1")).arg(synced);
  if (ahead > 0)
    summary += "<br>" + QString(t("Already up to date on StoryGraph: %1")).arg(ahead);
  if (notFound > 0)
    summary += "<br>" + QString(t("Not found on StoryGraph: %1")).arg(notFound) + "<br><br>" +
               t("Link them from the book menu.");
  if (errors > 0)
    summary += "<br>" + QString(t("Errors: %1")).arg(errors);

  closeProgress();
  // The automatic path is silent: record the results, show nothing.
  if (!autoSync)
    ConfirmationDialogFactory__showErrorDialog("StoryGraph", summary);
  deleteLater();
}

void SyncAll::closeProgress() {
  if (progress != nullptr) {
    progress->close();
    progress->deleteLater();
    progress = nullptr;
  }
}
