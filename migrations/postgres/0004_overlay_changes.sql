-- ============================================================================
-- Content overlays: the changes they made
-- ============================================================================

-- What a content overlay last set on a record it changed but did not create
-- (`[[kind_change]]`, docs/overlays.md). A change checks that the record holds the values
-- its `was` expects; once applied, the record holds the overlay's own values instead, so a
-- revised change is let through when the record still holds exactly what this overlay last
-- set, and refused when anyone else has changed it since.
CREATE TABLE overlay_changes (
    kind       TEXT NOT NULL, -- room, monster, item, spell, shop or message
    rec_key    TEXT NOT NULL, -- the record's key: "map/room" for a room, else the number
    overlay    TEXT NOT NULL, -- the name in the overlay's overlay.toml
    fields     TEXT NOT NULL, -- JSON object: each field the change set, as written
    applied_at TEXT NOT NULL, -- RFC 3339 UTC, when the overlay last wrote the change
    PRIMARY KEY (kind, rec_key, overlay)
);
