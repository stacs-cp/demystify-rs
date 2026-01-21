#!/bin/bash
# Test runner for demystify-lua with LuaJIT
# Requires: LuaJIT, conjure (or Docker/Podman)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LUA_MODULE_DIR="$PROJECT_ROOT/demystify-lua"
TEST_DIR="$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=== demystify-lua LuaJIT test runner ==="
echo

# Check for LuaJIT
if ! command -v luajit &> /dev/null; then
    echo -e "${RED}Error: LuaJIT not found. Please install LuaJIT.${NC}"
    echo "  On Debian/Ubuntu: sudo apt install luajit"
    echo "  On Fedora: sudo dnf install luajit"
    echo "  On Arch: sudo pacman -S luajit"
    exit 1
fi

echo -e "${GREEN}Found LuaJIT:${NC} $(luajit -v 2>&1 | head -n1)"
echo

# Build the library in release mode
echo "Building demystify-lua (release)..."
cd "$PROJECT_ROOT"
cargo build --release -p demystify-lua
echo -e "${GREEN}Build successful${NC}"
echo

# Find the built library
LIB_NAME="libdemystify_lua.so"
if [[ "$OSTYPE" == "darwin"* ]]; then
    LIB_NAME="libdemystify_lua.dylib"
fi

LIB_PATH="$PROJECT_ROOT/target/release/$LIB_NAME"

if [[ ! -f "$LIB_PATH" ]]; then
    echo -e "${RED}Error: Could not find built library at $LIB_PATH${NC}"
    exit 1
fi

echo "Library built at: $LIB_PATH"
echo

# Create test puzzle JSON if it doesn't exist
PUZZLE_JSON="$TEST_DIR/test_puzzle.json"
EPRIME_FILE="$PROJECT_ROOT/demystify/tst/little1.eprime"
PARAM_FILE="$PROJECT_ROOT/demystify/tst/little1.param"

if [[ ! -f "$PUZZLE_JSON" ]]; then
    echo "Creating test puzzle JSON..."

    if [[ ! -f "$EPRIME_FILE" ]] || [[ ! -f "$PARAM_FILE" ]]; then
        echo -e "${YELLOW}Warning: Test eprime/param files not found at expected location${NC}"
        echo "  Expected: $EPRIME_FILE and $PARAM_FILE"
        echo "  Trying binairo test files..."

        EPRIME_FILE="$PROJECT_ROOT/demystify/tst/binairo.eprime"
        PARAM_FILE="$PROJECT_ROOT/demystify/tst/binairo-1.param"

        if [[ ! -f "$EPRIME_FILE" ]] || [[ ! -f "$PARAM_FILE" ]]; then
            echo -e "${RED}Error: No test puzzle files found${NC}"
            exit 1
        fi
    fi

    echo "Using: $EPRIME_FILE and $PARAM_FILE"
    cargo run --release --bin demystify -- --model "$EPRIME_FILE" --param "$PARAM_FILE" --save-parsed "$PUZZLE_JSON"
    echo -e "${GREEN}Puzzle JSON created${NC}"
fi

echo

# Set up Lua path to find the module
# LuaJIT loads .so files directly, so we create a symlink named demystify.so
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

ln -sf "$LIB_PATH" "$TEMP_DIR/demystify.so"

# Run pure Lua helper tests first (no native library needed)
echo "Running helper tests (pure Lua)..."
echo
cd "$TEST_DIR"
LUA_PATH="$TEST_DIR/?.lua;;" luajit test_helpers.lua

echo

# Run the demystify library tests
echo "Running demystify library tests..."
echo
LUA_CPATH="$TEMP_DIR/?.so;;" LUA_PATH="$TEST_DIR/?.lua;;" luajit test_demystify.lua "$PUZZLE_JSON"

echo
echo -e "${GREEN}=== All LuaJIT tests completed successfully ===${NC}"
