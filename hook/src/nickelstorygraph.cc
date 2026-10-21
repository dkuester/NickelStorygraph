#include <dlfcn.h>

#include <stdio.h>
#include <string.h>

#include <QApplication>
#include <QFont>
#include <QHBoxLayout>
#include <QImage>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPixmap>
#include <QPointer>
#include <QPushButton>
#include <QStackedWidget>
#include <QString>
#include <QWidgetAction>

#include <NickelHook.h>

// One source tree builds for both firmware lines (NH_QT_MAJOR in hook/Makefile):
// Qt 5.2.1 on firmware 4.x and Qt 6.5.2 on firmware 5.18+. The few libnickel
// symbols whose ABI changed between the two are switched on this.
#define NSG_QT6 (QT_VERSION >= QT_VERSION_CHECK(6, 0, 0))

#include "cli.h"
#include "files.h"
#include "menucontroller.h"
#include "feed/feeddialog.h"
#include "goals/goalsdialog.h"
#include "reading/readingdialog.h"
#include "settings.h"
#include "settings/settingsdialog.h"
#include "syncall.h"
#include "synccontroller.h"
#include "translations.h"

typedef void ReadingController;
typedef void Volume;
typedef void Bookmark;
static void (*ReadingController__setVolume)(ReadingController *_this, Volume *volume, Bookmark *bookmark);
#if NSG_QT6
// Kobo vs. Tolino: only firmware 5.18+ runs on Tolinos, so the Qt5 build needs
// no lookup. Both symbols are optional; without them the device counts as Kobo.
typedef void Device;
static Device *(*Device__getCurrentDevice)() = nullptr;
static bool (*Device__isTolino)(Device const *_this) = nullptr;
#endif

#if NSG_QT6
static QString (*Content__getId)(Volume *_this, QString const &);
#else
static QString (*Content__getId)(Volume *_this);
#endif
static QString (*Content__getTitle)(Volume *_this);
static QString (*Content__getAttribution)(Volume *_this);

typedef QList<QPair<QString, QString>> SupportedLocales;
static SupportedLocales *(*SupportedLocales__supportedLocales)(SupportedLocales *_this, bool b1);

MainWindowController *(*MainWindowController__sharedInstance)();
QWidget *(*MainWindowController__currentView)(MainWindowController *mwc);
QWidget *(*MainWindowController__pushView)(MainWindowController *mwc, QWidget *view);
WirelessManager *(*WirelessManager__sharedInstance)();

void (*ConfirmationDialogFactory__showErrorDialog)(QString const &title, QString const &content);
ConfirmationDialog *(*ConfirmationDialogFactory__getConfirmationDialog)(QWidget *parent);
void (*ConfirmationDialog__setTitle)(ConfirmationDialog *_this, QString const &);
void (*ConfirmationDialog__setText)(ConfirmationDialog *_this, QString const &);
void (*ConfirmationDialog__setAcceptButtonText)(ConfirmationDialog *_this, QString const &);
void (*ConfirmationDialog__setRejectButtonText)(ConfirmationDialog *_this, QString const &);
void (*ConfirmationDialog__showCloseButton)(ConfirmationDialog *_this, bool enabled);

int (*ReadingView__getCalculatedReadProgress)(QWidget *_this);

WirelessWorkflowManager *(*WirelessWorkflowManager__sharedInstance)();
void (*WirelessWorkflowManager__connectWirelessSilently)(WirelessWorkflowManager *_this);
void (*WirelessWorkflowManager__connectWireless)(WirelessWorkflowManager *_this, bool, bool);
bool (*WirelessWorkflowManager__isInternetAccessible)(WirelessWorkflowManager *_this);

void (*TouchLabel__constructor)(TouchLabel *_this, QWidget *parent, QFlags<Qt::WindowType>);
void (*TouchLabel__setHitStateEnabled)(TouchLabel *_this, bool enabled);

// Nickel's widget classes are allocated by hand because their constructors are
// not exported symbols: we resolve the constructor function pointers below
// (via NickelHook) and call them on raw zeroed storage. Every allocation is a
// uniform 2048 bytes — a generous upper bound, not an exact sizeof: the
// hand-measured Qt6 / firmware 5.18 layouts fit with large headroom, so a
// modest layout growth on newer firmware (5.19+/6.x) stays inside the buffer
// without re-measuring. The buffer must be >= sizeof(the class) or the
// constructor overruns it and corrupts the heap.
TouchLabel *construct_TouchLabel(QWidget *parent) {
  TouchLabel *label = reinterpret_cast<TouchLabel *>(calloc(1, 2048));
  TouchLabel__constructor(label, parent, Qt::WindowType());
  return label;
}

void (*N3ButtonLabel__constructor)(N3ButtonLabel *_this, QWidget *parent);

N3ButtonLabel *construct_N3ButtonLabel(QWidget *parent) {
  N3ButtonLabel *button = reinterpret_cast<N3ButtonLabel *>(calloc(1, 2048));
  N3ButtonLabel__constructor(button, parent);
  return button;
}

void (*TouchCheckBox__constructor)(TouchCheckBox *_this, QWidget *parent);

TouchCheckBox *construct_TouchCheckBox(QWidget *parent) {
  TouchCheckBox *checkbox = reinterpret_cast<TouchCheckBox *>(calloc(1, 2048));
  TouchCheckBox__constructor(checkbox, parent);
  return checkbox;
}

void (*NickelTouchMenu__constructor)(NickelTouchMenu *_this, QWidget *parent, int pos);
void (*NickelTouchMenu__showDecoration)(NickelTouchMenu *_this, bool show);

NickelTouchMenu *construct_NickelTouchMenu(QWidget *parent, int pos) {
  NickelTouchMenu *menu = reinterpret_cast<NickelTouchMenu *>(calloc(1, 2048));
  NickelTouchMenu__constructor(menu, parent, pos);
  return menu;
}

void (*MenuTextItem__constructor)(MenuTextItem *_this, QWidget *parent, bool checkable, bool italic);
void (*MenuTextItem__setText)(MenuTextItem *_this, QString const &text);
void (*MenuTextItem__setSelected)(MenuTextItem *_this, bool selected);
void (*MenuTextItem__registerForTapGestures)(MenuTextItem *_this);

