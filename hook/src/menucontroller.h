#pragma once

#include <functional>

#include <QJsonObject>
#include <QObject>
#include <QSettings>
#include <QWidgetAction>

#include "nickelstorygraph.h"

struct Item {
  QString text;
  QVariant value;
  bool checked;
};

class MenuController : public QWidget {
  Q_OBJECT

public:
  MenuController(int iconHeight, QWidget *parent = nullptr);
  TouchLabel *icon = nullptr;

public Q_SLOTS:
  static NickelTouchMenu *showMenu(QList<Item> items, QWidget *anchor, int offset, bool checkable = false,
                                   bool decorated = true);

  void showMainMenu();
  void showBookManagementMenu();
  void showStatusMenu(QJsonObject doc);
  void triggered(QAction *action);

private:
  void setSelected(bool selected);
  void setStatus(int status);
  void togglePrivate();
  // On a Kobo, asks before the plugin links an unlinked store book (Kobo
  // already syncs those with StoryGraph); runs `proceed` right away otherwise.
  void confirmStoreBook(std::function<void()> proceed);

  int iconHeight;
  bool currentPrivate = false; // from the last get/set-user-book response
};
