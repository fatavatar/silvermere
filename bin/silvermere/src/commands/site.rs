//! `silvermere site build`: the realm's web site, written to `<data_dir>/site` (docs/site.md).

use std::process::ExitCode;

use anyhow::Context;
use clap::Subcommand;
use silvermere_db::Db;

use crate::config::Config;

/// `silvermere site`.
#[derive(Debug, Subcommand)]
pub enum SiteCmd {
    /// Write the web site (and the MajorMUD Explorer database) from the world as it is now.
    /// Safe while the server runs; the server builds it too each time it starts.
    Build,
}

pub fn run(cfg: &Config, cmd: &SiteCmd) -> anyhow::Result<ExitCode> {
    match cmd {
        SiteCmd::Build => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async {
                let db = Db::connect_and_migrate(&cfg.database.url)
                    .await
                    .with_context(|| format!("opening {}", cfg.database_shown()))?;
                let result = crate::site::build(cfg, &db).await;
                db.close().await;
                let report = result?;
                for w in &report.warnings {
                    eprintln!("site build: {w}");
                }
                if let Some((file, bytes)) = &report.mme {
                    println!("MajorMUD Explorer database: {file} ({bytes} bytes)");
                }
                println!("wrote {} files to {}", report.files, report.dir.display());
                Ok(ExitCode::SUCCESS)
            })
        }
    }
}