MenuTextItem *construct_MenuTextItem(QWidget *parent, bool checkable, bool italic) {
  MenuTextItem *item = reinterpret_cast<MenuTextItem *>(calloc(1, 2048));
  MenuTextItem__constructor(item, parent, checkable, italic);
  return item;
}

typedef QWidget ReadingMenuView;
#if NSG_QT6
void (*ReadingMenuView__constructor)(ReadingMenuView *_this, QWidget *parent, bool unknown, bool unknown2);
#else
void (*ReadingMenuView__constructor)(ReadingMenuView *_this, QWidget *parent, bool unknown);
void (*ReadingMenuView__constructor_2)(ReadingMenuView *_this, QWidget *, QByteArray const &unknownArray,
                                       bool unknownBool);
#endif

typedef QWidget MainNavView;
typedef QWidget MainNavButton;
static void (*MainNavView__constructor)(MainNavView *_this, QWidget *parent);
static void (*MainNavButton__constructor)(MainNavButton *_this, QWidget *parent);
static void (*MainNavButton__setPixmap)(MainNavButton *_this, QString const &pixmapName);
static void (*MainNavButton__setActivePixmap)(MainNavButton *_this, QString const &pixmapName);
static void (*MainNavButton__setText)(MainNavButton *_this, QString const &text);

N3Dialog *(*N3DialogFactory__getDialog)(QWidget *content, bool unknown);
void (*N3Dialog__disableCloseButton)(N3Dialog *__this);
void (*N3Dialog__enableBackButton)(N3Dialog *__this, bool enable);
void (*N3Dialog__setTitle)(N3Dialog *__this, QString const &);
KeyboardFrame *(*N3Dialog__keyboardFrame)(N3Dialog *__this);
void (*N3Dialog__enableFullViewMode)(N3Dialog *__this);
void (*N3Dialog__showKeyboard)(N3Dialog *__this);
void (*N3Dialog__hideKeyboard)(N3Dialog *__this);

void (*KeyboardReceiver__constructor)(KeyboardReceiver *__this, QLineEdit *lineEdit, bool unknown);
void (*KeyboardReceiver__TextEdit_constructor)(KeyboardReceiver *__this, QTextEdit *parent, bool unknown);

KeyboardReceiver *construct_KeyboardReceiver(QLineEdit *lineEdit) {
  KeyboardReceiver *receiver = reinterpret_cast<KeyboardReceiver *>(calloc(1, 2048));
  KeyboardReceiver__constructor(receiver, lineEdit, false);
  return receiver;
}

KeyboardReceiver *construct_KeyboardReceiver(QTextEdit *textEdit) {
  KeyboardReceiver *receiver = reinterpret_cast<KeyboardReceiver *>(calloc(1, 2048));
  KeyboardReceiver__TextEdit_constructor(receiver, textEdit, false);
  return receiver;
}

SearchKeyboardController *(*KeyboardFrame__createKeyboard)(KeyboardFrame *__this, int keyboardScript, QLocale locale);
void (*SearchKeyboardController__setReceiver)(SearchKeyboardController *__this, KeyboardReceiver *receiver, bool idk);
void (*SearchKeyboardController__setGoText)(SearchKeyboardController *__this, QString const &text);

void (*TouchLineEdit__constructor)(TouchLineEdit *__this, QWidget *parent);

TouchLineEdit *construct_TouchLineEdit(QWidget *parent) {
  TouchLineEdit *lineEdit = reinterpret_cast<TouchLineEdit *>(calloc(1, 2048));
  TouchLineEdit__constructor(lineEdit, parent);
  return lineEdit;
}

void (*SettingContainer__constructor)(SettingContainer *__this, QWidget *parent);

SettingContainer *construct_SettingContainer(QWidget *parent) {
  SettingContainer *container = reinterpret_cast<SettingContainer *>(calloc(1, 2048));
  SettingContainer__constructor(container, parent);
  return container;
}

void (*TouchTextEdit__constructor)(TouchTextEdit *__this, QWidget *parent);
void (*TouchTextEdit__setCustomPlaceholderText)(TouchTextEdit *__this, QString const &text);

TouchTextEdit *construct_TouchTextEdit(QWidget *parent) {
  TouchTextEdit *touchText = reinterpret_cast<TouchTextEdit *>(calloc(1, 2048));
  TouchTextEdit__constructor(touchText, parent);
  return touchText;
}

void (*PowerTimer__constructor)(PowerTimer *__this, QString const &name, QObject *parent);
void (*PowerTimer__fireAt)(PowerTimer *__this, QDateTime const &datatime);
int (*PowerTimer__timeRemaining)(PowerTimer *__this);

PowerTimer *construct_PowerTimer(QString const &name, QObject *parent) {
  PowerTimer *timer = reinterpret_cast<PowerTimer *>(calloc(1, 2048));
  PowerTimer__constructor(timer, name, parent);
  return timer;
}

static struct nh_info NickelStorygraph = (struct nh_info){.name = "NickelStorygraph",
                                                         .desc = "Updates reading progress on StoryGraph",
                                                         .uninstall_flag = "/mnt/onboard/NickelStorygraph_uninstall",
                                                         .uninstall_xflag = "/mnt/onboard/.adds/NickelStorygraph",
                                                         .failsafe_delay = 30};

