//! [`MemTx`] repository traits: Subtitle capture diagnostics (v1.4 A5).

use super::*;
use fc_storage::repo::CaptureLogRecord;

/// `subtitle_capture_log`.
#[derive(Debug, Clone, Default)]
pub struct CaptureLogState {
    rows: Vec<CaptureLogRecord>,
    next: i64,
}

impl CaptureLogRead for MemTx<'_> {
    fn recent_capture_log(&self, limit: u32) -> StorageResult<Vec<CaptureLogRecord>> {
        let rows = &self.s().domains.capture_log.rows;
        Ok(rows.iter().rev().take(limit as usize).cloned().collect())
    }
}

impl CaptureLogWrite for MemTx<'_> {
    fn append_capture_log(&mut self, rec: &CaptureLogRecord, keep: u32) -> StorageResult<()> {
        let st = &mut self.m().domains.capture_log;
        st.next += 1;
        st.rows.push(CaptureLogRecord {
            seq: st.next,
            ..rec.clone()
        });
        let excess = st.rows.len().saturating_sub(keep as usize);
        st.rows.drain(..excess);
        Ok(())
    }
}
