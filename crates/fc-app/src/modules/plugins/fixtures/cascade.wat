;; Adversarial fixture (security re-review, finding 1): whatever it is called for (an
;; `event:deck_deleted` hook), it creates decks PA and PB, then fails. Its writes are rolled
;; back; neither they nor the rollback may feed hooks again (no runaway cascade).
(module
  (import "fc" "host_call" (func $host_call (param i32 i32) (result i32)))
  (import "fc" "host_reply" (func $host_reply (param i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  (func (export "fc_abi_version") (result i32) (i32.const 1))
  (func $alloc (export "fc_alloc") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $p))
  ;; Sends the request at $ptr/$len and fetches the reply; returns it packed (ptr<<32|len).
  (func $call (param $ptr i32) (param $len i32) (result i64)
    (local $rlen i32)
    (local $buf i32)
    (local.set $rlen (call $host_call (local.get $ptr) (local.get $len)))
    (local.set $buf (call $alloc (local.get $rlen)))
    (call $host_reply (local.get $buf))
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $buf)) (i64.const 32))
      (i64.extend_i32_u (local.get $rlen))))
  (data (i32.const 0) "{\"fn\":\"invoke\",\"args\":{\"method\":\"v1.decks.create\",\"params\":{\"path\":\"PA\"}}}")
  (data (i32.const 128) "{\"fn\":\"invoke\",\"args\":{\"method\":\"v1.decks.create\",\"params\":{\"path\":\"PB\"}}}")
  (data (i32.const 256) "{\"err\":{\"code\":\"BOOM\",\"message\":\"boom\"}}")
  (func (export "fc_handle") (param $ptr i32) (param $len i32) (result i64)
    (drop (call $call (i32.const 0) (i32.const 74)))
    (drop (call $call (i32.const 128) (i32.const 74)))
    (i64.const 1099511627816))
)
