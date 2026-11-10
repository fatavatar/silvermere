# Content overlays

An overlay adds content to a world that is already in the database, and changes content that is
there: new rooms and maps, monsters, items, spells, shops, messages, text blocks, classes, races
and actions, and changes to the realm's own records. It is a directory of TOML files, written in
the record fields of [schema.md](schema.md), and it can be applied again after every re-import.

A whole realm can be an overlay too: `silvermere overlay export` writes what a world holds
beyond the stock one, so the realm is the stock import plus its overlays, with no legacy data
files (see "Exporting a world").

```
silvermere overlay check areas/ashfall      # build and check it; writes nothing
silvermere overlay apply areas/ashfall      # write it (stop the server first)
silvermere import --overlay areas/ashfall   # the stock world, then the overlay
```

The overlays a realm is built with can be listed once, in `content.overlays`
([config.md](config.md#content)): every `silvermere import` (so also the stock world a first
start builds) applies them, and `overlay check`/`apply` use them when given no directories.

`check` reads the database and writes nothing, so it is safe while the server runs. `apply`
takes the single-writer guard: a running server would not see the new records, and its saves
could overwrite the changed ones. Start (or restart) the server afterwards.

## Layout

```
areas/ashfall/
  overlay.toml     name = "ashfall-valley", and optionally title, description, credits,
                   after, overrides, references, and [[news]]
  rooms.toml       any number of content files, read in name order
  monsters.toml
  text.toml
```

`name` is the overlay's identity: every record it creates is recorded as its own (the
`overlay_records` table). Keep it stable; renaming an overlay makes its records look like
someone else's.

### What players see

The realm's web site ([site.md](site.md)) lists the overlays of `content.overlays` and reads
these keys of `overlay.toml`; nothing else does:

| Key | Meaning |
|---|---|
| `title` | the overlay's name as players see it (`"Ashfall Valley"`); `name` when absent |
| `description` | a sentence or two on what it is, for players |
| `credits` | who made it, or the content it comes from |
| `[[news]]` | what it brought, newest first on the site |

Each `[[news]]` entry has a `date` (`YYYY-MM-DD`), a `title`, a `text` (paragraphs separated by
blank lines; text between backticks shows as a command) and a `kind`:

- `"new"`: new content. Write it as a hint that sends players looking, not a map: where the
  rumour starts and what level should go, not what is there.
- `"change"` and `"fix"`: changes to what was there. Write these plainly: what is different now.

```toml
[[news]]
date = "2026-11-09"
kind = "new"
title = "Something stirs beneath the Volcano"
text = """
The miners who work the Volcano's lower tunnels have stopped going past a certain bend.
A character of level 55 or so might go and look."""
```

`overlay export` keeps an `overlay.toml` already in its output directory (when it names the same
overlay), so these survive regenerating an exported world.

## Load order

Overlays are applied one after another, each seeing what the ones before it wrote:

- `after = ["mudrev"]`: this overlay loads after those it names, whatever order they are
  given in. One it names that is not given must have been applied before; two overlays that
  each wait for the other are refused.
- Otherwise they load in the order given (or listed in `content.overlays`).

Two overlays that touch the same record do not silently win in turn:

- A record one overlay created cannot be created by another (see "Owners").
- A change whose `was` no longer matches is refused, so an overlay that changes what another
  already changed stops loudly.
- `overrides = ["mudrev"]` says this overlay builds on the ones it names: it may take over the
  records they created, and a change of its own passes where the record holds what they last
  set (rather than its `was`). It should also be `after` them.

## Entries

Each content file holds entries by kind:

| Entry | Key | Starts from |
|---|---|---|
| `[[room]]` | `map`, `room` | a blank room, or `from = [map, room]` |
| `[[monster]]` | `number` | a blank monster, or `from = <number>` |
| `[[item]]` | `number` | a blank item, or `from` |
| `[[spell]]` | `number` | a blank spell, or `from` |
| `[[shop]]` | `number` | an empty shop, or `from` |
| `[[message]]` | `number` | three empty lines, or `from` |
| `[[class]]`, `[[race]]` | `number` | a blank one, or `from` |
| `[[action]]` | `name` (the verb, any case) | no texts, or `from = "verb"` |
| `[[text]]` | `number` | nothing: `text` (or `parts`) and `link` |
| `[[room_change]]` .. `[[action_change]]` | as above | the record that exists |

A blank record has every field zero or empty, so a new record from scratch must set what it
needs; a template (`from`) is the quicker start for "a variant of that one".

Every other key of an entry is a field of the record, named as in the schema. `why`, `note`
and `source` are for the reader and ignored. A field the record does not have is an error, so
a misspelt field never silently does nothing.

A new record copies everything from its template except the key and what the entry sets. A
copied room keeps only its look and spawns: its exits, floor, placed items, `specmon`,
`cmdtext`, `protmsg`, shop and controlling room start empty. A copied monster starts with
nobody alive; a copied item with no copies in play.

A `[defaults.room]` (or `.monster`, ...) table in a file is merged under every new record of
that kind in that file: a whole area's level range and monster group in one place.

## Values

- **Codes** are written by name, as the schema's enums name them: `item_type = "Weapon"`,
  `alignment = "Evil"`, `kind = "Door"`. Flags are a list of names joined by `|`:
  `flags = "PROTECTED | PATROL"`.
