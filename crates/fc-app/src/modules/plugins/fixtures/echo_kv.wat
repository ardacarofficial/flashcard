;; Test fixture: stores what the host hands it (the whole `{op, input}` envelope) under key
;; "in" of its own storage, then replies `null`. Shows exactly what a plugin gets to see.
(module
  (import "fc" "host_call" (func $host_call (param i32 i32) (result i32)))
  (import "fc" "host_reply" (func $host_reply (param i32)))
  (memory (export "memory") 4)
  (global $heap (mut i32) (i32.const 1024))
  ;; 42 bytes: the request up to the value.
  (data (i32.const 0) "{\"fn\":\"kv.set\",\"args\":{\"key\":\"in\",\"value\":")
  (data (i32.const 64) "}}")
  ;; 11 bytes: the reply.
  (data (i32.const 128) "{\"ok\":null}")
  (func (export "fc_abi_version") (result i32) (i32.const 1))
  (func $alloc (export "fc_alloc") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $p))
  (func (export "fc_handle") (param $ptr i32) (param $len i32) (result i64)
    (local $req i32)
    (local $n i32)
    (local $rlen i32)
    (local.set $n (i32.add (i32.add (i32.const 42) (local.get $len)) (i32.const 2)))
    (local.set $req (call $alloc (local.get $n)))
    (memory.copy (local.get $req) (i32.const 0) (i32.const 42))
    (memory.copy (i32.add (local.get $req) (i32.const 42)) (local.get $ptr) (local.get $len))
    (memory.copy
      (i32.add (i32.add (local.get $req) (i32.const 42)) (local.get $len))
      (i32.const 64)
      (i32.const 2))
    (local.set $rlen (call $host_call (local.get $req) (local.get $n)))
    (call $host_reply (call $alloc (local.get $rlen)))
    (i64.or (i64.shl (i64.const 128) (i64.const 32)) (i64.const 11)))
)
