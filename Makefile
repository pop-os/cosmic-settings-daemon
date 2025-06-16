prefix ?= /usr
bindir = $(prefix)/bin
libdir = $(prefix)/lib
includedir = $(prefix)/include
sharedir = $(prefix)/share
geoclue_agent ?= /usr/libexec/geoclue-2.0/demos/agent

CARGO_TARGET_DIR ?= target
TARGET = debug
DEBUG ?= 0
ifeq ($(DEBUG),0)
	TARGET = release
	ARGS += --release
endif

VENDOR ?= 0
ifneq ($(VENDOR),0)
	ARGS += --frozen
endif

BIN = cosmic-settings-daemon
SYSTEM_ACTIONS_CONF = "$(DESTDIR)$(sharedir)/cosmic/com.system76.CosmicSettings.Shortcuts/v1/system_actions"
POLKIT_RULE = "$(DESTDIR)$(sharedir)/polkit-1/rules.d/cosmic-settings-daemon.rules"

all: $(BIN)

clean:
	rm -rf target

distclean: clean
	rm -rf .cargo vendor vendor.tar

$(BIN): Cargo.toml Cargo.lock src/main.rs vendor-check
	cargo build $(ARGS) --bin ${BIN}

install:
	install -Dm0755 "$(CARGO_TARGET_DIR)/$(TARGET)/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"
	install -Dm0644 "data/system_actions.ron" "$(SYSTEM_ACTIONS_CONF)"
	install -Dm0644 "data/polkit-1/rules.d/cosmic-settings-daemon.rules" "$(POLKIT_RULE)"

## Cargo Vendoring

vendor:
	rm .cargo -rf
	mkdir -p .cargo
	cargo vendor | head -n -1 > .cargo/config
	echo 'directory = "vendor"' >> .cargo/config
	tar cf vendor.tar vendor
	rm -rf vendor

vendor-check:
ifeq ($(VENDOR),1)
	rm vendor -rf && tar xf vendor.tar
endif
