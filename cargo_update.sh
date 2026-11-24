#!/bin/bash
# Recursively update all cargo
find . -type d \( -name target -o -name .git \) -prune -o \
  -type f -name Cargo.toml \
  -execdir cargo update --manifest-path Cargo.toml \;
