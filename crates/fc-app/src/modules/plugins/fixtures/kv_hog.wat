;; Adversarial fixture (security re-review, finding 3): writes two 600 KiB values to its
;; private storage (keys "a" then "b") and returns the host's reply to the second write. The
;; per-plugin storage quota and the `storage:kv` permission must stop it.
(module
  (import "fc" "host_call" (func $host_call (param i32 i32) (result i32)))
  (import "fc" "host_reply" (func $host_reply (param i32)))
  (memory (export "memory") 40)
  (global $heap (mut i32) (i32.const 1048576))
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
  (data (i32.const 0) "{\"fn\":\"kv.set\",\"args\":{\"key\":\"a\",\"value\":\"")
  (func (export "fc_handle") (param $ptr i32) (param $len i32) (result i64)
    ;; value: 614400 × 'x', then the closing "}}
    (memory.fill (i32.const 42) (i32.const 120) (i32.const 614400))
    (i32.store8 (i32.const 614442) (i32.const 34))
    (i32.store8 (i32.const 614443) (i32.const 125))
    (i32.store8 (i32.const 614444) (i32.const 125))
    (drop (call $call (i32.const 0) (i32.const 614445)))
    ;; key "a" -> "b"
    (i32.store8 (i32.const 30) (i32.const 98))
    (call $call (i32.const 0) (i32.const 614445)))
)
