;; Test fixture: every call fails with a plain plugin error (not a fault), so each hook run
;; leaves exactly one log line. Counts hook runs in the hook budget tests.
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
  (data (i32.const 0) "{\"err\":{\"code\":\"BOOM\",\"message\":\"boom\"}}")
  (func (export "fc_handle") (param $ptr i32) (param $len i32) (result i64)
    (i64.const 40))
)