// clang-format off
static struct nh_hook NickelStorygraphHook[] = {
  { .sym = "_ZN17ReadingController9setVolumeERK6VolumeRK8Bookmark", .sym_new = "_nh_ReadingController__setVolume",       .lib = "libnickel.so.1.0.0", .out = nh_symoutptr(ReadingController__setVolume),       .desc = "The main entry point" },
#if NSG_QT6
  { .sym = "_ZN15ReadingMenuViewC1EP7QWidgetbb",               .sym_new = "_nh_ReadingMenuView__constructor",       .lib = "libnickel.so.1.0.0", .out = nh_symoutptr(ReadingMenuView__constructor),       .desc = "Used to inject menu items", .optional = true }, // Version 5.18+
#else
  { .sym = "_ZN15ReadingMenuViewC1EP7QWidgetb",                     .sym_new = "_nh_ReadingMenuView__constructor",       .lib = "libnickel.so.1.0.0", .out = nh_symoutptr(ReadingMenuView__constructor),       .desc = "Used to inject menu items", .optional = true },
  { .sym = "_ZN15ReadingMenuViewC1EP7QWidgetRK10QByteArrayb",       .sym_new = "_nh_ReadingMenuView__constructor_2",     .lib = "libnickel.so.1.0.0", .out = nh_symoutptr(ReadingMenuView__constructor_2),     .desc = "Used to inject menu items", .optional = true }, // Version 4.44+
#endif
  { .sym = "_ZN16SupportedLocales16supportedLocalesEb",             .sym_new = "_nh_SupportedLocales__supportedLocales", .lib = "libnickel.so.1.0.0", .out = nh_symoutptr(SupportedLocales__supportedLocales), .desc = "Add support for 'English (UK)'", .optional = true },
  { .sym = "_ZN11MainNavViewC1EP7QWidget",                         .sym_new = "_nh_MainNavView__constructor",          .lib = "libnickel.so.1.0.0", .out = nh_symoutptr(MainNavView__constructor),          .desc = "Home screen StoryGraph menu button", .optional = true },
  {0},
};

static struct nh_dlsym NickelStorygraphDlsym[] = {
#if NSG_QT6
  { .name = "_ZN6Device16getCurrentDeviceEv",                                  .out = nh_symoutptr(Device__getCurrentDevice),      .desc = "Kobo/Tolino detection", .optional = true },
  { .name = "_ZNK6Device8isTolinoEv",                                          .out = nh_symoutptr(Device__isTolino),              .desc = "Kobo/Tolino detection", .optional = true },
  { .name = "_ZNK7Content5getIdERK7QString",                                 .out = nh_symoutptr(Content__getId) },
#else
  { .name = "_ZNK7Content5getIdEv",                                            .out = nh_symoutptr(Content__getId) },
#endif
  { .name = "_ZNK7Content8getTitleEv",                                         .out = nh_symoutptr(Content__getTitle) },
  { .name = "_ZNK7Content14getAttributionEv",                                  .out = nh_symoutptr(Content__getAttribution) },

  { .name = "_ZN20MainWindowController14sharedInstanceEv",                     .out = nh_symoutptr(MainWindowController__sharedInstance) },
  { .name = "_ZNK20MainWindowController11currentViewEv",                       .out = nh_symoutptr(MainWindowController__currentView) },
  { .name = "_ZN20MainWindowController8pushViewEP7QWidget",                    .out = nh_symoutptr(MainWindowController__pushView) },
  { .name = "_ZN11ReadingView25getCalculatedReadProgressEv",                   .out = nh_symoutptr(ReadingView__getCalculatedReadProgress) },

  { .name = "_ZN25ConfirmationDialogFactory15showErrorDialogERK7QStringS2_",   .out = nh_symoutptr(ConfirmationDialogFactory__showErrorDialog) },
  { .name = "_ZN25ConfirmationDialogFactory21getConfirmationDialogEP7QWidget", .out = nh_symoutptr(ConfirmationDialogFactory__getConfirmationDialog) },
  { .name = "_ZN18ConfirmationDialog8setTitleERK7QString",                     .out = nh_symoutptr(ConfirmationDialog__setTitle) },
  { .name = "_ZN18ConfirmationDialog7setTextERK7QString",                      .out = nh_symoutptr(ConfirmationDialog__setText) },
  { .name = "_ZN18ConfirmationDialog19setAcceptButtonTextERK7QString",         .out = nh_symoutptr(ConfirmationDialog__setAcceptButtonText) },
  { .name = "_ZN18ConfirmationDialog19setRejectButtonTextERK7QString",         .out = nh_symoutptr(ConfirmationDialog__setRejectButtonText) },
  { .name = "_ZN18ConfirmationDialog15showCloseButtonEb",                      .out = nh_symoutptr(ConfirmationDialog__showCloseButton) },

  { .name = "_ZN23WirelessWorkflowManager14sharedInstanceEv",                  .out = nh_symoutptr(WirelessWorkflowManager__sharedInstance) },
  { .name = "_ZN23WirelessWorkflowManager20isInternetAccessibleEv",            .out = nh_symoutptr(WirelessWorkflowManager__isInternetAccessible) },
  { .name = "_ZN23WirelessWorkflowManager15connectWirelessEbb",                .out = nh_symoutptr(WirelessWorkflowManager__connectWireless) },
  { .name = "_ZN23WirelessWorkflowManager23connectWirelessSilentlyEv",         .out = nh_symoutptr(WirelessWorkflowManager__connectWirelessSilently) },
  { .name = "_ZN15WirelessManager14sharedInstanceEv",                          .out = nh_symoutptr(WirelessManager__sharedInstance) },

  { .name = "_ZN10TouchLabelC1EP7QWidget6QFlagsIN2Qt10WindowTypeEE",           .out = nh_symoutptr(TouchLabel__constructor) },
  { .name = "_ZN10TouchLabel18setHitStateEnabledEb",                           .out = nh_symoutptr(TouchLabel__setHitStateEnabled) },

  { .name = "_ZN13N3ButtonLabelC1EP7QWidget",                                  .out = nh_symoutptr(N3ButtonLabel__constructor) },

  { .name = "_ZN13TouchCheckBoxC1EP7QWidget",                                  .out = nh_symoutptr(TouchCheckBox__constructor) },

