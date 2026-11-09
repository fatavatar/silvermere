-- ============================================================================
-- Content overlays
-- ============================================================================

-- The records a content overlay created (`silvermere overlay apply`, docs/overlays.md). An
-- overlay may later rewrite a record it created itself, but never one the realm's own files or
-- another overlay made, so the owner of every created key is kept here. A change to an existing
-- record is not listed: it carries its own check of the values it expects to replace.
CREATE TABLE overlay_records (
    kind       TEXT NOT NULL, -- room, monster, item, spell, shop, message or text
    rec_key    TEXT NOT NULL, -- the record's key: "map/room" for a room, else the number
    overlay    TEXT NOT NULL, -- the name in the overlay's overlay.toml
    applied_at TEXT NOT NULL, -- RFC 3339 UTC, when the overlay last wrote the record
    PRIMARY KEY (kind, rec_key)
);
