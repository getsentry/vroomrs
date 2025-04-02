#!/usr/bin/env bash
set -euxo pipefail

# Store the target version in an environment variable for Perl to access safely
export BUMP_VERSION="$2"

echo "Bumping version: ${BUMP_VERSION}"

# Use Perl for in-place editing, replacing only the first occurrence
perl -i -pe 's/^version =.*/version = "$ENV{BUMP_VERSION}"/ && ($found=1) unless $found' Cargo.toml

# Unset the temporary environment variable
unset BUMP_VERSION