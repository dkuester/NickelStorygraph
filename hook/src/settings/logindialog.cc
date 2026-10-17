#include <QJsonDocument>
#include <QJsonObject>
#include <QLineEdit>
#include <QVBoxLayout>

#include <NickelHook.h>

#include "../cli.h"
#include "../settings.h"
#include "../translations.h"
#include "logindialog.h"

void LoginDialog::show() { new LoginDialog(); }

LoginDialog::LoginDialog() : Dialog(t("Log in")) {
  QVBoxLayout *layout = new QVBoxLayout(this);

  emailEdit = construct_TouchLineEdit(nullptr);
  emailEdit->setPlaceholderText(t("Email address"));
  layout->addWidget(emailEdit);

  passwordEdit = construct_TouchLineEdit(nullptr);
  passwordEdit->setPlaceholderText(t("Password"));
  passwordEdit->setEchoMode(QLineEdit::Password);
  layout->addWidget(passwordEdit);

  buildKeyboardFrame(emailEdit, t("Log in"));
  buildKeyboardFrame(passwordEdit, t("Log in"));
  showKeyboard();
}

void LoginDialog::commit() {
  if (submitted) {
    return;
  }
  submitted = true;

  nh_log("LoginDialog::commit()");

  QString email = emailEdit->text().trimmed();
  QString password = passwordEdit->text();

  if (email.isEmpty() || password.isEmpty()) {
    submitted = false;
    return;
  }

  CLI *cli = CLI::login(email, password);
  QObject::connect(cli, &CLI::response, this, [this](QJsonObject obj) {
    Settings *settings = Settings::getInstance();
    settings->setSessionCookie(obj.value("session_cookie").toString());
    settings->setRememberToken(obj.value("remember_user_token").toString());
  });
  QObject::connect(cli, &CLI::success, dialog, &QDialog::deleteLater);
  QObject::connect(cli, &CLI::failure, dialog, &QDialog::deleteLater);
}
