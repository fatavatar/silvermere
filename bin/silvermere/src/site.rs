//! The realm's web site (docs/site.md): gathered from the database and the overlays'
//! manifests, rendered by `silvermere-site`, written to `<data_dir>/site`, and served on
//! `site.bind` by the server.
//!
//! A build writes into a fresh directory beside the site and swaps it in when every file is
//! there, so the server never serves half a site. The MajorMUD Explorer database is the slow
//! part (the template's download the first time, then the export): a build without it still
//! makes every page, and the tools page says the database is not there.

use std::path::{Path, PathBuf};

use anyhow::Context;
use silvermere_db::Db;
use silvermere_import::overlay;
use silvermere_import::stock::{Origin, Pinned};
use silvermere_site::{MmeFile, NewsKind, NewsView, OverlayView, RoomView, Site};
use silvermere_types::ids::RoomKey;

use crate::config::Config;

/// The directory the site is written to.
pub fn site_dir(cfg: &Config) -> PathBuf {
    cfg.data_dir.join("site")
}

/// MME's database, which the realm's is written into.
pub const MME_TEMPLATE: Pinned = Pinned {
    name: "MajorMUD Explorer's data-v1.11p.mdb",
    url: silvermere_mme::template::URL,
    url_env: "SILVERMERE_MME_TEMPLATE_URL",
    file_name: silvermere_mme::template::FILE_NAME,
    sha256: silvermere_mme::template::SHA256,
    size: silvermere_mme::template::SIZE,
};

/// What a build made.
#[derive(Debug)]
pub struct Report {
    pub dir: PathBuf,
    pub files: usize,
    /// The MME database's file name and size.
    pub mme: Option<(String, u64)>,
    /// What was left out, and why.
    pub warnings: Vec<String>,
}

/// Builds the whole site from the world in `db`.
pub async fn build(cfg: &Config, db: &Db) -> anyhow::Result<Report> {
    let mut warnings = Vec::new();
    let classes = db.classes().all().await?;
    let races = db.races().all().await?;
    let start_room = db
        .rooms()
        .get(RoomKey::new(1, cfg.game.start_room))
        .await?
        .map(|r| room_view(&r));
    let overlays = overlays(cfg, db, &mut warnings).await?;
    let help_topics = help_topics(cfg, &mut warnings);

    let dir = site_dir(cfg);
    let staging = cfg.data_dir.join(".site.new");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("clearing {}", staging.display()))?;
    }
    std::fs::create_dir_all(staging.join("assets"))
        .with_context(|| format!("creating {}", staging.display()))?;

    let mme = if cfg.site.mme {
        match build_mme(cfg, db, &staging).await {
            Ok(m) => Some(m),
            Err(e) => {
                warnings.push(format!(
                    "the MajorMUD Explorer database was not built: {e:#}"
                ));
                None
            }
        }
    } else {
        None
    };

    let site = Site {
        realm: cfg.site.name.clone(),
        tagline: cfg.site.tagline.clone(),
        connect: (!cfg.site.connect.trim().is_empty()).then(|| cfg.site.connect.trim().to_owned()),
        open_signup: cfg.game.open_signup,
        death_hp: cfg.game.death_hp,
        top_coin: cfg.game.usr_cur_name.clone(),
        built: jiff::Zoned::now().date().to_string(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        start_room,
        classes,
        races,
        help_topics,
        overlays,
        mme: mme.clone(),
    };
    let pages = silvermere_site::render(&site);
    for page in &pages {
        let path = staging.join(&page.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &page.bytes)
            .with_context(|| format!("writing {}", path.display()))?;
    }

    // swap the new site in
    let old = cfg.data_dir.join(".site.old");
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    if dir.exists() {
        std::fs::rename(&dir, &old).with_context(|| format!("moving {} aside", dir.display()))?;
    }
    std::fs::rename(&staging, &dir)
        .with_context(|| format!("putting the site in {}", dir.display()))?;
    if old.exists() {
        let _ = std::fs::remove_dir_all(&old);
    }

    Ok(Report {
        dir,
        files: pages.len() + usize::from(mme.is_some()),
        mme: mme.map(|m| (m.file, m.bytes)),
        warnings,
    })
}

