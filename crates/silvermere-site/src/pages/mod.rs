//! The pages.

pub mod abilities;
pub mod classes;
pub mod home;
pub mod news;
pub mod play;
pub mod races;
pub mod tools;
pub mod world;

use crate::{NewsKind, NewsView, OverlayView, Site};

/// The latest release of Majordomo, the client the site recommends, with its installers.
pub const MAJORDOMO_RELEASES: &str = "https://github.com/fatavatar/majordomo/releases/latest";

/// Every overlay's news, newest first; on one day, new places before changes and fixes.
pub fn all_news(site: &Site) -> Vec<(&OverlayView, &NewsView)> {
    let mut all: Vec<(&OverlayView, &NewsView)> = site
        .overlays
        .iter()
        .flat_map(|o| o.news.iter().map(move |n| (o, n)))
        .collect();
    let rank = |k: NewsKind| match k {
        NewsKind::New => 0,
        NewsKind::Change => 1,
        NewsKind::Fix => 2,
    };
    all.sort_by(|a, b| {
        b.1.date
            .cmp(&a.1.date)
            .then(rank(a.1.kind).cmp(&rank(b.1.kind)))
    });
    all
}

/// The word a news entry is marked with, and its class.
pub fn kind_label(kind: NewsKind) -> (&'static str, &'static str) {
    match kind {
        NewsKind::New => ("Rumour", "new"),
        NewsKind::Change => ("Revision", "change"),
        NewsKind::Fix => ("Errata", "fix"),
    }
}

/// One news entry as an article.
pub fn news_item(overlay: &OverlayView, n: &NewsView, heading: &str) -> String {
    use crate::html::{esc, paragraphs};
    let (label, class) = kind_label(n.kind);
    format!(
        r#"<article class="news {class}" data-kind="{class}">
<p class="meta"><span class="tag">{label}</span> <time datetime="{date}">{date}</time> · <a href="world.html#{anchor}">{overlay}</a></p>
<{heading}>{title}</{heading}>
{text}
</article>
"#,
        date = esc(&n.date),
        anchor = esc(&crate::html::slug(&overlay.name)),
        overlay = esc(overlay.title()),
        title = esc(&n.title),
        text = paragraphs(&n.text),
    )
}

/// The connect address split for a telnet command: (host, port).
pub fn host_port(connect: &str) -> (String, Option<String>) {
    let bare = connect
        .trim()
        .trim_start_matches("telnet://")
        .trim_end_matches('/');
    match bare.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host.to_owned(), Some(port.to_owned()))
        }
        _ => (bare.to_owned(), None),
    }
}

/// What a player types to connect: `telnet host port`.
pub fn telnet_command(connect: &str) -> String {
    match host_port(connect) {
        (host, Some(port)) if port != "23" => format!("telnet {host} {port}"),
        (host, _) => format!("telnet {host}"),
    }
}

/// A `telnet://` link to the realm.
pub fn telnet_url(connect: &str) -> String {
    match host_port(connect) {
        (host, Some(port)) => format!("telnet://{host}:{port}"),
        (host, None) => format!("telnet://{host}"),
    }
}
