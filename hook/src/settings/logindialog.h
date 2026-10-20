#pragma once

#include "../nickelstorygraph.h"
#include "../widgets/dialog.h"

class LoginDialog : public Dialog {
  Q_OBJECT

public:
  static void show();

  void commit() override;

private:
  LoginDialog();

  TouchLineEdit *emailEdit = nullptr;
  TouchLineEdit *passwordEdit = nullptr;
  bool submitted = false;
};
