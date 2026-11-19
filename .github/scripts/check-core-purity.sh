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
deny='^(rusqlite|sqlx|diesel|sea-orm|rand|rand_core|getrandom|fastrand|uuid|fs-err|fs_extra|tokio|async-std|smol|reqwest|hyper|ureq|surf|isahc|attohttpc|tauri|tauri-.*|walkdir|tempfile|notify|keyring|dirs|directories|home|mio|socket2|libsqlite3-sys|fc-storage|fc-app|fc-api|fc-providers|fc-cli)$'
deps=$(awk '
  /^\[/ {
    sec = 0
    if ($0 ~ /^\[(target\.[^]]*\.)?(build-)?dependencies\]/) { sec = 1 }
    else if (match($0, /^\[(target\.[^]]*\.)?(build-)?dependencies\.[A-Za-z0-9_-]+\]/)) {
      n = $0; sub(/^\[(target\.[^]]*\.)?(build-)?dependencies\./, "", n); sub(/\].*/, "", n); print n
    }
    next
  }
  sec && /^[A-Za-z0-9_-]+[ \t]*(=|\.)/ { n=$1; sub(/[ \t=.].*/, "", n); print n }
' "$CORE/Cargo.toml" | sort -u)
for d in $deps; do
  if [[ "$d" =~ $deny ]]; then
    echo "::error file=$CORE/Cargo.toml::fc-core yasaklı bağımlılık içeriyor: $d"
    fail=1
  fi
done

# 2) Yasaklı kullanımlar (src/ altında; #[cfg(test)] sonrası, yorum ve string içerikleri hariç)
patterns=(
  'std::fs' 'std::net' 'std::process' 'std::env::' 'std::io::stdin' 'std::io::stdout' 'std::io::stderr'
  'SystemTime::now' 'Instant::now' 'Utc::now' 'Local::now'
  'thread_rng' 'rand::random' 'rand::rng' 'OsRng' 'from_entropy' 'getrandom'
  'Uuid::new_v[0-9]+' 'Uuid::now_v[0-9]+'
  'File::open' 'File::create' 'TcpStream' 'UdpSocket'
  'static mut' 'lazy_static' 'thread_local!'
  'static[^;=]*(Mutex|RwLock|Atomic[A-Za-z0-9]*)'
)
strip() {
  awk '
    /#\[cfg\(test\)\]/ { exit }
    {
      line = $0
      out = ""
      while (length(line) > 0) {
        if (inblock) {
          i = index(line, "*/")
          if (i == 0) { line = ""; break }
          line = substr(line, i + 2); inblock = 0; continue
        }
        c2 = substr(line, 1, 2)
        if (c2 == "//") break
        if (c2 == "/*") { inblock = 1; line = substr(line, 3); continue }
        ch = substr(line, 1, 1)
        if (ch == "\"") {
          line = substr(line, 2)
          while (length(line) > 0) {
            ch = substr(line, 1, 1)
            if (ch == "\\") { line = substr(line, 3); continue }
            line = substr(line, 2)
            if (ch == "\"") break
          }
          out = out "\"\""
          continue
        }
        out = out ch; line = substr(line, 2)
      }
      print FILENAME ":" FNR ": " out
    }
  ' "$1"
}
if [ -d "$CORE/src" ]; then
  stripped=$(find "$CORE/src" -name '*.rs' -print0 | while IFS= read -r -d '' f; do strip "$f"; done)
  for p in "${patterns[@]}"; do
    if hits=$(printf '%s\n' "$stripped" | grep -E -- "$p"); then
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
