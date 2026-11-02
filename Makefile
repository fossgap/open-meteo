PREFIX ?= /usr/local
APP_ID = io.github.open_meteo.OpenMeteo

.PHONY: run build test check check-ui check-packages install uninstall deb rpm appimage arch arch-source

run:
	cargo run --locked

build:
	cargo build --release --locked

test:
	cargo test --locked --workspace --features open-meteo-checks/desktop

check:
	cargo fmt --all --check
	cargo clippy --locked --workspace --all-targets --features open-meteo-checks/desktop -- -D warnings
	cargo test --locked --workspace --features open-meteo-checks/desktop

check-ui:
	sh scripts/check-ui.sh

check-packages:
	sh scripts/verify-packages.sh dist/release

install:
	install -Dm755 target/release/open-meteo "$(DESTDIR)$(PREFIX)/bin/open-meteo"
	install -Dm644 resources/$(APP_ID).desktop "$(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop"
	install -Dm644 resources/$(APP_ID).svg "$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg"
	install -Dm644 README.md "$(DESTDIR)$(PREFIX)/share/doc/open-meteo/README.md"
	install -Dm644 packaging/copyright "$(DESTDIR)$(PREFIX)/share/doc/open-meteo/copyright"
	install -Dm644 LICENSE "$(DESTDIR)$(PREFIX)/share/licenses/open-meteo/LICENSE"
	install -Dm644 vendor/ksni/UNLICENSE "$(DESTDIR)$(PREFIX)/share/licenses/open-meteo/ksni-UNLICENSE"
	for file in resources/gnome-extension/*; do install -Dm644 "$$file" "$(DESTDIR)$(PREFIX)/share/gnome-shell/extensions/open-meteo-clock@fossgap.github.io/$$(basename "$$file")"; done

uninstall:
	rm -f "$(DESTDIR)$(PREFIX)/bin/open-meteo"
	rm -f "$(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop"
	rm -f "$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg"
	rm -f "$(DESTDIR)$(PREFIX)/share/doc/open-meteo/README.md"
	rm -f "$(DESTDIR)$(PREFIX)/share/doc/open-meteo/copyright" "$(DESTDIR)$(PREFIX)/share/licenses/open-meteo/LICENSE" "$(DESTDIR)$(PREFIX)/share/licenses/open-meteo/ksni-UNLICENSE"
	rm -f "$(DESTDIR)$(PREFIX)/share/gnome-shell/extensions/open-meteo-clock@fossgap.github.io/metadata.json" "$(DESTDIR)$(PREFIX)/share/gnome-shell/extensions/open-meteo-clock@fossgap.github.io/extension.js" "$(DESTDIR)$(PREFIX)/share/gnome-shell/extensions/open-meteo-clock@fossgap.github.io/stylesheet.css"

deb rpm appimage arch arch-source:
	./scripts/package.sh $@
