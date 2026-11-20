//! [`MemTx`] repository traits: first-run and reminder bookkeeping (v1.0).

use super::*;

/// `ux_state`.
#[derive(Debug, Clone, Default)]
pub struct UxState {
    values: BTreeMap<String, Value>,
}

impl UxStateRead for MemTx<'_> {
    fn ux_state(&self, key: &str) -> StorageResult<Option<Value>> {
        Ok(self.s().domains.ux.values.get(key).cloned())
    }
}

impl UxStateWrite for MemTx<'_> {
    fn set_ux_state(&mut self, key: &str, value: &Value, _now: Timestamp) -> StorageResult<()> {
        self.m()
            .domains
            .ux
            .values
            .insert(key.to_owned(), value.clone());
        Ok(())
    }
}