- **Abilities** are `[name, value]` pairs: `abilities = [["Damage", 5], ["TextBlock", 9001]]`.
- **Lists** (abilities, attacks, monster items and spells, shop stock, descriptions) are
  written from the first slot; slots after the last one written are emptied. A slot that is
  a table is merged onto an empty slot, so `attacks = [{ kind = "Normal", min = 10, max = 30,
  pct = 100 }]` needs only what it sets.
- **Descriptions** may be one string: `desc = """..."""` is split into the record's lines.
- **Exits** may be written by direction: `exits = { north = 12, up = { target = 5, kind =
  "MapChange", par1 = 1 } }`. A number is a plain exit to that room on the same map; a table
  is the whole exit. Directions not named stay as they are. A `MapChange` exit leads to the
  map in `par1`, which is how a new map joins the world.
- **Text** may hold colour markup: `{ansi:1;32}` is the escape sequence `ESC[1;32m`, and
  `{{` and `}}` are literal braces. `text` longer than one stored part (1,999 characters) is
  split into parts at line ends; `parts = ["...", "..."]` gives the parts exactly instead.

## Changes

```toml
[[monster_change]]
number = 1206
why = "killing it is the neutral 8th quest's step 48"
set = { deathspell = 2272 }
was = { deathspell = 0 }
```

`set` is merged onto the record; `was` holds the values the overlay expects to replace. When
the record holds something else the whole overlay stops, so an overlay written against one
realm never overwrites a record a later edit or another overlay changed. A change whose `set`
is already in place is reported unchanged, so re-applying is safe. What play changes (a shop's
stock on hand, a room's floor) is left out of both comparisons: a shop that has sold since still
holds the change.

Each applied change is remembered (the `overlay_changes` table): the fields it set and their
values. Once applied, a record holds the overlay's values rather than `was`, so a revised
change (a new `set`, the same `was`) is let through while the record still holds exactly what
this overlay last set, and refused once anyone else has changed those fields.

## Owners

A new record's key must be free, with three exceptions:

- the record was created by this overlay before (an overlay may be edited and applied again);
- the record is a room placeholder (a room named `Buffer Room...`, which a realm's editor
  leaves for later use);
- the entry says `replace = true`, which takes a realm record over on purpose.

A key another overlay created is never taken.

Writing a record keeps its live state: a room's floor and monsters, a monster's counts and kill
time, an item's copies in play. A rewrite that differs from the stored record only in that state
is reported unchanged.

## Checks

Before anything is written, every record is built and every number it points at must exist,
in the world or in the overlay: exits (a `MapChange` exit on the map in `par1`), a room's
`specmon`, `genmonster`, `cmdtext`, shop and placed items; a monster's weapon, items, spells,
attack spells and messages, death and create spells and texts; an item's text, messages and
negated spells; a spell's message; a shop's stock; and the spells, texts, monsters and
messages abilities such as `TextBlock`, `EndCast`, `Summon` and `DescMsg` name. Numbers a
record kept unchanged from its template or from the realm are not checked: they are the
realm's own.

Scripts are read more loosely: topics (`word:N`), `text`, the item, monster, spell, room and
message commands, and the message a failed condition shows. A missing number there is a
warning, as is a text line wider than 79 columns.

Any problem stops the overlay with every problem listed, and nothing is written.

`references = "warn"` in `overlay.toml` makes a number that exists nowhere a warning instead.
It is for a copy of an existing world (what `overlay export` writes), whose dangling numbers
are that world's own; an overlay written by hand should keep the default, `"error"`.

## Exporting a world

```
silvermere overlay export --base sqlite://stock.db --name mudrev --out areas/mudrev
```

writes what this database's content holds beyond the base world's: each record the base
lacks as a new entry, and each record that differs as a change whose `was` is the base's
values. Applied to the base, it gives the same content back: a stock import plus a realm's
exported overlay holds that realm's rooms, monsters, items, spells, shops, messages, texts,
classes, races and actions exactly. What play changes (floors, monsters about, shop stock on
hand, copies in play) is not content and is not exported. Rooms go in one file per map.

An export of a world against the world built from it (stock plus the export) finds nothing: a
quick check that nothing was lost.

## Adopting an overlay

A realm imported from its own data files holds what its exported overlay holds, but nothing
records that: an overlay written against it (`after = ["mudrev"]`) is refused, because `mudrev`
was never applied there, and `mudrev` cannot be applied, because every record it would create
already exists.

```
silvermere overlay adopt --check areas/mudrev   # what would be adopted; writes nothing
silvermere overlay adopt areas/mudrev           # record it (stop the server first)
```

`adopt` builds every record the overlay creates and every change it makes, and compares them
with the world. When the world holds them, it records the overlay as their owner and as having
made the changes, in one transaction, writing no content; from then on the overlay is applied
there, as if `apply` had written it. A record another overlay has changed since (Ashfall Valley
changing a MudRev creature) is excused, and one another overlay has taken over stays that
overlay's. Any other difference is a problem, and nothing is adopted.

Records are read one at a time and only their keys kept, so a whole world's overlay is adopted
in a few hundred megabytes. `apply` and `check` work the same way: the world's records are read
when needed and not kept, and each entry is let go once built, so planning MudRev's 78,809
entries onto a stock world peaks near 350 MB.

## Re-imports

An import upserts the content its files hold and deletes nothing, so the records an overlay
created survive a re-import while the records it changed go back to the import's values. List
the overlays in `content.overlays` (or pass `--overlay DIR`, repeatable) and the import puts
them back in the same run.
