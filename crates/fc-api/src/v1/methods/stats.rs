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
    /// Suspends or tags every leech of a scope (nested `bulk.*` call: one undo step).
    StatsApplyToLeeches = "stats.apply_to_leeches", Command, "stats", "cards:write", Public, undo: false, ApplyToLeeches => LeechActionResult;
    /// v1.3: today's answers next to yesterday's, with a pace verdict.
    StatsToday = "stats.today", Query, "stats", "stats:read", Public, undo: false, TodayRequest => TodayStats;
    /// v1.3: cards answered wrong recently, most misses first (with a query to practise them).
    StatsStruggles = "stats.struggles", Query, "stats", "stats:read", Public, undo: false, StrugglesRequest => StruggleList;
    /// The report card of every deck.
    StatsDeckReports = "stats.deck_reports", Query, "stats", "stats:read", Public, undo: false, DeckReportsRequest => DeckReports;
    /// Brings the statistics cache up to date (derived data; no undo step).
    StatsRefreshCache = "stats.refresh_cache", Command, "stats", "stats:write", Public, undo: false, RefreshStatsCache => StatsCacheInfo;
}
