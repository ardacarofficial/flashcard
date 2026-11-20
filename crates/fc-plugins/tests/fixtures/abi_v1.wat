;; Frozen ABI v1 contract fixture. DO NOT EDIT: later app versions must keep running this
;; module unchanged (docs/04 v0.7, "v1 eklentisi sonraki çekirdek sürümünde çalışmaya devam
;; eder"). It exercises every v1 export and import:
;;   fc_abi_version -> 1
;;   fc_alloc       -> bump allocator
;;   fc_handle      -> calls host `invoke` (v1.decks.list), fetches the reply with
;;                     `host_reply` and returns it unchanged as its own reply (HostReply and
;;                     OpReply share the {"ok"} / {"err"} shape).
(module
  (import "fc" "host_call" (func $host_call (param i32 i32) (result i32)))
  (import "fc" "host_reply" (func $host_reply (param i32)))
  (memory (export "memory") 2)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "{\"fn\":\"invoke\",\"args\":{\"method\":\"v1.decks.list\",\"params\":{}}}")
  (func (export "fc_abi_version") (result i32) (i32.const 1))
  (func $alloc (export "fc_alloc") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $p))
  (func (export "fc_handle") (param $ptr i32) (param $len i32) (result i64)
    (local $rlen i32)
    (local $buf i32)
    (local.set $rlen (call $host_call (i32.const 0) (i32.const 61)))
    (local.set $buf (call $alloc (local.get $rlen)))
    (call $host_reply (local.get $buf))
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $buf)) (i64.const 32))
      (i64.extend_i32_u (local.get $rlen))))
)