  { .name = "_ZN15NickelTouchMenuC1EP7QWidget18DecorationPosition",            .out = nh_symoutptr(NickelTouchMenu__constructor) },
  { .name = "_ZN15NickelTouchMenu14showDecorationEb",                          .out = nh_symoutptr(NickelTouchMenu__showDecoration) },
  { .name = "_ZN12MenuTextItemC1EP7QWidgetbb",                                 .out = nh_symoutptr(MenuTextItem__constructor) },
  { .name = "_ZN12MenuTextItem7setTextERK7QString",                            .out = nh_symoutptr(MenuTextItem__setText) },
  { .name = "_ZN12MenuTextItem11setSelectedEb",                                .out = nh_symoutptr(MenuTextItem__setSelected) },
  { .name = "_ZN12MenuTextItem22registerForTapGesturesEv",                     .out = nh_symoutptr(MenuTextItem__registerForTapGestures) },

  { .name = "_ZN15N3DialogFactory9getDialogEP7QWidgetb",                       .out = nh_symoutptr(N3DialogFactory__getDialog) },
  { .name = "_ZN8N3Dialog18disableCloseButtonEv",                              .out = nh_symoutptr(N3Dialog__disableCloseButton) },
  { .name = "_ZN8N3Dialog16enableBackButtonEb",                                .out = nh_symoutptr(N3Dialog__enableBackButton) },
  { .name = "_ZN8N3Dialog8setTitleERK7QString",                                .out = nh_symoutptr(N3Dialog__setTitle) },
  { .name = "_ZN8N3Dialog13keyboardFrameEv",                                   .out = nh_symoutptr(N3Dialog__keyboardFrame) },
  { .name = "_ZN8N3Dialog18enableFullViewModeEv",                              .out = nh_symoutptr(N3Dialog__enableFullViewMode) },
  { .name = "_ZN8N3Dialog12hideKeyboardEv",                                    .out = nh_symoutptr(N3Dialog__hideKeyboard) },
  { .name = "_ZN8N3Dialog12showKeyboardEv",                                    .out = nh_symoutptr(N3Dialog__showKeyboard) },

  { .name = "_ZN13TouchLineEditC1EP7QWidget",                                  .out = nh_symoutptr(TouchLineEdit__constructor) },
  { .name = "_ZN16KeyboardReceiverC1EP9QLineEditb",                            .out = nh_symoutptr(KeyboardReceiver__constructor) },
  { .name = "_ZN16KeyboardReceiverC1EP9QTextEditb",                            .out = nh_symoutptr(KeyboardReceiver__TextEdit_constructor) },
  { .name = "_ZN13KeyboardFrame14createKeyboardE14KeyboardScriptRK7QLocale",   .out = nh_symoutptr(KeyboardFrame__createKeyboard) },
  { .name = "_ZN24SearchKeyboardController11setReceiverEP16KeyboardReceiverb", .out = nh_symoutptr(SearchKeyboardController__setReceiver) },
  { .name = "_ZN24SearchKeyboardController9setGoTextERK7QString",              .out = nh_symoutptr(SearchKeyboardController__setGoText) },

  { .name = "_ZN16SettingContainerC1EP7QWidget",                               .out = nh_symoutptr(SettingContainer__constructor) },

  { .name = "_ZN13TouchTextEditC1EP7QWidget",                                  .out = nh_symoutptr(TouchTextEdit__constructor) },
  { .name = "_ZN13TouchTextEdit24setCustomPlaceholderTextERK7QString",         .out = nh_symoutptr(TouchTextEdit__setCustomPlaceholderText) },

  { .name = "_ZN10PowerTimerC1ERK7QStringP7QObject",                           .out = nh_symoutptr(PowerTimer__constructor) },
  { .name = "_ZN10PowerTimer6fireAtERK9QDateTime",                             .out = nh_symoutptr(PowerTimer__fireAt) },
  { .name = "_ZNK10PowerTimer13timeRemainingEv",                               .out = nh_symoutptr(PowerTimer__timeRemaining) },

  { .name = "_ZN13MainNavButtonC1EP7QWidget",                .out = nh_symoutptr(MainNavButton__constructor),     .desc = "Home screen StoryGraph menu button", .optional = true },
  { .name = "_ZN13MainNavButton9setPixmapERK7QString",        .out = nh_symoutptr(MainNavButton__setPixmap),       .desc = "Home screen StoryGraph menu button", .optional = true },
  { .name = "_ZN13MainNavButton15setActivePixmapERK7QString", .out = nh_symoutptr(MainNavButton__setActivePixmap), .desc = "Home screen StoryGraph menu button", .optional = true },
  { .name = "_ZN13MainNavButton7setTextERK7QString",          .out = nh_symoutptr(MainNavButton__setText),         .desc = "Home screen StoryGraph menu button", .optional = true },

  {0},
};
// clang-format on

bool hardcover_uninstall() {
  nh_delete_file(Files::config);
  nh_delete_file(Files::settings);
  nh_delete_file(Files::cli);
  nh_delete_dir(Files::adds_directory);
  nh_delete_file(Files::nickelmenu_config_entry);

  return true;
}

// --- NickelMenu coexistence ---
//
// NickelMenu hooks the same MainNavView constructor we do to add its own tab
// bar button. NickelHook's GOT-patching has no chaining: whichever mod
// installs its hook last wins the slot outright and the other's button
// silently never appears. We detect NickelMenu (its hook symbol is visible
// process-wide once its plugin is loaded) and, when present, call its hook
// function directly instead of the true original - this runs its real
// button-adding logic (which itself calls through to the true original) and
// leaves us free to add our own button afterward, since both just land as
// separate widgets in the same tab bar layout.

static bool nickelMenuDetected = false;

