#include "translations.h"

#include <QHash>
#include <QLocale>

namespace {

QString g_overrideLanguage;  // empty = auto-detect

QString currentLanguage() {
  if (!g_overrideLanguage.isEmpty()) {
    return g_overrideLanguage;
  }
  // "de_DE" → "de", "en_US" → "en", etc.
  return QLocale::system().name().left(2).toLower();
}

struct Entry {
  const char *en;
  const char *de;
};

// German translations, keyed by the exact English source string.
const Entry kDe[] = {
  {"A mix", "Eine Mischung"},
  {"Add New Journal Entry", "Neuer Journaleintrag"},
  {"Added to list <i>%1</i>", "Zur Liste <i>%1</i> hinzugefügt"},
  {"Advanced", "Erweitert"},
  {"After closing a book or the Kobo is put to sleep", "Nach dem Schließen eines Buchs oder dem Ruhezustand"},
  {"All", "Alle"},
  {"Already on your shelf", "Bereits auf deinem Regal"},
  {"Always", "Immer"},
  {"Annotations", "Markierungen"},
  {"Answered prompt <i>%1</i>", "Aufforderung <i>%1</i> beantwortet"},
  {"Auto-sync", "Auto-Sync"},
  {"Auto-sync scheduled for", "Auto-Sync geplant für"},
  {"Authorized user", "Angemeldeter Benutzer"},
  {"BOOK PROPERTIES", "BUCH-EIGENSCHAFTEN"},
  {"Back", "Zurück"},
  {"Book Complete!", "Buch beendet!"},
  {"Book complete!", "Buch beendet!"},
  {"Book information", "Buchinformationen"},
  {"Book management", "Buchverwaltung"},
  {"Books", "Bücher"},
  {"Cancel", "Abbrechen"},
  {"Character", "Charakter"},
  {"Character flaws main focus?", "Charakterschwächen im Fokus?"},
  {"Characters loveable?", "Liebenswerte Charaktere?"},
  {"Clear", "Löschen"},
  {"Community Feed", "Community-Feed"},
  {"Complicated", "Kompliziert"},
  {"Country", "Land"},
  {"Current progress", "Aktueller Fortschritt"},
  {"Current streak", "Aktuelle Serie"},
  {"Currently Reading", "Lese ich gerade"},
  {"Data Score", "Datenqualität"},
  {"Debug logs", "Debug-Protokolle"},
  {"Did Not Finish", "Abgebrochen"},
  {"Disable auto-sync", "Auto-Sync deaktivieren"},
  {"Diverse cast?", "Diverse Besetzung?"},
  {"Done", "Fertig"},
  {"E-Book", "E-Book"},
  {"Email address", "E-Mail-Adresse"},
  {"Edition is already up to date.", "Ausgabe ist bereits aktuell."},
  {"Edition updated from StoryGraph.", "Ausgabe von StoryGraph aktualisiert."},
  {"Editions", "Ausgaben"},
  {"Enable auto-sync", "Auto-Sync aktivieren"},
  {"Enable auto-sync by default", "Auto-Sync standardmäßig aktivieren"},
  {"Enter a value", "Wert eingeben"},
  {"Failed to load books.", "Bücher konnten nicht geladen werden."},
  {"Failed to load community feed.", "Community-Feed konnte nicht geladen werden."},
  {"Failed to load reading goals.", "Leseziele konnten nicht geladen werden."},
  {"Fast", "Schnell"},
  {"Finished", "Beendet"},
  {"Finished reading", "Lese beendet"},
  {"Format", "Format"},
  {"General", "Allgemein"},
  {"Home menu", "Startbildschirm-Menü"},
  {"Hours", "Stunden"},
  {"Information", "Information"},
  {"ISBN 10", "ISBN 10"},
  {"ISBN 13", "ISBN 13"},
  {"It's complicated", "Es ist kompliziert"},
  {"Keep StoryGraph progress", "StoryGraph-Fortschritt behalten"},
  {"Language", "Sprache"},
  {"Last synced", "Zuletzt synchronisiert"},
  {"Let StoryGraph know you finished: Write a review, or update your reading status?", "StoryGraph mitteilen, dass du fertig bist: Rezension schreiben oder Lesestatus aktualisieren?"},
  {"Link book", "Buch verknüpfen"},
  {"Log in", "Anmelden"},
  {"Loading. Please wait...", "Wird geladen. Bitte warten..."},
  {"Loading...", "Wird geladen..."},
  {"Longest streak", "Längste Serie"},
  {"MOODS", "STIMMUNGEN"},
  {"Manual only", "Nur manuell"},
  {"Manually link book", "Buch manuell verknüpfen"},
  {"Mark as finished", "Als beendet markieren"},
  {"Marked as Read", "Als gelesen markiert"},
  {"Medium", "Mittel"},
  {"N/A", "k. A."},
  {"Never", "Nie"},
  {"No", "Nein"},
  {"No activity in your feed.", "Keine Aktivität in deinem Feed."},
  {"No annotations found.", "Keine Markierungen gefunden."},
  {"No data", "Keine Daten"},
  {"No editions found.", "Keine Ausgaben gefunden."},
  {"No reading goals set for this year.", "Keine Leseziele für dieses Jahr festgelegt."},
  {"No results found.", "Keine Ergebnisse gefunden."},
  {"Nothing here.", "Nichts hier."},
  {"Off", "Aus"},
  {"On", "Ein"},
  {"On finish", "Bei Abschluss"},
  {"Once per day", "Einmal täglich"},
  {"Open reading journal", "Lesejournal öffnen"},
  {"Pace", "Tempo"},
  {"Page %1", "Seite %1"},
  {"Page %1 of %2", "Seite %1 von %2"},
  {"Pages", "Seiten"},
  {"Paused", "Pausiert"},
  {"Paused (%1)", "Pausiert (%1)"},
  {"Password", "Passwort"},
  {"Periodically by read percentage", "Periodisch nach Lesefortschritt"},
  {"Plot", "Handlung"},
  {"Plot or character driven?", "Handlungs- oder charaktergetrieben?"},
  {"Print", "Gedruckt"},
  {"Prompt to review or mark finished on completion", "Nach Abschluss zum Bewerten oder Abschließen auffordern"},
  {"Publisher:", "Verlag:"},
  {"Rated ", "Bewertet mit "},
  {"Read", "Gelesen"},
  {"Readers", "Leser"},
  {"Reading", "Lesen"},
  {"Reading (%1)", "Lesen (%1)"},
  {"Reading Goals", "Leseziele"},
  {"Reading Journal", "Lesejournal"},
  {"Reading Streak", "Leseserie"},
  {"Reading progress must be at least 1% to sync with StoryGraph", "Der Lesefortschritt muss mindestens 1 % betragen, um mit StoryGraph zu synchronisieren"},
  {"Release Date", "Erscheinungsdatum"},
  {"Review Saved!", "Rezension gespeichert!"},
  {"Reviewed", "Rezensiert"},
  {"Save", "Speichern"},
  {"Save system logs", "Systemprotokolle speichern"},
  {"Saved a Note", "Notiz gespeichert"},
  {"Saved a Quote", "Zitat gespeichert"},
  {"Saved as Want To Read", "Als „Möchte ich lesen“ gespeichert"},
  {"Search", "Suchen"},
  {"Select book", "Buch auswählen"},
  {"Select edition", "Ausgabe auswählen"},
  {"Set", "Setzen"},
  {"Set a threshold", "Schwellenwert festlegen"},
  {"Set time of day", "Uhrzeit festlegen"},
  {"Settings", "Einstellungen"},
  {"Share your thoughts about this book with the world.", "Teile deine Gedanken zu diesem Buch mit der Welt."},
  {"Show StoryGraph button on home screen", "StoryGraph-Button auf dem Startbildschirm anzeigen"},
  {"Simple review (rating and thoughts only)", "Einfache Rezension (nur Bewertung und Gedanken)"},
  {"Slow", "Langsam"},
  {"Started reading", "Lese begonnen"},
  {"Stopped reading", "Lese abgebrochen"},
  {"Streak", "Serie"},
  {"Strong character dev?", "Starke Charakterentwicklung?"},
  {"Submit", "Absenden"},
  {"Submit review", "Rezension absenden"},
  {"Success!", "Erfolg!"},
  {"Sync Now", "Jetzt synchronisieren"},
  {"Sync annotations to reading journal", "Markierungen ins Lesejournal übertragen"},
  {"Sync edition from web", "Ausgabe aus dem Web synchronisieren"},
  {"Sync from Kobo", "Vom Kobo synchronisieren"},
  {"Sync now", "Jetzt synchronisieren"},
  {"Syncing annotations with StoryGraph...", "Markierungen werden mit StoryGraph synchronisiert..."},
  {"Syncing with StoryGraph...", "Wird mit StoryGraph synchronisiert..."},
  {"Type", "Typ"},
  {"Unknown", "Unbekannt"},
  {"Unknown journal type ", "Unbekannter Journaltyp "},
  {"Unlink book", "Verknüpfung aufheben"},
  {"Unset", "Zurücksetzen"},
  {"Update book status", "Buchstatus aktualisieren"},
  {"Updated progress from %1% → %2%", "Fortschritt von %1% → %2% aktualisiert"},
  {"Version", "Version"},
  {"Want to Read", "Möchte ich lesen"},
  {"Write a new note...", "Neue Notiz schreiben..."},
  {"Write a review", "Rezension schreiben"},
  {"Write your review", "Schreibe deine Rezension"},
  {"Would you like to update this book's reading status on Storygraph?", "Möchtest du den Lesestatus dieses Buchs auf StoryGraph aktualisieren?"},
  {"Would you like to write a review?", "Möchtest du eine Rezension schreiben?"},
  {"YOUR THOUGHTS", "DEINE GEDANKEN"},
  {"Yes", "Ja"},
  {"Your StoryGraph shelf has this book at %1%. Keep that progress?", "Dein StoryGraph-Regal hat dieses Buch bei %1%. Fortschritt behalten?"},
  {"<year> Reading Goals", "Leseziele <year>"},

  // Update check.
  {"A new version is available: %1<br>You are running %2.", "Eine neue Version ist verfügbar: %1<br>Du verwendest %2."},
  {"Check for updates", "Nach Updates suchen"},
  {"You're up to date (%1).", "Du bist auf dem neuesten Stand (%1)."},

  // Review moods (displayed in the review dialog).
  {"adventurous", "abenteuerlich"},
  {"challenging", "herausfordernd"},
  {"dark", "düster"},
  {"emotional", "emotional"},
  {"funny", "lustig"},
  {"hopeful", "hoffnungsvoll"},
  {"informative", "informativ"},
  {"inspiring", "inspirierend"},
  {"lighthearted", "unbeschwert"},
  {"mysterious", "geheimnisvoll"},
  {"reflective", "nachdenklich"},
  {"relaxing", "entspannend"},
  {"sad", "traurig"},
  {"tense", "spannungsgeladen"},
};

const QHash<QString, QString> &de() {
  static const QHash<QString, QString> map = [] {
    QHash<QString, QString> m;
    m.reserve(sizeof(kDe) / sizeof(kDe[0]));
    for (const Entry &e : kDe) {
      m.insert(QString::fromUtf8(e.en), QString::fromUtf8(e.de));
    }
    return m;
  }();
  return map;
}

}  // namespace

static QString translate(const QString &source) {
  if (currentLanguage() == "de") {
    auto it = de().constFind(source);
    if (it != de().constEnd()) {
      return *it;
    }
  }
  return source;
}

QString t(const char *source) {
  return translate(QString::fromUtf8(source));
}

QString t(const QString &source) {
  return translate(source);
}

void setLanguage(const QString &lang) {
  g_overrideLanguage = lang.trimmed().left(2).toLower();
}