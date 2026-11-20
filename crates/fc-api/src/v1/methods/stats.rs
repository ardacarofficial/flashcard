//! Statistics (v0.6).

use crate::v1::stats::*;

methods! {
    // ---- stats ----
    /// Review and card statistics of a scope and date range (cached per study day).
    StatsGetOverview = "stats.overview", Query, "stats", "stats:read", Public, undo: false, StatsRequest => StatsOverview;
    /// Statistics per value of a classification (retention per level, workload per exam…).
    StatsGetBreakdown = "stats.breakdown", Query, "stats", "stats:read", Public, undo: false, StatsBreakdownRequest => StatsBreakdown;
    /// Leeches: cards forgotten at least the threshold number of times.
    StatsLeeches = "stats.leeches", Query, "stats", "stats:read", Public, undo: false, LeechRequest => LeechList;
    /// The report card of every deck.
    StatsDeckReports = "stats.deck_reports", Query, "stats", "stats:read", Public, undo: false, DeckReportsRequest => DeckReports;
    /// Brings the statistics cache up to date (derived data; no undo step).
    StatsRefreshCache = "stats.refresh_cache", Command, "stats", "stats:write", Public, undo: false, RefreshStatsCache => StatsCacheInfo;
}
