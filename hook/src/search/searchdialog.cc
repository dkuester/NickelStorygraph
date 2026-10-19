#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QScreen>
#include <QVBoxLayout>

#include <NickelHook.h>

#include "../cli.h"
#include "../editions/editionsdialog.h"
#include "../settings.h"
#include "bookrow.h"
#include "searchdialog.h"
#include "../translations.h"

void SearchDialog::show(QString contentId, QString query) { new SearchDialog(contentId, query); }

SearchDialog::SearchDialog(QString contentId, QString query) : Dialog(t("Manually link book")), contentId(contentId) {
  setStyleSheet(R"(
    [qApp_deviceIsTrilogy=true] TouchLineEdit {
      margin: 0 20px;
    }
    [qApp_deviceIsPhoenix=true] TouchLineEdit {
      margin: 0 25px;
    }
    [qApp_deviceIsDragon=true] TouchLineEdit {
      margin: 0 35px;
    }
    [qApp_deviceIsStorm=true] TouchLineEdit {
      margin: 0 40px;
    }
    [qApp_deviceIsDaylight=true] TouchLineEdit {
      margin: 0 45px;
    }

    [qApp_deviceIsTrilogy=true] QStackedWidget {
      margin: 0 20px;
    }
    [qApp_deviceIsPhoenix=true] QStackedWidget {
      margin: 0 24px;
    }
    [qApp_deviceIsDragon=true] QStackedWidget {
      margin: 0 37px;
    }
    [qApp_deviceIsStorm=true] QStackedWidget {
      margin: 0 42px;
    }
    [qApp_deviceIsDaylight=true] QStackedWidget {
      margin: 0 48px;
    }
  )");

  setFixedSize(parentWidget()->size());

  QVBoxLayout *layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(0);

  lineEdit = construct_TouchLineEdit(nullptr);
  lineEdit->setText(query);
  layout->addWidget(lineEdit);

  pages = new PagedStack(this);
  layout->addWidget(pages, 1);
  QObject::connect(pages, &PagedStack::requestPage, this, &SearchDialog::requestPage);
  QObject::connect(pages, &PagedStack::afterLayout, this, &SearchDialog::commit);

  buildKeyboardFrame(lineEdit, t("Search"));
}

void SearchDialog::commit() {
  QObject::disconnect(pages, &PagedStack::afterLayout, this, &SearchDialog::commit);

  pages->clear();
  pages->next();
}

void SearchDialog::requestPage(int index) {
  currentPage = index;
  QString query = lineEdit->text();

  BookRow *dummy = new BookRow(QJsonObject(), this);
  int limit = pages->getAvailableHeight() / dummy->sizeHint().height();
  dummy->deleteLater();

  CLI *cli = CLI::search(query, limit, index);
  QObject::connect(cli, &CLI::response, this, &SearchDialog::response);
}

void SearchDialog::response(QJsonObject doc) {
  QJsonArray resultsArray = doc.value("results").toArray();
  int length = resultsArray.size();

  if (length < 1) {
    pages->setStatusText(t("No results found."));
    return;
  }

  QWidget *box = new QWidget(pages);
  QVBoxLayout *results = new QVBoxLayout(box);
  results->setContentsMargins(0, 0, 0, 0);
  results->setSpacing(0);

  for (int i = 0; i < length; i++) {
    BookRow *row = new BookRow(resultsArray.at(i).toObject());
    if (i == 0) {
      row->setProperty("noBorder", true);
    }

    results->addWidget(row);

    QObject::connect(row, &BookRow::editions, this, &SearchDialog::editions);
    QObject::connect(row, &BookRow::selected, this, &SearchDialog::selected);
  }

  results->addStretch(1);
  pages->addPage(box);

  // The CLI reports whether rows follow this slice; map that onto the total
  // page count. `has_more` → total 0 means "unknown, keep offering next"
  // (PagedStack treats total <= 0 as paging-forward-enabled); the last page
  // sets total to its own index so the next button hides and the footer reads
  // "Page X of X". Called after addPage() so PagedStack::current is the page
  // just built.
  bool hasMore = doc.value("has_more").toBool();
  pages->setTotal(hasMore ? 0 : currentPage);
}

void SearchDialog::editions(QString id) {
  nh_log("SearchDialog::editions(%s)", qPrintable(id));

  EditionsDialog *editions = EditionsDialog::show(id);
  QObject::connect(editions, &EditionsDialog::closed, dialog, &SearchDialog::deleteLater);
  QObject::connect(editions, &EditionsDialog::selected, this, &SearchDialog::selected);
}

void SearchDialog::selected(QString id) {
  nh_log("SearchDialog::selected(%s)", qPrintable(id));
  Settings::getInstance()->setLinkedId(contentId, id);
  dialog->deleteLater();
}
