#pragma once

#include <QDateTime>
#include <QObject>
#include <QSettings>
#include <QVariant>

#include "files.h"

class Settings : public QObject {
  Q_OBJECT

public:
  static Settings *getInstance();

  void setEnabled(QString contentId, bool value);
  bool isEnabled(QString contentId);

  void setLinkedId(QString contentId, QString value);
  QString getLinkedId(QString contentId);
  void clearBookLink(QString contentId);

  void setStatus(QString contentId, int value);
  int getStatus(QString contentId);

  void setLastProgress(QString contentId, int value);
  int getLastProgress(QString contentId);

  void setCompletionPrompted(QString contentId, bool value);
  bool getCompletionPrompted(QString contentId);

  void setAutoSyncDefault(bool value);
  bool getAutoSyncDefault();

  void setSyncBookmarks(QString value);
  QString getSyncBookmarks();

  QString getSessionCookie();
  QString getRememberToken();
  QString getLanguage();

  void setUserSlug(QString value);

  void setSessionCookie(QString value);
  void setRememberToken(QString value);

  void setSyncDaily(int value);
  int getSyncDaily();

  void setCloseThreshold(int value);
  int getCloseThreshold();

  void setPageThreshold(int value);
  int getPageThreshold();

  void setDebug(bool value);
  bool getDebug();

  void setSimpleReview(bool value);
  bool getSimpleReview();

  void setCompletionPrompt(bool value);
  bool getCompletionPrompt();

  bool getHomeMenuEnabled();
  bool getHomeMenuReading();
  bool getHomeMenuFeed();
  bool getHomeMenuGoals();

  void setHomeMenuEnabled(bool value);
  void setHomeMenuReading(bool value);
  void setHomeMenuFeed(bool value);
  void setHomeMenuGoals(bool value);

  bool is24HourClock();

  // "Sync all books": whether store books are included. Kobo syncs store books
  // with StoryGraph natively, so the default is off there and on on Tolino.
  bool getSyncAllStoreBooks();
  // "Sync all books": whether books that aren't linked yet get linked on the
  // way. Off by default, so the batch only fills in progress for linked books.
  bool getSyncAllLinkNew();
  // Every book with plugin settings, as the JSON list `sync-all` reads on stdin.
  QByteArray knownBooksJson();

public Q_SLOTS:
  void currentViewChanged(QString name);

private:
  Settings(QObject *parent = nullptr);

  QSettings *internal = nullptr;
  QSettings *config = nullptr;
  QSettings *kobo = nullptr;

  QString getPath(QString contentId, QString key);
  void setValue(QString contentId, QString key, QVariant value);
  QVariant getValue(QString contentId, QString key, QVariant defaultValue = QVariant());
};
