;; Test fixture: a hook that always traps (a fault). The victim in the fault attribution test.
(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  (func (export "fc_abi_version") (result i32) (i32.const 1))
  (func (export "fc_alloc") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $p))
  (func (export "fc_handle") (param $ptr i32) (param $len i32) (result i64)
    unreachable)
)
