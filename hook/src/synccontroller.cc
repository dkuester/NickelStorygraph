#include <NickelHook.h>
#include <QDateTime>
#include <QLabel>
#include <QSettings>
#include <QTimer>

#include "cli.h"
#include "settings.h"
#include "syncall.h"
#include "synccontroller.h"

// Minimum gap between two automatic batch syncs. The trigger is the Wi-Fi
// connect event, which can fire repeatedly (a flapping link, a reconnect after
// sleep); this window keeps a burst of them from turning into a sync storm.
// The manual home-menu entry is unaffected and always available.
static constexpr qint64 kAutoSyncMinIntervalSeconds = 60 * 60;

SyncController *SyncController::getInstance() {
  static SyncController instance;
  return &instance;
};

SyncController::SyncController(QObject *parent) : QObject(parent) {
  // Stufe 1: the batch sync can also run automatically when Wi-Fi comes up.
  // Connected here (not at a book-open hook) so it is active as soon as the
  // plugin is up, independent of Nickel's reading stack — books read in Kobalt
  // never touch those hooks.
  WirelessManager *wm = WirelessManager__sharedInstance();
  if (wm) {
    QObject::connect(wm, SIGNAL(networkConnected()), this, SLOT(wifiConnected()));
  }
};

void SyncController::wifiConnected() {
  if (!Settings::getInstance()->getSyncAllAuto())
    return;

  QDateTime now = QDateTime::currentDateTime();
  if (lastAutoSyncAt.isValid() && lastAutoSyncAt.secsTo(now) < kAutoSyncMinIntervalSeconds) {
    nh_log("SyncController::wifiConnected: last auto sync %llds ago, skipping",
           static_cast<long long>(lastAutoSyncAt.secsTo(now)));
    return;
  }

  nh_log("SyncController::wifiConnected: triggering automatic sync-all");
  lastAutoSyncAt = now;
  SyncAll::startAuto();
}

void SyncController::currentViewIndexChanged(int index) {
  if (index < 0)
    return;

  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);
  if (!cv) return;
  QString name = cv->objectName();
  nh_log("Current view changed to %s last view was %s", qPrintable(name), qPrintable(lastViewName));

  if (name != lastViewName) {
    currentViewChanged(name);
  }

  QDateTime alarm = getAlarm();
  if (name.endsWith("DragonPowerView") && alarm.isValid()) {
    nh_log("Alarm set for %s", qPrintable(alarm.toString()));
  }

  Settings *settings = Settings::getInstance();
  if (lastViewName == "ReadingView" && settings->isEnabled(contentId)) {
    if (queue->checkThreshold(contentId, settings->getCloseThreshold())) {
      nh_log("Triggered on close auto-sync");
      queue->run(contentId);
    }
  }

  if (name == "ReadingView") {
    queue->updateReadProgress(contentId);

    QObject::connect(cv, SIGNAL(pageChanged(int)), this, SLOT(pageChanged()), Qt::UniqueConnection);
  }

  lastViewName = name;
}

void SyncController::pageChanged() {
  nh_log("SyncController::pageChanged()");

  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);
  if (!cv) return;
  QString name = cv->objectName();
  if (name == "ReadingView") {
    queue->updateReadProgress(contentId);
  }

  Settings *settings = Settings::getInstance();

  // Independent of the per-book auto-sync toggle below - completion is a
  // one-time user prompt, not a recurring background sync, so it shouldn't
  // require the user to have opted a book into auto-sync just to be asked
  // whether they finished it.
  if (settings->getCompletionPrompt() && queue->getReadProgress(contentId) == 100 &&
      !settings->getCompletionPrompted(contentId)) {
    settings->setCompletionPrompted(contentId, true);
    queue->showCompletionPrompt(contentId);
  }

  if (!settings->isEnabled(contentId))
    return;

  int syncDaily = settings->getSyncDaily();

  if (timer != nullptr && (PowerTimer__timeRemaining(timer) <= 0 || syncDaily != lastSyncDaily)) {
    timer->deleteLater();
    timer = nullptr;
  }

  lastSyncDaily = syncDaily;

  if (timer == nullptr && syncDaily > 0 && queue->checkThreshold(contentId, 1)) {
    timer = construct_PowerTimer("NickelStorygraph-alarm", this);

    QDateTime time = QDateTime::currentDateTime();
    if (time.time().hour() >= syncDaily) {
      time = time.addDays(1);
    }
    time.setTime(QTime(syncDaily, 0));
    nh_log("Setting alarm for %s", qPrintable(time.toString()));

    PowerTimer__fireAt(timer, time);
    QObject::connect(timer, SIGNAL(timeout()), this, SLOT(alarm()));
  }

  if ((queue->getReadProgress(contentId) == 100 && settings->getLastProgress(contentId) != 100) ||
      queue->checkThreshold(contentId, settings->getPageThreshold())) {
    nh_log("Triggered threshold auto-sync");
    queue->run(contentId);
  }
}

void SyncController::alarm() {
  nh_log("SyncController::alarm()");

  timer->deleteLater();
  timer = nullptr;
  queue->prepareNext();
}

int SyncController::getReadProgress() { return queue->getReadProgress(contentId); }

void SyncController::manualSync() {
  nh_log("SyncController::manualSync()");
  // Read the current progress directly so a manual sync also works right after
  // opening a book, when the QStackedWidget::currentChanged signal (which
  // normally queues the progress) may not have fired yet.
  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);
  if (cv && cv->objectName() == "ReadingView") {
    queue->updateReadProgress(contentId);
  }
  queue->run(contentId, true);
}

QDateTime SyncController::getAlarm() {
  if (timer != nullptr && PowerTimer__timeRemaining(timer) > 0) {
    return QDateTime::currentDateTime().addMSecs(PowerTimer__timeRemaining(timer));
  }

  return QDateTime();
}