// NickelMenu hooks the exact same MainNavView constructor symbol we do (its
// own tab bar button injection, "_nm_menu_hook2" in NickelMenu's source).
// NickelHook's GOT-patching has no chaining - only the mod whose library
// loads (and thus hooks) LAST is ever actually invoked by libnickel; the
// other mod's hook function is never called at all. Whenever nickelMenuDetected
// is true, we are deterministically the one libnickel calls (we load after
// NickelMenu alphabetically), and NickelMenu's own hook (and therefore its
// own button) would otherwise never run. Calling it ourselves - it calls
// through to the true original constructor itself before adding its button
// - restores it.
//
// Resolving "_nm_menu_hook2" isn't as simple as dlsym(RTLD_DEFAULT, ...):
// Nickel loads these .so files as generic Qt image-format plugins, and Qt's
// plugin loader dlopen()s them with the default RTLD_LOCAL (it only asks
// for RTLD_GLOBAL when QLibrary::ExportExternalSymbolsHint is set, which
// Nickel's plugin scan doesn't do). RTLD_LOCAL means NickelMenu's exported
// symbols are invisible to a dlsym(RTLD_DEFAULT, ...) call made from a
// different shared object, even once it's fully loaded - so that lookup
// always silently fails. Instead we reopen NickelMenu's library by its
// exact install path with RTLD_NOLOAD (glibc matches already-loaded objects
// by name before touching the filesystem, so this works even if NickelHook's
// own crash-failsafe has the file transiently renamed away) and resolve the
// symbol against that handle directly, which searches its symbol table
// regardless of RTLD_LOCAL.
typedef void (*NmMenuHook2)(QWidget *, QWidget *);
static NmMenuHook2 nmMenuHook2 = nullptr;

// --- NickelHardcover coexistence ---
//
// Same problem as NickelMenu above, but for NickelHardcover: this project was
// forked from it and still hooks the exact same ReadingController::setVolume
// and ReadingMenuView constructor symbols. libstorygraph.so loads after
// libhardcover.so alphabetically, so ours are the hooks libnickel calls and
// NickelHardcover's progress sync and reading menu button silently never run.
// When it's loaded, we call its hooks in place of the true originals (they
// call through to the true originals themselves), the same way we call
// nmMenuHook2 above.
//
// SupportedLocales is deliberately not chained: both mods' hooks insert the
// same "English (UK)" entry, so calling both would list it twice.
typedef void (*HcSetVolume)(ReadingController *, Volume *, Bookmark *);
static HcSetVolume hcSetVolume = nullptr;
#if NSG_QT6
// Qt 6 (firmware 5.18+) has a single ReadingMenuView(QWidget*, bool, bool)
// constructor; Qt 5's second QByteArray overload does not exist there.
typedef void (*HcReadingMenuViewConstructor)(ReadingMenuView *, QWidget *, bool, bool);
static HcReadingMenuViewConstructor hcReadingMenuViewConstructor = nullptr;
#else
typedef void (*HcReadingMenuViewConstructor)(ReadingMenuView *, QWidget *, bool);
typedef void (*HcReadingMenuViewConstructor2)(ReadingMenuView *, QWidget *, QByteArray const &, bool);
static HcReadingMenuViewConstructor hcReadingMenuViewConstructor = nullptr;
static HcReadingMenuViewConstructor2 hcReadingMenuViewConstructor2 = nullptr;
#endif

// The home screen tab bar button, cached so toggling "Show StoryGraph button
// on home screen" in Settings can show/hide it immediately rather than
// requiring Nickel to restart and reconstruct MainNavView.
static QPointer<QWidget> sgButtonWidget;

void setHomeButtonVisible(bool visible) {
  if (sgButtonWidget)
    sgButtonWidget->setVisible(visible);
}

// scaledNavIconPath sizes our tab bar icon to match Nickel's own native tab
// icons at runtime, instead of guessing per device tier from fixed PNGs.
// ":/images/home/main_nav_more.png" is Nickel's own built-in "more" tab
// icon - not a resource we own, but Qt's resource registry is global to the
// process once a .qrc is loaded, so it's visible from our .so too (same
// trick NickelMenu's nm_menu_pixmap() uses to size custom menu icons: see
// nickelmenu.cc's own reference to that exact path). Scaling our high-res
// master against its real size means we automatically match whatever the
// native tab icons actually look like on this device/firmware, rather than
// needing on-device measurements per tier.
//
// Returns an absolute file path MainNavButton__setPixmap can load, or an
// empty string on any failure (missing resource, write failure, etc.) - the
// caller falls back to the static nav_icon/nav_icon_hit assets in that case.
QString scaledNavIconPath() {
  QImage master;
  if (!master.load(Files::icon_master_outline)) {
    nh_log("scaledNavIconPath: could not load master icon");
    return QString();
  }

  QPixmap reference;
  if (!reference.load(QStringLiteral(":/images/home/main_nav_more.png"))) {
    nh_log("scaledNavIconPath: could not load native reference icon");
    return QString();
  }

  QImage scaled = master.scaled(reference.size(), Qt::KeepAspectRatio, Qt::SmoothTransformation);
  nh_log("scaledNavIconPath: scaled master to match native icon (%dx%d): %dx%d", reference.width(),
         reference.height(), scaled.width(), scaled.height());

  QString tempOut = QStringLiteral("/tmp/nickelstorygraph_nav_icon.png");
  if (!scaled.save(tempOut, "PNG")) {
    nh_log("scaledNavIconPath: could not save scaled icon to %s", qPrintable(tempOut));
    return QString();
  }

  return tempOut;
}

bool isTolinoDevice() {
#if NSG_QT6
  if (Device__getCurrentDevice && Device__isTolino) {
    Device *device = Device__getCurrentDevice();
    if (device)
      return Device__isTolino(device);
  }
#endif
  return false;
}

void checkForUpdates() {
  CLI *cli = CLI::checkUpdate();
  QObject::connect(cli, &CLI::response, [](QJsonObject doc) {
    bool upToDate = doc.value("up_to_date").toBool();
    QString current = doc.value("current").toString();
    QString latest = doc.value("latest").toString();

    ConfirmationDialog *dialog = ConfirmationDialogFactory__getConfirmationDialog(nullptr);
    ConfirmationDialog__setTitle(dialog, "StoryGraph");
    if (upToDate) {
      ConfirmationDialog__setText(dialog, t("You're up to date (%1).").arg(current));
    } else {
      ConfirmationDialog__setText(dialog,
          t("A new version is available: %1<br>You are running %2.").arg(latest, current));
    }
    ConfirmationDialog__setAcceptButtonText(dialog, "OK");
    ConfirmationDialog__showCloseButton(dialog, true);

    QObject::connect(dialog, &QDialog::accepted, dialog, &QObject::deleteLater);
    QObject::connect(dialog, &QDialog::rejected, dialog, &QObject::deleteLater);
    dialog->open();
  });
}

