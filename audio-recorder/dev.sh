#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/crates/app"
exec cargo tauri dev "$@"
