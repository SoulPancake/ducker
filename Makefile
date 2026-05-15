.PHONY: build run watch clean check help

# Build the release binary
build:
	@echo "🦆 Building Ducker (release)..."
	cargo build --release
	@echo "✅ Build complete! Binary at ./target/release/ducker"

# Run the compiled binary
run: build
	@echo "🦆 Running Ducker..."
	./target/release/ducker

# Watch mode: auto-rebuild and run on file changes
watch:
	@echo "👀 Watching for changes..."
	@command -v cargo-watch >/dev/null 2>&1 || (echo "📦 Installing cargo-watch..." && cargo install cargo-watch)
	cargo watch -x "build --release" -x "run --release"

# Quick check without full build
check:
	@echo "🔍 Running checks..."
	cargo clippy --all-targets --all-features

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	@echo "✅ Clean complete!"

# Development build (debug, faster)
dev:
	@echo "⚡ Building Ducker (debug)..."
	cargo build
	./target/debug/ducker

# Watch debug build
watch-dev:
	@echo "👀 Watching (debug mode)..."
	@command -v cargo-watch >/dev/null 2>&1 || (echo "📦 Installing cargo-watch..." && cargo install cargo-watch)
	cargo watch -x "build" -x "run"

# Help menu
help:
	@echo "🦆 Ducker Makefile Commands:"
	@echo ""
	@echo "  make build       - Build release binary"
	@echo "  make run         - Build and run release binary"
	@echo "  make watch       - Auto-rebuild & run on file changes (release mode)"
	@echo "  make dev         - Build and run debug binary (faster compile)"
	@echo "  make watch-dev   - Auto-rebuild & run on file changes (debug mode)"
	@echo "  make check       - Run clippy checks"
	@echo "  make clean       - Remove build artifacts"
	@echo "  make help        - Show this help menu"
	@echo ""

# Default target
.DEFAULT_GOAL := help

