_default:
    @just --list

ci_mode := env_var_or_default("CI", "")
locked := if ci_mode != "" { "--locked" } else { "" }
fmt_mode := if ci_mode != "" { "--check" } else { "" }

check:
    cargo check --all-targets

fmt:
    cargo fmt {{fmt_mode}}

lint:
    cargo clippy --all-targets {{locked}} -- -D warnings

test:
    cargo test {{locked}}

build:
    cargo build {{locked}}

run:
    cargo run

audit:
    cargo deny check advisories
    cargo deny check licenses

ci: check fmt lint test audit

all: ci build

bundle:
    ./scripts/build-macos-bundle.sh

icon png="resources/icon.png":
    ./scripts/make-icon.sh {{png}}
