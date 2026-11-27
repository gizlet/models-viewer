#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mode=${1:-}
if [[ $# -gt 0 ]]; then
    shift
fi

case "$mode" in
    run) exec cargo run --locked "$@" ;;
    build) exec cargo build --locked "$@" ;;
    test) exec cargo test --locked "$@" ;;
    package) exec bash scripts/package.sh "$@" ;;
    *)
        printf '%s\n' 'Usage: scripts/dev.sh {run|build|test|package} [arguments]' >&2
        exit 2
        ;;
esac
