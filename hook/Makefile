NH_QT_MAJOR ?= 6

include ./NickelHook/NickelHook.mk

override LIBRARY  := libstorygraph.so
override SOURCES  += $(wildcard src/*.cc src/**/*.cc)
override CFLAGS   += -Wall -Wextra -Werror -Wno-missing-field-initializers -fvisibility=hidden
override CXXFLAGS += -Wall -Wextra -Werror -Wno-missing-field-initializers -isystemminiz -fvisibility=hidden -fvisibility-inlines-hidden
override PKGCONF  += Qt$(NH_QT_MAJOR)Widgets Qt$(NH_QT_MAJOR)Network
override MOCS     += $(wildcard src/*.h src/**/*.h)
override QRCS     += res/nickelstorygraph.qrc
override GENERATED += .version-stamp

include ./NickelHook/NickelHook.mk

# NH_VERSION (git describe) and NH_QT_MAJOR reach the build through CPPFLAGS
# and the toolchain choice, which make doesn't track: after a new tag, or when
# switching between the Qt5 and Qt6 builds, unchanged sources would keep stale
# objects. Rewrite the stamp whenever either changes and make every generated
# file depend on it, so the hook is rebuilt.
ifneq ($(filter-out clean gitignore,$(or $(MAKECMDGOALS),all)),)
$(shell printf '%s\n' '$(VERSION) qt$(NH_QT_MAJOR)' | cmp -s - .version-stamp || printf '%s\n' '$(VERSION) qt$(NH_QT_MAJOR)' > .version-stamp)
endif
$(OBJECTS_C) $(OBJECTS_CXX) $(OBJECTS_CXX1) $(MOCS_MOC) $(OBJECTS_MOC) $(RCCS_RCC) $(OBJECTS_RCC): .version-stamp
