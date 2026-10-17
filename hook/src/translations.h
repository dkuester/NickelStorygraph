#pragma once

#include <QString>

// Central translation table for the NickelStorygraph UI.
//
// Source strings are English (the literal text in the code). German
// translations live in translations.cc. The active language is auto-detected
// from QLocale::system() (the Nickel/Kobo locale) — "de" yields German,
// anything else yields the English source — and can be overridden by calling
// setLanguage().

// Returns the translated string for the given English source text.
QString t(const char *source);

// QString overload (for translating values that are already QString).
QString t(const QString &source);

// Overrides the auto-detected language (e.g. "de" or "en"). Pass an empty
// string to revert to auto-detection.
void setLanguage(const QString &lang);
