//! `overlay`: content overlays (docs/overlays.md). `check` plans an overlay against the
//! database and writes nothing, so it is safe while the server runs; `apply` writes it and
//! takes the single-writer guard, since a running server would not see the new records and its
//! saves could overwrite the changed ones.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context as _;
use clap::{Args, Subcommand};
use silvermere_db::Db;
use silvermere_import::overlay;

use crate::config::Config;

/// `overlay` subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum OverlayCmd {
    /// Build and check overlays against the database without writing anything.
    Check(OverlayArgs),
    /// Write overlays into the database (stop the server first).
    Apply(OverlayArgs),
    /// Record that the world already holds an overlay (a realm imported from the data files the
    /// overlay was exported from), writing no content (stop the server first).
    Adopt(AdoptArgs),
    /// Print a stored record as an overlay entry names its fields, to copy from.
    Show(ShowArgs),
    /// Write what this database holds beyond a base world as an overlay.
    Export(ExportArgs),
}

/// `overlay export` arguments.
#[derive(Debug, Clone, Args)]
pub struct ExportArgs {
    /// The base world's database URL: the world the overlay is applied to (often a stock
    /// import).
    #[arg(long, value_name = "URL")]
    pub base: String,
    /// The overlay's name, for its overlay.toml.
    #[arg(long)]
    pub name: String,
    /// The directory to write it into (created; files there of the same names are replaced).
    #[arg(long, value_name = "DIR")]
    pub out: PathBuf,
    /// Database URL (sqlite:// or postgres://); overrides `database.url`.
    #[arg(long)]
    pub database_url: Option<String>,
}

/// `overlay adopt` arguments.
#[derive(Debug, Clone, Args)]
pub struct AdoptArgs {
    /// The overlay's directory.
    pub dir: PathBuf,
    /// Check only: say what would be adopted, and write nothing.
    #[arg(long)]
    pub check: bool,
    /// Database URL (sqlite:// or postgres://); overrides `database.url`.
    #[arg(long)]
    pub database_url: Option<String>,
}

/// `overlay show` arguments.
#[derive(Debug, Clone, Args)]
pub struct ShowArgs {
    /// room, monster, item, spell, shop, message, text, class, race, action, help, desc_file
    /// or game_text.
    pub kind: String,
    /// The key: `map/room` for a room, else the number.
    pub key: String,
    /// Database URL (sqlite:// or postgres://); overrides `database.url`.
    #[arg(long)]
    pub database_url: Option<String>,
}

impl OverlayCmd {
    /// The database URL given on the command line, if any.
    pub fn database_url(&self) -> Option<&str> {
        match self {
            Self::Check(a) | Self::Apply(a) => a.database_url.as_deref(),
            Self::Show(a) => a.database_url.as_deref(),
            Self::Adopt(a) => a.database_url.as_deref(),
            Self::Export(a) => a.database_url.as_deref(),
        }
    }
}

/// `overlay check` / `overlay apply` flags.
#[derive(Debug, Clone, Args)]
pub struct OverlayArgs {
    /// Overlay directories (each with an overlay.toml), applied in load order; none given: the
    /// ones in `content.overlays`.
    #[arg(value_name = "DIR")]
    pub dirs: Vec<PathBuf>,
    /// Database URL (sqlite:// or postgres://); overrides `database.url`.
    #[arg(long)]
    pub database_url: Option<String>,
}

/// The overlays to work on: those given, else those configured.
fn dirs_of(cfg: &Config, a: &OverlayArgs) -> anyhow::Result<Vec<PathBuf>> {
    let dirs = if a.dirs.is_empty() {
        cfg.content.overlays.clone()
    } else {
        a.dirs.clone()
    };
    if dirs.is_empty() {
        anyhow::bail!("no overlays: give their directories, or list them in content.overlays");
    }
    Ok(dirs)
}