void showStorygraphHomeMenu(QWidget *anchor) {
  Settings *settings = Settings::getInstance();
  if (!settings->getHomeMenuEnabled()) return;

  QList<Item> items;
  if (settings->getHomeMenuReading())
    items.append({t("Currently Reading"), 1});
  if (settings->getHomeMenuFeed())
    items.append({t("Community Feed"), 2});
  if (settings->getHomeMenuGoals())
    items.append({t("Reading Goals"), 3});
  items.append({t("Sync all books"), 6});
  items.append({t("Check for updates"), 5});
  items.append({t("Settings"), 4});

  if (!anchor) return;

  // pos=3 + decoration off matches how Nickel/NickelMenu construct the home
  // screen tab bar's own popup: a plain rectangle, not the pointer/"speech
  // bubble" triangle used for popups anchored to a small toolbar icon.
  NickelTouchMenu *menu = construct_NickelTouchMenu(anchor, 3);
  NickelTouchMenu__showDecoration(menu, false);
  QWidget::connect(menu, &QMenu::aboutToHide, menu, &QWidget::deleteLater);

  for (const Item &item : items) {
    int val = item.value.toInt();

    MenuTextItem *mti = construct_MenuTextItem(menu, false, true);
    MenuTextItem__setText(mti, item.text);
    MenuTextItem__registerForTapGestures(mti);

    QWidgetAction *action = new QWidgetAction(menu);
    action->setDefaultWidget(mti);
    action->setEnabled(true);
    action->setData(item.value);
    menu->addAction(action);
    menu->addSeparator();

    QObject::connect(action, &QAction::triggered, [menu, val] {
      menu->hide();
      if (val == 1) ReadingDialog::show();
      else if (val == 2) FeedDialog::show();
      else if (val == 3) GoalsDialog::show();
      // No book is open on the home screen, so there's no contentId to show
      // sync/progress info for - suppress the t("Book information") section.
      else if (val == 4) SettingsDialog::show(true);
      else if (val == 5) checkForUpdates();
      else if (val == 6) SyncAll::start();
    });
    QObject::connect(mti, SIGNAL(tapped(bool)), action, SIGNAL(triggered()));
  }

  menu->ensurePolished();
  // Popup sits entirely above the anchor's top edge (negative Y), matching
  // NickelMenu's own tab bar popup — bottom-aligning against the anchor
  // (the old formula) covers the bottom nav bar whenever the anchor is the
  // full window rather than a small toolbar icon.
  QPoint pos = anchor->mapToGlobal(QPoint((anchor->width() - menu->sizeHint().width()) / 2, -menu->sizeHint().height()));
  menu->popup(pos);
}

// Firmware self-check: each build only runs on the firmware line it was compiled
// for. Kobo moved Nickel to Qt6 at firmware 5.18, and the hooked constructors
// and the calloc-based widget construction (construct_TouchLabel & co.) differ
// between the Qt5 and Qt6 ABIs. The Qt6 build refuses anything before 5.18; the
// Qt5 build refuses 5.18 and later. Firmware beyond what a build was measured
// on (Qt6: 5.19+/6.x, Qt5: 5.0-5.17) is accepted with a warning so a layout
// change is diagnosable instead of silently corrupting the heap. Returning
// nonzero from init makes NickelHook restore the hooks it just installed and
// park the plugin, so no calloc and no constructor ever runs on the wrong ABI.
//
// The version is read from /mnt/onboard/.kobo/version rather than the exported
// N3SettingsDeviceInformationController::viewFirmwareVersion symbol: a plain
// file read has no Qt/Nickel dependency and is safe this early in init. The
// file is a single comma-separated line - serial, kernel, firmware, ... - e.g.
// "T303440090606,4.9.77,5.18.264769,4.9.77,4.9.77,<model uuid>". The firmware
// is the third field; the kernel field also looks like "x.y.z", so the field
// is picked by position, not by shape. A line without commas is taken whole.
typedef enum {
  FW_INCOMPATIBLE, // the other Qt ABI, or an unreadable version: refuse.
  FW_SUPPORTED,    // the firmware line this build was tested on.
  FW_UNVERIFIED,   // same Qt ABI, but not tested: accepted with a warning.
} FirmwareVerdict;

static FirmwareVerdict firmwareVerdict() {
  FILE *f = fopen("/mnt/onboard/.kobo/version", "r");
  if (!f) {
    nh_log("firmware check: could not read /mnt/onboard/.kobo/version");
    return FW_INCOMPATIBLE;
  }

  char line[256] = {0};
  bool read = fgets(line, sizeof(line), f) != nullptr;
  fclose(f);

  if (!read) {
    nh_log("firmware check: /mnt/onboard/.kobo/version is empty");
    return FW_INCOMPATIBLE;
  }

  // Trim trailing newline/whitespace.
  char *end = line + strlen(line);
  while (end > line && (end[-1] == '\n' || end[-1] == '\r' || end[-1] == ' ' || end[-1] == '\t'))
    *--end = '\0';

  // Select the firmware field (the third one) when the line is comma-separated.
  char *buf = line;
  if (strchr(line, ',')) {
    for (int field = 0; field < 2 && buf; field++) {
      buf = strchr(buf, ',');
      if (buf)
        buf++;
    }
    if (!buf) {
      nh_log("firmware check: unrecognized version line '%s' - refusing to hook", line);
      return FW_INCOMPATIBLE;
    }
    if (char *comma = strchr(buf, ','))
      *comma = '\0';
  }

  int major = 0;
  int minor = 0;
  if (sscanf(buf, "%d.%d", &major, &minor) < 2) {
    nh_log("firmware check: unrecognized version '%s' - refusing to hook", buf);
    return FW_INCOMPATIBLE;
  }

  bool qt6Firmware = major > 5 || (major == 5 && minor >= 18);
#if NSG_QT6
  if (!qt6Firmware) {
    nh_log("firmware check: version '%s' is pre-5.18 (Qt5) - refusing to hook; this is the 5.18+ build", buf);
    return FW_INCOMPATIBLE;
  }
  if (major == 5 && minor == 18) {
    nh_log("firmware check: version '%s' -> supported", buf);
    return FW_SUPPORTED;
  }
  nh_log("firmware check: version '%s' is newer than 5.18 - hooking, but the object sizes are unverified for this layout", buf);
  return FW_UNVERIFIED;
#else
  if (qt6Firmware) {
    nh_log("firmware check: version '%s' is 5.18+ (Qt6) - refusing to hook; this is the 4.x build", buf);
    return FW_INCOMPATIBLE;
  }
  if (major == 4) {
    nh_log("firmware check: version '%s' -> supported", buf);
    return FW_SUPPORTED;
  }
  nh_log("firmware check: version '%s' is not 4.x - hooking, but this build is untested on it", buf);
  return FW_UNVERIFIED;
#endif
}

