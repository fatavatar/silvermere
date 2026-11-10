# The realm's web site

silvermere writes a small web site for the realm and can serve it:

- **How to play**: a guide to MajorMUD for new players, and the commands to start with.
- **Classes** and **Races**: every class and race the world has, described by the game's own help screens (`HELP NECROLYTES`: its description, magic, combat, weapons, armour and abilities as the realm wrote them), side by side, with an experience calculator for any class and race together. A class or race with no help screen is described from its record.
- **The world**: the overlays the world is built from ([overlays.md](overlays.md)), what each adds and changes, and who made it.
- **News**: each overlay's news, newest first.
- **Tools**: [Majordomo](https://github.com/fatavatar/majordomo), the client the site recommends, and the realm's own MajorMUD Explorer database (which Majordomo reads too), made from the world as the server runs it.

Every page is made from the database and the overlays' manifests, so it follows the world: a
new class or overlay shows up the next time the site is built.

## Turning it on

```toml
[site]
bind = "0.0.0.0:8000"                  # where the site listens; empty: not served
name = "Realm Of Legends"              # the realm's name on the site
tagline = ""                           # a line under the name
connect = "realm.example.org:2323"     # what players type to connect
```

Every key is in [config.md](config.md#site). With `site.bind` set, the server builds the site
each time it starts, into `data/site/`, while it serves the previous build; the new one replaces
it whole when it is ready. To build it by hand (also while the server runs):

```bash
silvermere site build
```

With `site.bind` empty nothing is served and nothing is built at start unless `site.build = true`;
the files in `data/site/` can then be served by any web server (they are static: HTML, CSS, a little JavaScript, and the `.mdb`). silvermere's own
listener serves those files and nothing else, without TLS; put a proxy in front for HTTPS.

The overlays listed are those in `content.overlays`, in load order, with their `title`,
`description`, `credits` and `[[news]]` ([overlays.md](overlays.md#what-players-see)), then any
overlay the database records as applied that `content.overlays` does not list (by name only).

## The MajorMUD Explorer database

[MajorMUD Explorer](https://github.com/syntax53/MMUD-Explorer) (MME), by syntax53, reads a world
from an Access database that Nightmare Redux (NMR), syntax53's exporter, makes from a BBS's data
files. silvermere makes the same database without Windows, Access or Java:

1. `silvermere-mme`'s export, a port of NMR's, turns the world into the rows of each table MME
   reads, with the columns NMR works out: where each item is obtained from, each room's exits
   and lair, what summons each monster, each monster's average damage, and so on.
2. Its Jet 4 writer puts those rows into a copy of MME's own `data-v1.11p.mdb`, replacing every
   row of those tables and rebuilding their indexes. MME's database is downloaded once, from
   MME's repository at a fixed commit, and checked against its sha256 (like the stock world,
   into `data/cache/`). `SILVERMERE_MME_TEMPLATE_URL` names a mirror.

The database also carries a table MME does not read: **Messages**, MegaMUD's message triggers
(its `Messages.md`) made from the realm's own spells, for Majordomo. Each row is a sentence the
game prints and what it means: a lasting spell's start and end lines (`You feel perceptive!` …
`The effects of alertness wear off!`) with the condition its abilities give (blind, confused,
poisoned, held, losing hit points, regenerating), a player's casting lines, and the lines a
monster's spell shows its target, the game's `%s` and `%d` written as `{target}`, `{source}` and
`{dmg}`. Its columns are `Name`, `Message`, `EndMessage`, `Response`, `Source` (`Spell #105`),
`Flags` (MegaMUD's effect bits), `Action` and `Kind` (`effect`, `cast`, `start` or `hit`).

A condition is read from the spell's abilities as the engine applies them: Blind User at any
value, or light too dark to see by, blinds; Paralyze, or Confusion that loses every action,
holds and stops attacks; Hold Person holds; Flee stops attacks; lesser Confusion (one action in
ten or more) confuses; Poison poisons; damage each round loses hit points. Regeneration (hit
points or mana) is set only for a spell that does nothing else, since MegaMUD and Majordomo read
it as "the regeneration spell is up". A line several spells print takes the conditions most of
them give, so a monster's spell that shares a haste line does not mark every haste blind. The
table is added the way Access would add it (a definition, usage maps and a catalog row, with the
catalog's indexes rebuilt); it has no index and no permission rows, which MME and Majordomo do
not need.

`site.mme = false` leaves it out. When it cannot be made (no network for the first download,
say), the build says why and the rest of the site is made without it.

Compared with an NMR export of the same world, every table has the same rows and every column NMR
works out from the data matches. Two kinds of difference remain:

- NMR's average damage comes from a random combat simulation, and ours is a fresh run of the same
  simulation. It differs by about 1% (as two NMR runs do), and so do the figures made from it.
- silvermere does not keep a drop percentage on an empty drop slot, or the high byte of a
  monster's backstab defence, so those cells differ.

## Notes for developers

- `silvermere-site` renders the pages from a `Site` value and reads nothing itself;
  `bin/silvermere/src/site.rs` gathers it.
- `silvermere-mme` holds the export (`nmr`) and the writer (`jet`). The writer supports what MME's
  tables use: number, text and memo columns, and indexes on numbers and on printable ASCII text
  (Access's legacy General sort order). Its round trip over MME's database is
  `tests/jet.rs`, run when `SILVERMERE_MME_TEMPLATE` names a copy of the template.
