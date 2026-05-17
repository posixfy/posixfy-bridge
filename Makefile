.PHONY: all build test lint clean dev docker docker-up docker-down

all: build

build:
	cargo build --release

test:
	cargo test

lint:
	cargo fmt -- --check
	cargo clippy -- -D warnings

docker:
	docker compose build

docker-up:
	docker compose up -d

docker-down:
	docker compose down

dev:
	cargo run

clean:
	rm -rf target
