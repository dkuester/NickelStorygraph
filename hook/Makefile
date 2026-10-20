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

# NH_VERSION (git describe) is baked in through CPPFLAGS, which make doesn't
# track: after a new tag or commit, unchanged sources would keep the old
# version string. Rewrite the stamp whenever the version changes and make every
# object depend on it, so a version change rebuilds the hook.
ifneq ($(filter-out clean gitignore,$(or $(MAKECMDGOALS),all)),)
$(shell printf '%s\n' '$(VERSION)' | cmp -s - .version-stamp || printf '%s\n' '$(VERSION)' > .version-stamp)
endif
$(OBJECTS_C) $(OBJECTS_CXX) $(OBJECTS_CXX1): .version-stamp
