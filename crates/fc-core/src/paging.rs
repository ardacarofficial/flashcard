//! Cursor-based pagination primitives shared by storage and API.

use serde::{Deserialize, Serialize};

/// Default page size when the caller does not specify one.
pub const DEFAULT_PAGE_SIZE: u32 = 100;
/// Hard upper bound on page size.
pub const MAX_PAGE_SIZE: u32 = 1000;

/// Requests one page. `cursor` is opaque (produced by the previous page).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PageRequest {
    /// Opaque cursor from a previous page; `None` = first page.
    pub cursor: Option<String>,
    /// Page size; `None` = [`DEFAULT_PAGE_SIZE`]. Clamped to [`MAX_PAGE_SIZE`].
    pub limit: Option<u32>,
}

impl PageRequest {
    /// Effective limit: default applied, clamped to `1..=MAX_PAGE_SIZE`.
    pub fn effective_limit(&self) -> u32 {
        self.limit
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE)
    }
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page<T> {
    /// Items in this page.
    pub items: Vec<T>,
    /// Cursor for the next page; `None` = last page.
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_clamped() {
        assert_eq!(PageRequest::default().effective_limit(), DEFAULT_PAGE_SIZE);
        let big = PageRequest {
            cursor: None,
            limit: Some(10_000),
        };
        assert_eq!(big.effective_limit(), MAX_PAGE_SIZE);
        let zero = PageRequest {
            cursor: None,
            limit: Some(0),
        };
        assert_eq!(zero.effective_limit(), 1);
    }
}
