#pragma once

#include <QJsonObject>
#include <QObject>

#include "nickelstorygraph.h"

// "Sync all books" from the home menu: syncs every book whose progress in
// Nickel's database moved forward since its last sync — books read in another
// reader (Kobalt) or while offline. Previews first (`sync-all --dry-run`) and
// asks before syncing, since unlinked books get linked to StoryGraph on the way.
class SyncAll : public QObject {
  Q_OBJECT

public:
  static void start();

private:
  explicit SyncAll(QObject *parent = nullptr);

  void preview(QJsonObject doc);
  void sync();
  void apply(QJsonObject doc);
  void closeProgress();

  bool includeStore = false;
  bool linkNew = false;
  ConfirmationDialog *progress = nullptr;
};
