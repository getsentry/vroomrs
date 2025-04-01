#!/usr/bin/env bash
set -euxo pipefail

# Store the target version in an environment variable for Perl to access safely
export BUMP_VERSION="$2"

echo "Bumping version: ${BUMP_VERSION}"

CARGO_TOML_FILE="$(pwd)/cargo.toml"

# Use Perl for in-place editing, replacing only the first occurrence
perl -i -pe 's/^version =.*/version = "$ENV{BUMP_VERSION}"/ && ($found=1) unless $found' $CARGO_TOML_FILE

# Unset the temporary environment variable
unset BUMP_VERSION