PREFIX ?= /usr
DESTDIR ?=
BUILD_MODE ?= release

APP_ID := io.github.erayq1.Veyra
BINARY := veyra

BINDIR := $(DESTDIR)$(PREFIX)/bin
APPDIR := $(DESTDIR)$(PREFIX)/share/applications
METAINFODIR := $(DESTDIR)$(PREFIX)/share/metainfo
ICONDIR := $(DESTDIR)$(PREFIX)/share/icons/hicolor

CARGO_FLAGS := --release --locked

.PHONY: all build install uninstall clean

all: build

build:
	cargo build $(CARGO_FLAGS) --workspace

install: build
	install -Dm755 target/$(BUILD_MODE)/$(BINARY) $(BINDIR)/$(BINARY)
	install -Dm644 data/$(APP_ID).desktop $(APPDIR)/$(APP_ID).desktop
	install -Dm644 data/$(APP_ID).metainfo.xml $(METAINFODIR)/$(APP_ID).metainfo.xml
	find data/icons/hicolor -type f | while read -r icon; do \
		dest=$(ICONDIR)/$${icon#data/icons/hicolor/}; \
		install -Dm644 "$$icon" "$$dest"; \
	done

uninstall:
	rm -f $(BINDIR)/$(BINARY)
	rm -f $(APPDIR)/$(APP_ID).desktop
	rm -f $(METAINFODIR)/$(APP_ID).metainfo.xml
	find data/icons/hicolor -type f | while read -r icon; do \
		rm -f $(ICONDIR)/$${icon#data/icons/hicolor/}; \
	done

clean:
	cargo clean