/// The game's help topics (`WCCMMHLP.MSG` in the assets), for the class and race pages.
fn help_topics(cfg: &Config, warnings: &mut Vec<String>) -> Vec<(String, String)> {
    let assets = cfg.data_dir.join("assets");
    match silvermere_text::package::PackageText::load(&assets) {
        Ok(text) => text
            .help()
            .topics
            .iter()
            .map(|t| (t.words.clone(), t.text.clone()))
            .collect(),
        Err(e) => {
            warnings.push(format!("the game's help text was not read ({e}): classes and races are described from their data alone"));
            Vec::new()
        }
    }
}

/// A room as the game shows it: its name, description and the exits a player can see.
fn room_view(room: &silvermere_types::records::Room) -> RoomView {
    use silvermere_types::enums::ExitType;
    const DIRECTIONS: [&str; 10] = [
        "north",
        "south",
        "east",
        "west",
        "northeast",
        "northwest",
        "southeast",
        "southwest",
        "up",
        "down",
    ];
    let exits = room
        .exits
        .iter()
        .zip(DIRECTIONS)
        .filter(|(e, _)| {
            e.target.0 != 0 && !matches!(e.kind, ExitType::Hidden | ExitType::RemoteAction)
        })
        .map(|(_, d)| d.to_owned())
        .collect();
    RoomView {
        name: room.name.clone(),
        description: room
            .desc
            .iter()
            .map(|l| silvermere_text::strip::strip_ansi_str(l))
            .collect(),
        exits,
    }
}

/// The overlays the site lists: those `content.overlays` names, in load order, with what the
/// database says each holds; then any applied overlay not listed there.
async fn overlays(
    cfg: &Config,
    db: &Db,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<OverlayView>> {
    let mut manifests = Vec::new();
    let mut dirs = std::collections::BTreeMap::new();
    for dir in &cfg.content.overlays {
        match overlay::manifest(dir) {
            Ok(m) => {
                dirs.insert(m.name.clone(), dir.clone());
                manifests.push(m);
            }
            Err(e) => warnings.push(format!("overlay {} not listed: {e}", dir.display())),
        }
    }
    let manifests = load_order(manifests);
    let summary = silvermere_db::overlay_records::summary(db).await?;
    let counts = |name: &str| {
        let mut created = Vec::new();
        let mut changed = Vec::new();
        for (o, kind, c, x) in &summary {
            if o == name {
                if *c > 0 {
                    created.push((kind.clone(), *c));
                }
                if *x > 0 {
                    changed.push((kind.clone(), *x));
                }
            }
        }
        (created, changed)
    };
    let mut out: Vec<OverlayView> = manifests
        .into_iter()
        .map(|m| {
            let (mut created, mut changed) = counts(&m.name);
            // a world built before it was an overlay (MudRev, imported from its data files)
            // has no record of it: count what its files hold instead
            if created.is_empty()
                && changed.is_empty()
                && let Some(dir) = dirs.get(&m.name)
            {
                (created, changed) = count_files(dir);
            }
            OverlayView {
                news: m
                    .news
                    .into_iter()
                    .map(|n| NewsView {
                        date: n.date,
                        title: n.title,
                        kind: match n.kind {
                            overlay::NewsKind::New => NewsKind::New,
                            overlay::NewsKind::Change => NewsKind::Change,
                            overlay::NewsKind::Fix => NewsKind::Fix,
                        },
                        text: n.text,
                    })
                    .collect(),
                name: m.name,
                title: m.title,
                description: m.description,
                credits: m.credits,
                created,
                changed,
            }
        })
        .collect();
    let mut applied: Vec<&str> = summary.iter().map(|(o, ..)| o.as_str()).collect();
    applied.dedup();
    for name in applied {
        if !out.iter().any(|o| o.name == name) {
            let (created, changed) = counts(name);
            out.push(OverlayView {
                name: name.to_owned(),
                created,
                changed,
                ..Default::default()
            });
        }
    }
    Ok(out)
}

/// Records by kind.
type KindCounts = Vec<(String, i64)>;

/// What an overlay's files create and change, by kind: (created, changed).
fn count_files(dir: &Path) -> (KindCounts, KindCounts) {
    let Ok(o) = overlay::load(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut created: std::collections::BTreeMap<&str, i64> = Default::default();
    let mut changed: std::collections::BTreeMap<&str, i64> = Default::default();
    for e in &o.entries {
        let tally = if e.change { &mut changed } else { &mut created };
        *tally.entry(e.kind.name()).or_default() += 1;
    }
    let list = |m: std::collections::BTreeMap<&str, i64>| {
        m.into_iter().map(|(k, n)| (k.to_owned(), n)).collect()
    };
    (list(created), list(changed))
}

/// Manifests in load order: each after those its `after` names (when they are listed),
/// otherwise in the order given.
fn load_order(mut left: Vec<overlay::Manifest>) -> Vec<overlay::Manifest> {
    let mut out: Vec<overlay::Manifest> = Vec::with_capacity(left.len());
    while !left.is_empty() {
        let ready = left.iter().position(|m| {
            m.after
                .iter()
                .all(|a| out.iter().any(|o| &o.name == a) || !left.iter().any(|l| &l.name == a))
        });
        // a cycle (which `overlay check` refuses): keep the rest as given
        let i = ready.unwrap_or(0);
        out.push(left.remove(i));
    }
    out
}

/// Rows per table, by table name.
type RowCounts = Vec<(String, usize)>;

/// Builds the realm's MME database into `dir`.
async fn build_mme(cfg: &Config, db: &Db, dir: &Path) -> anyhow::Result<MmeFile> {
    let cache = crate::stock::cache_dir(&cfg.data_dir);
    let template = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let (path, origin) = MME_TEMPLATE.cached(&cache, false)?;
        if origin != Origin::Cache {
            tracing::info!(path = %path.display(), "downloaded {}", MME_TEMPLATE.name);
        }
        Ok(std::fs::read(&path)?)
    })
    .await??;
    let content = crate::mme::content(db).await?;
    let options = crate::mme::options(cfg);
    let (bytes, tables) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<u8>, RowCounts)> {
            let tables = silvermere_mme::nmr::export(&content, &options);
            // MegaMUD's message triggers, for Majordomo, in a table of their own
            let triggers = silvermere_mme::triggers::message_triggers(&content);
            let template = silvermere_mme::jet::add_table_like(
                &template,
                silvermere_mme::triggers::TABLE,
                silvermere_mme::triggers::LIKE,
                &silvermere_mme::triggers::COLUMNS,
            )?;
            let mut counts: RowCounts = tables
                .by_name()
                .iter()
                .filter(|(name, _)| !matches!(*name, "Info" | "Lairs"))
                .map(|(name, rows)| {
                    let shown = if *name == "TBInfo" {
                        "Text blocks"
                    } else {
                        name
                    };
                    (shown.to_owned(), rows.len())
                })
                .collect();
            counts.push(("Message triggers".to_owned(), triggers.len()));
            let mut named: Vec<(&str, &[silvermere_mme::Row])> = tables
                .by_name()
                .iter()
                .map(|(n, r)| (*n, r.as_slice()))
                .collect();
            named.push((silvermere_mme::triggers::TABLE, triggers.as_slice()));
            Ok((
                silvermere_mme::jet::write_tables(&template, &named)?,
                counts,
            ))
        })
        .await??;
    let file = mme_file_name(&cfg.site.name);
    std::fs::write(dir.join(&file), &bytes)?;
    Ok(MmeFile {
        file,
        bytes: bytes.len() as u64,
        tables,
    })
}

