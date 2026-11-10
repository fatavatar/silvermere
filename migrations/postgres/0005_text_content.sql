-- ============================================================================
-- Content: help topics, description files and game texts
-- ============================================================================

-- 0001's help_topics was never filled; it is made again with the topic's message name and
-- its place in the list, so an overlay can change one topic or add new ones.
DROP TABLE help_topics;

-- Help topics (WCCMMHLP.MSG): the overview, then each topic in the order HELP tries them.
CREATE TABLE help_topics (
    topic    TEXT PRIMARY KEY, -- the message name: HL1B0 the overview, HL1Tn a stock topic
    position BIGINT NOT NULL,  -- the order HELP tries the topics; the highest is the end marker
    words    TEXT NOT NULL,    -- what a player's topic is matched against; empty for HL1B0
    text     TEXT NOT NULL     -- the help text, as the .MSG file holds it (CP437 as Unicode)
);

-- The text files `FILE DESCRIPTION` rooms and items name (gang houses' WCCnnnnn.HSE, the
-- emblem, banner, statue and stone files).
CREATE TABLE desc_files (
    name TEXT PRIMARY KEY, -- the file name, upper case (WCC85015.HSE)
    text TEXT NOT NULL     -- the file's text (CP437 as Unicode, line ends as stored)
);

-- The game's texts that the realm's message files set (WCCMMUD.MSG and WCCTEXT.MSG blocks:
-- USRLOGIN, USRFOLED, ...).
CREATE TABLE game_texts (
    msg_key TEXT PRIMARY KEY, -- the message name (USRLOGIN)
    text    TEXT NOT NULL     -- the text, as the .MSG file holds it (CP437 as Unicode)
);
