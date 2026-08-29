# Commands own only this project's recorded development process group.
SHELL := /bin/bash
.PHONY: dev stop kill run build test check clean install ps

dev:
	node scripts/dev.mjs start

stop:
	node scripts/dev.mjs stop

kill: stop
run: dev

ps:
	@pgrep -fl "cooldown-bar|Cooldown Bar" || true

build:
	npm run tauri build

test:
	npm test
	cd src-tauri && cargo test

check:
	npx tsc --noEmit
	cd src-tauri && cargo clippy --all-targets -- -D warnings

install:
	npm install

clean: stop
	rm -rf dist
	cd src-tauri && cargo clean