int nickelstorygraph_init() {
  if (firmwareVerdict() == FW_INCOMPATIBLE) {
    nh_log("nickelstorygraph_init: firmware doesn't match this build's Qt ABI - refusing to hook");
    return 1;
  }

  void *nmHandle = dlopen("/usr/local/Kobo/imageformats/libnm.so", RTLD_NOLOAD | RTLD_LAZY);
  if (nmHandle) {
    nmMenuHook2 = reinterpret_cast<NmMenuHook2>(dlsym(nmHandle, "_nm_menu_hook2"));
    dlclose(nmHandle);
  }
  nickelMenuDetected = nmMenuHook2 != nullptr;
  nh_log("nickelstorygraph_init: nickelMenuDetected=%s", nickelMenuDetected ? "true" : "false");

  void *hcHandle = dlopen("/usr/local/Kobo/imageformats/libhardcover.so", RTLD_NOLOAD | RTLD_LAZY);
  if (hcHandle) {
    hcSetVolume = reinterpret_cast<HcSetVolume>(dlsym(hcHandle, "_nh_ReadingController__setVolume"));
    hcReadingMenuViewConstructor =
        reinterpret_cast<HcReadingMenuViewConstructor>(dlsym(hcHandle, "_nh_ReadingMenuView__constructor"));
#if !NSG_QT6
    hcReadingMenuViewConstructor2 =
        reinterpret_cast<HcReadingMenuViewConstructor2>(dlsym(hcHandle, "_nh_ReadingMenuView__constructor_2"));
#endif
    dlclose(hcHandle);
  }
  nh_log("nickelstorygraph_init: hcSetVolume=%p hcReadingMenuViewConstructor=%p", (void *)hcSetVolume,
         (void *)hcReadingMenuViewConstructor);

  return 0;
}

NickelHook(.init = &nickelstorygraph_init, .info = &NickelStorygraph, .hook = NickelStorygraphHook,
           .dlsym = NickelStorygraphDlsym, .uninstall = &hardcover_uninstall);

QStackedWidget *stackedWidget = nullptr;

void handleStackedWidgetDestroyed() { stackedWidget = nullptr; }

extern "C" __attribute__((visibility("default"))) void
_nh_ReadingController__setVolume(ReadingController *_this, Volume *volume, Bookmark *bookmark) {
  nh_log("ReadingController::setVolume(%p, %p, %p)", _this, volume, bookmark);

  SyncController *syncController = SyncController::getInstance();
#if NSG_QT6
  syncController->contentId = Content__getId(volume, QString());
#else
  syncController->contentId = Content__getId(volume);
#endif
  syncController->title = Content__getTitle(volume);
  syncController->author = Content__getAttribution(volume);

  nh_log("ReadingController::setVolume contentId=%s", qPrintable(syncController->contentId));

  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);

  if (!stackedWidget) {
    QWidget *parent = cv ? cv->parentWidget() : nullptr;
    if (parent && QString(parent->metaObject()->className()) == "QStackedWidget") {
      stackedWidget = static_cast<QStackedWidget *>(parent);
      nh_log("ReadingController::setVolume connecting stackedWidget=%p", stackedWidget);
      QObject::connect(stackedWidget, &QStackedWidget::currentChanged, SyncController::getInstance(),
                       &SyncController::currentViewIndexChanged);
      QObject::connect(stackedWidget, &QObject::destroyed, handleStackedWidgetDestroyed);
    } else {
      nh_log("Error: expected QStackedWidget, got %s", parent ? parent->metaObject()->className() : "null");
    }
  }

  if (hcSetVolume) {
    // NickelHardcover's hook calls through to the true original itself - see
    // the NickelHardcover coexistence comment above.
    nh_log("ReadingController::setVolume calling NickelHardcover's hook");
    hcSetVolume(_this, volume, bookmark);
  } else {
    nh_log("ReadingController::setVolume calling original");
    ReadingController__setVolume(_this, volume, bookmark);
  }
  nh_log("ReadingController::setVolume original returned");
}

void injectMenuWidget(ReadingMenuView *parent) {
  QHBoxLayout *childLayout = parent->findChild<QHBoxLayout *>("bottomHorizontalLayout");
  QLabel *settingsIcon = parent->findChild<QLabel *>("settingsIcon");

  nh_log("injectMenuWidget: childLayout=%p settingsIcon=%p", childLayout, settingsIcon);

  if (childLayout && settingsIcon) {
    // The bare settings icon height reads oversized next to the other toolbar icons.
    int h = static_cast<int>(settingsIcon->height() * 0.65);
    nh_log("injectMenuWidget: creating MenuController height=%d", h);
    MenuController *ctl = new MenuController(h, parent);
    nh_log("injectMenuWidget: inserting icon");
    childLayout->insertWidget(childLayout->count() - 1, ctl->icon);
    nh_log("injectMenuWidget: done");
  } else {
    nh_log("Error: unable to find bottomHorizontalLayout and settingsIcon");
  }
}

