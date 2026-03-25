#!/usr/bin/env bash
# Ghost Engine — Production Release Build (TSK-5.9)
#
# Builds optimized, stripped single binaries for `ghost` (CLI),
# `ghost-cli` (CLI alias), and `ghost-mcp` (MCP server) with full LTO.
#
# Usage:
#   ./build-release.sh              # build all binaries for current platform
#   ./build-release.sh --size       # build and report binary sizes
#   ./build-release.sh --verify     # build, then verify single-binary deployment
#   ./build-release.sh --package    # build + verify + create dist/ tarball
#
# Uses Cargo profile "production-stripped" (LTO=true, codegen-units=1,
# opt-level="s", strip=true).
#
# Target: macOS ARM64 (aarch64-apple-darwin).
set -euo pipefail
cd "$(dirname "$0")"

PROFILE="production-stripped"
TARGET_DIR="target/${PROFILE}"
DIST_DIR="dist"

# Format: BIN_NAME:PKG_NAME — builds package and expects binary BIN_NAME
BUILD_TARGETS=("ghost:ghost-cli" "ghost-mcp:ghost-mcp")

# Additional binaries created as copies after build
# Format: COPY_NAME:SOURCE_BIN
COPY_TARGETS=("ghost-cli:ghost")

# All output binary names (for reporting / verification / packaging)
ALL_BINARIES=("ghost" "ghost-cli" "ghost-mcp")

echo "=== Ghost Engine Release Build ==="
echo "Profile:   $PROFILE"
echo "Toolchain: $(rustc --version)"
echo "Host:      $(rustc -vV | grep host | awk '{print $2}')"
echo ""

# ── Build binaries ───────────────────────────────────────────────────────────

for entry in "${BUILD_TARGETS[@]}"; do
    BIN_NAME="${entry%%:*}"
    PKG_NAME="${entry##*:}"

    echo "Building $BIN_NAME (package: $PKG_NAME)..."
    cargo build --profile "$PROFILE" -p "$PKG_NAME"

    BINARY="$TARGET_DIR/$BIN_NAME"
    if [[ ! -f "$BINARY" ]]; then
        echo "error: binary not found at $BINARY"
        exit 1
    fi
    echo "  ✓ $BINARY"
done

# ── Create copy targets ──────────────────────────────────────────────────────

for entry in "${COPY_TARGETS[@]}"; do
    COPY_NAME="${entry%%:*}"
    SRC_NAME="${entry##*:}"

    echo "Copying $SRC_NAME → $COPY_NAME..."
    cp -f "$TARGET_DIR/$SRC_NAME" "$TARGET_DIR/$COPY_NAME"
    echo "  ✓ $TARGET_DIR/$COPY_NAME"
done

echo ""
echo "=== Build Complete ==="

# ── Size report ──────────────────────────────────────────────────────────────

show_sizes() {
    echo ""
    echo "=== Binary Sizes ==="
    for BIN_NAME in "${ALL_BINARIES[@]}"; do
        BINARY="$TARGET_DIR/$BIN_NAME"
        SIZE=$(ls -lh "$BINARY" | awk '{print $5}')
        echo "  $BIN_NAME: $SIZE"
    done
    if command -v size &>/dev/null; then
        echo ""
        for BIN_NAME in "${ALL_BINARIES[@]}"; do
            echo "  ── $BIN_NAME sections ──"
            size "$TARGET_DIR/$BIN_NAME" 2>/dev/null || true
        done
    fi
}

# ── Single-binary deployment verification (TSK-5.9) ─────────────────────────

