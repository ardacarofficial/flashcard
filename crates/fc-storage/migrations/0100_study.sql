-- v0.2 study core. The tables (`cards`, `review_log`, `sessions`) exist since 0001; the
-- session engine and card history add these lookups.

-- Reviews of a session (end-of-session numbers).
CREATE INDEX review_log_session ON review_log(session_id) WHERE session_id IS NOT NULL;

-- The open session (`study.active`: resume after a restart or crash).
CREATE INDEX sessions_open ON sessions(started_at) WHERE ended_at IS NULL;
