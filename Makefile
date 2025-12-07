.PHONY: build release install uninstall clean test check lint

# Installation directory (can override: make install PREFIX=~/.local)
PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin

# Binary name
BINARY := coderag
TARGET := target/release/$(BINARY)

# Default target
all: release

# Debug build
build:
	cargo build

# Release build
release:
	cargo build --release
	@echo "Built: $(TARGET)"
	@ls -lh $(TARGET)

# Install to PREFIX/bin
install: release
	@mkdir -p $(BINDIR)
	@cp $(TARGET) $(BINDIR)/$(BINARY)
	@chmod +x $(BINDIR)/$(BINARY)
	@echo "Installed: $(BINDIR)/$(BINARY)"
	@echo ""
	@echo "Make sure $(BINDIR) is in your PATH:"
	@echo '  export PATH="$$PATH:$(BINDIR)"'

# Uninstall
uninstall:
	@rm -f $(BINDIR)/$(BINARY)
	@echo "Removed: $(BINDIR)/$(BINARY)"

# Run tests
test:
	cargo test

# Check compilation
check:
	cargo check

# Lint with clippy
lint:
	cargo clippy -- -D warnings

# Format code
fmt:
	cargo fmt

# Clean build artifacts
clean:
	cargo clean

# Build and strip binary (smaller size)
release-strip: release
	strip $(TARGET)
	@echo "Stripped: $(TARGET)"
	@ls -lh $(TARGET)

# Install stripped binary
install-strip: release-strip
	@mkdir -p $(BINDIR)
	@cp $(TARGET) $(BINDIR)/$(BINARY)
	@chmod +x $(BINDIR)/$(BINARY)
	@echo "Installed (stripped): $(BINDIR)/$(BINARY)"

# Show help
help:
	@echo "CodeRAG Makefile"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build         Debug build"
	@echo "  release       Release build"
	@echo "  release-strip Release build + strip symbols"
	@echo "  install       Install to $(BINDIR)"
	@echo "  install-strip Install stripped binary"
	@echo "  uninstall     Remove from $(BINDIR)"
	@echo "  test          Run tests"
	@echo "  check         Check compilation"
	@echo "  lint          Run clippy"
	@echo "  fmt           Format code"
	@echo "  clean         Clean build artifacts"
	@echo ""
	@echo "Variables:"
	@echo "  PREFIX        Installation prefix (default: ~/.local)"
	@echo ""
	@echo "Examples:"
	@echo "  make install                    # Install to ~/.local/bin"
	@echo "  make install PREFIX=/usr/local  # Install to /usr/local/bin"
	@echo "  make install-strip              # Install smaller binary"