#if NSG_QT6
extern "C" __attribute__((visibility("default"))) void
_nh_ReadingMenuView__constructor(ReadingMenuView *_this, QWidget *parent, bool unknown, bool unknown2) {
  nh_log("ReadingMenuView::ReadingMenuView(%p, %p, %s, %s)", _this, parent, unknown ? "true" : "false",
         unknown2 ? "true" : "false");
  if (hcReadingMenuViewConstructor)
    hcReadingMenuViewConstructor(_this, parent, unknown, unknown2);
  else
    ReadingMenuView__constructor(_this, parent, unknown, unknown2);
  nh_log("ReadingMenuView::ReadingMenuView original returned, injecting");
  injectMenuWidget(parent);
  nh_log("ReadingMenuView::ReadingMenuView inject done");
}

#else
extern "C" __attribute__((visibility("default"))) void _nh_ReadingMenuView__constructor(ReadingMenuView *_this,
                                                                                        QWidget *parent, bool unknown) {
  nh_log("ReadingMenuView::ReadingMenuView(%p, %p, %s)", _this, parent, unknown ? "true" : "false");
  if (hcReadingMenuViewConstructor)
    hcReadingMenuViewConstructor(_this, parent, unknown);
  else
    ReadingMenuView__constructor(_this, parent, unknown);
  nh_log("ReadingMenuView::ReadingMenuView original returned, injecting");
  injectMenuWidget(parent);
  nh_log("ReadingMenuView::ReadingMenuView inject done");
}

extern "C" __attribute__((visibility("default"))) void
_nh_ReadingMenuView__constructor_2(ReadingMenuView *_this, QWidget *parent, QByteArray const &unknownArray,
                                   bool unknownBool) {
  nh_log("ReadingMenuView::ReadingMenuView(%p, %p, %s)", _this, parent, unknownBool ? "true" : "false");
  if (hcReadingMenuViewConstructor2)
    hcReadingMenuViewConstructor2(_this, parent, unknownArray, unknownBool);
  else
    ReadingMenuView__constructor_2(_this, parent, unknownArray, unknownBool);
  injectMenuWidget(parent);
}

#endif

extern "C" __attribute__((visibility("default"))) void
_nh_MainNavView__constructor(MainNavView *_this, QWidget *parent) {
  nh_log("MainNavView::MainNavView(%p, %p) nickelMenuDetected=%s nmMenuHook2=%p", _this, parent,
         nickelMenuDetected ? "true" : "false", (void *)nmMenuHook2);

  if (nmMenuHook2) {
    // We're guaranteed to be the hook libnickel actually calls whenever
    // NickelMenu is detected (see nmMenuHook2 comment above) - call its own
    // hook instead of the true original so its own tab bar button still
    // gets added; it calls through to the true original itself.
    nmMenuHook2(_this, parent);
  } else {
    MainNavView__constructor(_this, parent);
  }

  // Adds our own tab bar button even when NickelMenu is present - nmMenuHook2
  // above already ran NickelMenu's real button-adding logic against this same
  // MainNavView/QHBoxLayout, so this is just a second widget landing in the
  // same layout, not a rematch over the hook slot.
  if (!MainNavButton__constructor || !MainNavButton__setPixmap || !MainNavButton__setActivePixmap || !MainNavButton__setText) {
    nh_log("_nh_MainNavView__constructor: missing MainNavButton symbols, skipping");
    return;
  }

  QHBoxLayout *bl = _this->findChild<QHBoxLayout *>();
  if (!bl) {
    nh_log("_nh_MainNavView__constructor: could not find QHBoxLayout in MainNavView");
    return;
  }

  MainNavButton *btn = reinterpret_cast<MainNavButton *>(calloc(1, 2048));
  if (!btn) return;

  MainNavButton__constructor(btn, parent);
  QString iconPath = scaledNavIconPath();
  MainNavButton__setPixmap(btn, iconPath.isEmpty() ? QString(Files::nav_icon) : iconPath);
  MainNavButton__setActivePixmap(btn, iconPath.isEmpty() ? QString(Files::nav_icon_hit) : iconPath);
  MainNavButton__setText(btn, "StoryGraph");
  btn->setObjectName("sgButton");

  // Adding a fifth/sixth button to the tab bar makes Nickel's own layout
  // shrink every button's label to keep them all fit, but "StoryGraph" is
  // still noticeably larger than the native labels next to it - find the
  // label the text landed on (by content, since we don't know its internal
  // object name) and knock it down a bit further to match.
  bool foundLabel = false;
  for (QLabel *label : btn->findChildren<QLabel *>()) {
    if (label->text() != "StoryGraph") continue;
    foundLabel = true;
    QFont f = label->font();
    if (f.pixelSize() > 0)
      f.setPixelSize(static_cast<int>(f.pixelSize() * 0.85));
    else if (f.pointSizeF() > 0)
      f.setPointSizeF(f.pointSizeF() * 0.85);
    label->setFont(f);
    break;
  }
  if (!foundLabel)
    nh_log("_nh_MainNavView__constructor: could not find StoryGraph label to shrink font");

  QPushButton *sh = new QPushButton(_this);
  sh->setVisible(false);

  if (!QWidget::connect(btn, SIGNAL(tapped()), sh, SIGNAL(pressed()))) {
    nh_log("_nh_MainNavView__constructor: could not connect tapped() signal");
    return;
  }

  QWidget::connect(sh, &QPushButton::pressed, [btn] { showStorygraphHomeMenu(btn); });

  // Built unconditionally (even if currently disabled) so setHomeButtonVisible()
  // can show/hide it live as the setting is toggled from Settings, without
  // needing Nickel to restart and reconstruct MainNavView.
  sgButtonWidget = btn;
  btn->setVisible(Settings::getInstance()->getHomeMenuEnabled());

  bl->addWidget(btn, 1);
  _this->ensurePolished();
}

extern "C" __attribute__((visibility("default"))) SupportedLocales *
_nh_SupportedLocales__supportedLocales(SupportedLocales *_this, bool b1) {
  SupportedLocales *items = SupportedLocales__supportedLocales(_this, b1);

  int index = 0;
  for (; index < items->count(); index++) {
    if (items->at(index).second == "en")
      break;
  }

  items->insert(index + 1, QPair<QString, QString>(QString("English (UK)"), QString("en_GB")));

  return items;
}