pub fn run(cfg: &Config, cmd: &OverlayCmd) -> anyhow::Result<ExitCode> {
    match cmd {
        OverlayCmd::Check(a) => apply_all(cfg, &dirs_of(cfg, a)?, false),
        OverlayCmd::Apply(a) => {
            let dirs = dirs_of(cfg, a)?;
            super::with_guard(cfg, "overlay apply", || apply_all(cfg, &dirs, true))
        }
        OverlayCmd::Adopt(a) if a.check => adopt(cfg, a),
        OverlayCmd::Adopt(a) => super::with_guard(cfg, "overlay adopt", || adopt(cfg, a)),
        OverlayCmd::Show(a) => show(cfg, a),
        OverlayCmd::Export(a) => export(cfg, a),
    }
}

/// Writes the difference between this database and the base as an overlay.
fn export(cfg: &Config, a: &ExportArgs) -> anyhow::Result<ExitCode> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let db = Db::connect_and_migrate(&cfg.database.url)
            .await
            .with_context(|| format!("opening {}", cfg.database_shown()))?;
        let base = Db::connect_and_migrate(&a.base)
            .await
            .with_context(|| format!("opening {}", silvermere_db::redact_url(&a.base)))?;
        let result = overlay::export(&db, &base, &a.name, &a.out).await;
        db.close().await;
        base.close().await;
        match result {
            Ok(report) => {
                print!("{report}");
                println!("wrote {}", a.out.display());
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("overlay export: {e}");
                Ok(ExitCode::from(1))
            }
        }
    })
}

/// Prints the record as TOML in the overlay's field names.
fn show(cfg: &Config, a: &ShowArgs) -> anyhow::Result<ExitCode> {
    let kind = overlay::Kind::from_name(&a.kind).with_context(|| {
        format!(
            "{}: not a kind ({})",
            a.kind,
            overlay::Kind::ALL.map(|k| k.name()).join(", ")
        )
    })?;
    let key = overlay::Key::parse(kind, &a.key)
        .with_context(|| format!("{}: not a {kind} key", a.key))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let db = Db::connect_and_migrate(&cfg.database.url)
            .await
            .with_context(|| format!("opening {}", cfg.database_shown()))?;
        let found = overlay::stored_record(&db, kind, key.clone()).await;
        db.close().await;
        match found? {
            Some(v) => {
                print!("{}", toml::to_string(&v).context("as TOML")?);
                Ok(ExitCode::SUCCESS)
            }
            None => {
                eprintln!("no {kind} {key}");
                Ok(ExitCode::from(1))
            }
        }
    })
}

/// Plans (and with `write`, writes) the overlays in `dirs` in load order and prints what each
/// did. The first overlay with a problem stops the rest; what earlier ones wrote stays.
/// `overlay adopt`.
fn adopt(cfg: &Config, a: &AdoptArgs) -> anyhow::Result<ExitCode> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let db = Db::connect_and_migrate(&cfg.database.url)
            .await
            .with_context(|| format!("opening {}", cfg.database_shown()))?;
        let result = overlay::adopt_dir(&db, &a.dir, !a.check).await;
        db.close().await;
        match result {
            Ok(r) => {
                let verb = if a.check { "would adopt" } else { "adopted" };
                println!(
                    "overlay [{}]: {verb}: {} records it created, {} it changed ({} changed since by \
                     other overlays, {} taken over by them and left theirs); no content written",
                    r.overlay, r.owned, r.changed, r.changed_since, r.taken_over
                );
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("overlay adopt: {e}");
                Ok(ExitCode::from(1))
            }
        }
    })
}

pub fn apply_all(cfg: &Config, dirs: &[PathBuf], write: bool) -> anyhow::Result<ExitCode> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let db = Db::connect_and_migrate(&cfg.database.url)
            .await
            .with_context(|| format!("opening {}", cfg.database_shown()))?;
        let result = overlay::apply_dirs(&db, dirs, write).await;
        db.close().await;
        match result {
            Ok(plans) => {
                for plan in plans {
                    print!("{plan}");
                    if !write {
                        println!("overlay [{}]: checked, nothing written", plan.overlay);
                    }
                }
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("overlay: {e}");
                Ok(ExitCode::from(1))
            }
        }
    })
}
