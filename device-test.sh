#!/usr/bin/env bash
set -e
cargo build -p mixctl-beacn-test
exec cargo run -p mixctl-beacn-test $*
