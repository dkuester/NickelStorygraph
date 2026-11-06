#pragma once

#include <QJsonObject>
#include <QObject>

#include "nickelstorygraph.h"

// "Sync all books" from the home menu: syncs every book whose progress in
// Nickel's database moved forward since its last sync — books read in another
// reader (Kobalt) or while offline. Previews first (`sync-all --dry-run`) and
// asks before syncing, since unlinked books get linked to StoryGraph on the way.
//
// startAuto() is the same batch on the automatic path (e.g. when Wi-Fi comes
// up): silent, no confirmation, and it never links new books — only books the
// user already linked are synced, so nothing lands on StoryGraph on its own.
class SyncAll : public QObject {
  Q_OBJECT

public:
  static void start();
  static void startAuto();

private:
  explicit SyncAll(QObject *parent = nullptr, bool autoSync = false);

  void preview(QJsonObject doc);
  void sync();
  void apply(QJsonObject doc);
  void closeProgress();

  bool autoSync = false;
  bool includeStore = false;
  bool linkNew = false;
  ConfirmationDialog *progress = nullptr;
};
