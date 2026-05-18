#!/bin/bash
# Test runner for demystify-lua with LuaJIT
# Requires: LuaJIT, conjure (or Docker/Podman)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LUA_MODULE_DIR="$PROJECT_ROOT/demystify-lua"
TEST_DIR="$SCRIPT_DIR"
TST_DIR="$PROJECT_ROOT/demystify/tst"

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

# Define test puzzles: name:eprime:param
declare -a PUZZLES=(
    "binairo:binairo.eprime:binairo-1.param"
    "little_sudoku:little-sudoku.eprime:little-sudoku.param"
    "minesweeper:minesweeper.eprime:minesweeperPrinted.param"
    "little1:little1.eprime:little1.param"
)

# Generate test puzzle JSONs
echo "Generating test puzzle JSONs..."
for puzzle_entry in "${PUZZLES[@]}"; do
    IFS=':' read -r puzzle_name eprime_file param_file <<< "$puzzle_entry"
    PUZZLE_JSON="$TEST_DIR/test_${puzzle_name}.json"
    EPRIME_PATH="$TST_DIR/$eprime_file"
    PARAM_PATH="$TST_DIR/$param_file"

    if [[ ! -f "$PUZZLE_JSON" ]]; then
        if [[ -f "$EPRIME_PATH" ]] && [[ -f "$PARAM_PATH" ]]; then
            echo "  Creating $puzzle_name puzzle JSON..."
            cargo run --release --bin demystify -- \
                --model "$EPRIME_PATH" \
                --param "$PARAM_PATH" \
                --save-parsed "$PUZZLE_JSON" 2>/dev/null || {
                echo -e "${YELLOW}  Warning: Failed to generate $puzzle_name${NC}"
                continue
            }
            echo -e "  ${GREEN}Created: test_${puzzle_name}.json${NC}"
        else
            echo -e "${YELLOW}  Skipping $puzzle_name: files not found${NC}"
        fi
    else
        echo "  Exists: test_${puzzle_name}.json"
    fi
done

# Also create backward-compatible test_puzzle.json symlink
if [[ ! -f "$TEST_DIR/test_puzzle.json" ]]; then
    # Find first available puzzle JSON
    for puzzle_entry in "${PUZZLES[@]}"; do
        IFS=':' read -r puzzle_name _ _ <<< "$puzzle_entry"
        if [[ -f "$TEST_DIR/test_${puzzle_name}.json" ]]; then
            ln -sf "test_${puzzle_name}.json" "$TEST_DIR/test_puzzle.json"
            echo "  Symlinked test_puzzle.json -> test_${puzzle_name}.json"
            break
        fi
    done
fi

echo

# Set up Lua path to find the module
# LuaJIT loads .so files directly, so we create a symlink named demystify.so
TEMP_DIR=$(mktemp -d)

# Cleanup function to remove temp dir and generated JSON files
cleanup() {
    rm -rf "$TEMP_DIR"
    # Clean up generated test puzzle JSON files
    rm -f "$TEST_DIR"/test_*.json
}
trap cleanup EXIT

ln -sf "$LIB_PATH" "$TEMP_DIR/demystify.so"

# Run pure Lua helper tests first (no native library needed)
echo "Running helper tests (pure Lua)..."
echo
cd "$TEST_DIR"
LUA_PATH="$TEST_DIR/?.lua;;" luajit test_helpers.lua

echo

# Run builder tests (need the native library, but no Conjure / puzzle JSON).
echo "Running builder tests..."
echo
if LUA_CPATH="$TEMP_DIR/?.so;;" LUA_PATH="$TEST_DIR/?.lua;;" luajit test_builder.lua; then
    echo -e "${GREEN}PASS: builder${NC}"
else
    echo -e "${RED}FAIL: builder${NC}"
    exit 1
fi

echo
echo "Running builder sudoku-4x4 test..."
echo
if LUA_CPATH="$TEMP_DIR/?.so;;" LUA_PATH="$TEST_DIR/?.lua;;" luajit test_builder_sudoku.lua; then
    echo -e "${GREEN}PASS: builder sudoku-4x4${NC}"
else
    echo -e "${RED}FAIL: builder sudoku-4x4${NC}"
    exit 1
fi

echo
echo "Running builder minesweeper-via-REVEAL test..."
echo
if LUA_CPATH="$TEMP_DIR/?.so;;" LUA_PATH="$TEST_DIR/?.lua;;" luajit test_builder_minesweeper.lua; then
    echo -e "${GREEN}PASS: builder minesweeper${NC}"
else
    echo -e "${RED}FAIL: builder minesweeper${NC}"
    exit 1
fi

echo

# Run the demystify library tests with each puzzle type
echo "Running demystify library tests..."
echo

TESTS_PASSED=0
TESTS_FAILED=0

for puzzle_entry in "${PUZZLES[@]}"; do
    IFS=':' read -r puzzle_name _ _ <<< "$puzzle_entry"
    PUZZLE_JSON="$TEST_DIR/test_${puzzle_name}.json"

    if [[ -f "$PUZZLE_JSON" ]]; then
        echo "--- Testing with $puzzle_name ---"
        if LUA_CPATH="$TEMP_DIR/?.so;;" LUA_PATH="$TEST_DIR/?.lua;;" luajit test_demystify.lua "$PUZZLE_JSON"; then
            echo -e "${GREEN}PASS: $puzzle_name${NC}"
            TESTS_PASSED=$((TESTS_PASSED + 1))
        else
            echo -e "${RED}FAIL: $puzzle_name${NC}"
            TESTS_FAILED=$((TESTS_FAILED + 1))
        fi
        echo
    fi
done

# Summary
echo "=== Test Summary ==="
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
if [[ $TESTS_FAILED -gt 0 ]]; then
    echo -e "Failed: ${RED}$TESTS_FAILED${NC}"
    exit 1
else
    echo -e "Failed: $TESTS_FAILED"
fi
echo
echo -e "${GREEN}=== All LuaJIT tests completed successfully ===${NC}"