/// The database's file name: the realm's name, lower case, words joined by `-`.
fn mme_file_name(realm: &str) -> String {
    let mut stem = String::new();
    for c in realm.chars() {
        if c.is_ascii_alphanumeric() {
            stem.push(c.to_ascii_lowercase());
        } else if !stem.is_empty() && !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let stem = stem.trim_end_matches('-');
    if stem.is_empty() {
        "realm.mdb".to_owned()
    } else {
        format!("{stem}.mdb")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_from_realm_names() {
        assert_eq!(mme_file_name("Realm Of Legends"), "realm-of-legends.mdb");
        assert_eq!(mme_file_name(""), "realm.mdb");
        assert_eq!(mme_file_name("  ?? "), "realm.mdb");
    }

    #[test]
    fn manifests_load_after_what_they_name() {
        let m = |name: &str, after: &[&str]| overlay::Manifest {
            name: name.into(),
            title: String::new(),
            description: String::new(),
            credits: String::new(),
            after: after.iter().map(|s| (*s).into()).collect(),
            overrides: Vec::new(),
            references_warn: false,
            news: Vec::new(),
        };
        let order: Vec<String> = load_order(vec![
            m("ashfall", &["mudrev"]),
            m("dw", &["mudrev", "gone"]),
            m("mudrev", &[]),
        ])
        .into_iter()
        .map(|m| m.name)
        .collect();
        assert_eq!(order, ["mudrev", "ashfall", "dw"]);
    }
}
