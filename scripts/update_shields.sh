#!/usr/bin/env bash

# Calculate test coverage using tarpaulin
if ! command -v cargo-tarpaulin &> /dev/null
then
    echo "cargo-tarpaulin could not be found. Please install it with 'cargo install cargo-tarpaulin' or skip this hook."
    exit 0
fi

echo "Running tests and calculating coverage..."
COVERAGE=$(DATABASE_URL=postgres://postgres:password@localhost/cdd cargo tarpaulin --engine llvm --timeout 120 --out Lcov 2>&1 | grep -oE "coverage: [0-9.]+" | awk '{print $2}')
if [ -z "$COVERAGE" ]; then
    COVERAGE="0"
fi
COVERAGE_ROUNDED=$(printf "%.0f" "$COVERAGE")

# Determine color for test coverage
if [ "$COVERAGE_ROUNDED" -ge 80 ]; then
    TEST_COLOR="success"
elif [ "$COVERAGE_ROUNDED" -ge 50 ]; then
    TEST_COLOR="yellow"
else
    TEST_COLOR="red"
fi

# Calculate doc coverage using nightly rustdoc
DOC_COVERAGE_RAW=$(cargo +nightly rustdoc --lib -- -Z unstable-options --show-coverage 2>&1 | grep "Total" | awk '{print $6}' | tr -d '%')
if [ -z "$DOC_COVERAGE_RAW" ]; then
    DOC_COVERAGE_RAW="0"
fi
DOC_COVERAGE=$(printf "%.0f" "$DOC_COVERAGE_RAW")

if [ "$DOC_COVERAGE" -ge 80 ]; then
    DOC_COLOR="success"
elif [ "$DOC_COVERAGE" -ge 50 ]; then
    DOC_COLOR="yellow"
else
    DOC_COLOR="red"
fi

# Update README.md
if [ "$COVERAGE_ROUNDED" -gt 0 ]; then
    sed -i -E "s/!\[Test Coverage\]\(https:\/\/img\.shields\.io\/badge\/coverage-[0-9.]+.*?%25-.*?\.svg\)/![Test Coverage](https:\/\/img.shields.io\/badge\/coverage-${COVERAGE_ROUNDED}%25-${TEST_COLOR}.svg)/g" README.md
fi

sed -i -E "s/!\[Doc Coverage\]\(https:\/\/img\.shields\.io\/badge\/docs-[0-9.]+.*?%25-.*?\.svg\)/![Doc Coverage](https:\/\/img.shields.io\/badge\/docs-${DOC_COVERAGE}%25-${DOC_COLOR}.svg)/g" README.md

if [ "$COVERAGE_ROUNDED" -gt 0 ]; then
    echo "Updated shields in README.md (Test: ${COVERAGE_ROUNDED}%, Doc: ${DOC_COVERAGE}%)"
else
    echo "Updated shields in README.md (Test: Skipped due to local db error/0%, Doc: ${DOC_COVERAGE}%)"
fi

if command -v git &> /dev/null; then
    git add README.md
fi
