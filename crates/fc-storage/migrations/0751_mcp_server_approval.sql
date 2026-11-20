-- v0.8: a registration only runs once the user approved it. `approved` holds the approved
-- launch summary (JSON: transport + command + args, or transport + url); a row whose current
-- summary differs (edited, restored from a backup, written by anything but the commands) is
-- not approved and the client refuses to connect it. NULL = never approved.
ALTER TABLE mcp_servers ADD COLUMN approved TEXT;