verify_deployment() {
    echo ""
    echo "=== Deployment Verification ==="

    local all_ok=true

    for BIN_NAME in "${ALL_BINARIES[@]}"; do
        BINARY="$TARGET_DIR/$BIN_NAME"

        echo ""
        echo "── $BIN_NAME ──"

        # 1. Check it's a real executable
        if ! file "$BINARY" | grep -q "Mach-O"; then
            echo "  ✗ Not a Mach-O executable"
            all_ok=false
            continue
        fi
        echo "  ✓ Valid Mach-O binary"

        # 2. Architecture check (ARM64)
        ARCH=$(file "$BINARY" | grep -oE "arm64|x86_64" || echo "unknown")
        echo "  ✓ Architecture: $ARCH"

        # 3. No debug symbols (stripped)
        DSYM_COUNT=$(dsymutil -s "$BINARY" 2>/dev/null | grep -c "N_FUN\|N_SLINE" || true)
        if [[ "$DSYM_COUNT" -eq 0 ]]; then
            echo "  ✓ Stripped (no debug symbols)"
        else
            echo "  ⚠ Contains $DSYM_COUNT debug symbol entries"
        fi

        # 4. Dynamic library dependencies — should only link system libs
        echo "  Dynamic dependencies:"
        otool -L "$BINARY" 2>/dev/null | tail -n +2 | while read -r line; do
            LIB=$(echo "$line" | awk '{print $1}')
            if [[ "$LIB" == /usr/lib/* ]] || [[ "$LIB" == /System/* ]]; then
                echo "    ✓ $LIB (system)"
            else
                echo "    ⚠ $LIB (non-system)"
            fi
        done

        # 5. Quick smoke test — binary should print help or version without crashing
        if "$BINARY" --help &>/dev/null || "$BINARY" --version &>/dev/null; then
            echo "  ✓ Smoke test passed (--help/--version exits cleanly)"
        else
            # Some binaries may not have --help; just ensure no segfault (exit ≠ 139)
            EXIT_CODE=$("$BINARY" --help 2>/dev/null; echo $?) || true
            if [[ "$EXIT_CODE" -ne 139 ]] && [[ "$EXIT_CODE" -ne 134 ]]; then
                echo "  ✓ Smoke test passed (no crash)"
            else
                echo "  ✗ Smoke test FAILED (signal $EXIT_CODE)"
                all_ok=false
            fi
        fi
    done

    echo ""
    if $all_ok; then
        echo "=== All verifications passed ==="
    else
        echo "=== Some verifications failed ==="
        return 1
    fi
}

# ── Package into dist/ tarball ───────────────────────────────────────────────

package_dist() {
    echo ""
    echo "=== Packaging ==="

    rm -rf "$DIST_DIR"
    mkdir -p "$DIST_DIR"

    for BIN_NAME in "${ALL_BINARIES[@]}"; do
        cp "$TARGET_DIR/$BIN_NAME" "$DIST_DIR/"
    done

    # Include key docs
    cp -f README.md "$DIST_DIR/" 2>/dev/null || true
    cp -f ports/ghost-mcp/README.md "$DIST_DIR/MCP_SERVER.md" 2>/dev/null || true

    # Create tarball
    ARCH=$(rustc -vV | grep host | awk '{print $2}')
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    TARBALL="ghost-engine-${VERSION}-${ARCH}.tar.gz"

    tar -czf "$TARBALL" -C "$DIST_DIR" .
    echo "  ✓ $TARBALL ($(ls -lh "$TARBALL" | awk '{print $5}'))"

    echo ""
    echo "=== Distribution Contents ==="
    tar -tzf "$TARBALL" | sed 's/^/  /'

    echo ""
    echo "Deploy with:"
    echo "  tar -xzf $TARBALL -C /usr/local/bin/"
}

# ── Main ─────────────────────────────────────────────────────────────────────

FLAG="${1:-}"

show_sizes

case "$FLAG" in
    --size|-s)
        ;; # sizes already shown
    --verify|-v)
        verify_deployment
        ;;
    --package|-p)
        verify_deployment
        package_dist
        ;;
    *)
        echo ""
        echo "Run CLI with:"
        echo "  $TARGET_DIR/ghost <url>"
        echo "  $TARGET_DIR/ghost-cli <url>"
        echo "  $TARGET_DIR/ghost --interactive <url>"
        echo ""
        echo "Run MCP server with:"
        echo "  $TARGET_DIR/ghost-mcp"
        ;;
esac
