#!/usr/bin/env bash
# fc-core saf kalmalı: I/O crate'i, dosya/ağ/saat/rastgelelik yok (CLAUDE.md kural 2).
set -euo pipefail
cd "$(dirname "$0")/../.."

CORE=crates/fc-core
if [ ! -d "$CORE" ]; then
  echo "::notice::$CORE yok (henüz gelmedi), atlanıyor."
  exit 0
fi

fail=0

# 1) Yasaklı bağımlılıklar ([dependencies] ve [build-dependencies]; dev-dependencies serbest)
deny='^(rusqlite|sqlx|diesel|sea-orm|tokio|async-std|smol|reqwest|hyper|ureq|surf|isahc|attohttpc|tauri|tauri-.*|walkdir|tempfile|notify|keyring|dirs|directories|home|mio|socket2|libsqlite3-sys|fc-storage|fc-app|fc-api|fc-providers|fc-cli)$'
deps=$(awk '
  /^\[/ { sec = ($0 ~ /^\[(dependencies|build-dependencies|target\..*\.dependencies)\]/) ; next }
  sec && /^[A-Za-z0-9_-]+[ \t]*(=|\.)/ { n=$1; sub(/[ \t=.].*/, "", n); print n }
' "$CORE/Cargo.toml" | sort -u)
for d in $deps; do
  if [[ "$d" =~ $deny ]]; then
    echo "::error file=$CORE/Cargo.toml::fc-core yasaklı bağımlılık içeriyor: $d"
    fail=1
  fi
done

# 2) Yasaklı kullanımlar (src/ altında)
patterns=(
  'std::fs' 'std::net' 'std::process' 'std::env::'
  'SystemTime::now' 'Instant::now' 'Utc::now' 'Local::now'
  'thread_rng' 'rand::random' 'OsRng' 'from_entropy' 'getrandom'
  'File::open' 'File::create' 'TcpStream' 'UdpSocket'
  'static mut' 'lazy_static' 'OnceLock' 'OnceCell' 'thread_local!'
)
if [ -d "$CORE/src" ]; then
  for p in "${patterns[@]}"; do
    if hits=$(grep -rnF --include='*.rs' -- "$p" "$CORE/src" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'); then
      while IFS= read -r line; do
        echo "::error::fc-core saf değil ('$p'): $line"
      done <<< "$hits"
      fail=1
    fi
  done
fi

if [ "$fail" -ne 0 ]; then
  echo "fc-core saflık kontrolü BAŞARISIZ (Clock/Rng dışarıdan verilmeli)."
  exit 1
fi
echo "fc-core saflık kontrolü geçti."
