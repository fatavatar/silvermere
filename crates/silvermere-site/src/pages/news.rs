//! All the news, newest first, with a filter by kind.

use super::{all_news, news_item};
use crate::Site;
use crate::html::shell;

pub fn render(site: &Site) -> String {
    let news = all_news(site);
    let mut body = String::new();
    if news.is_empty() {
        body.push_str("<p class=\"note\">Nothing has been written in the chronicle yet.</p>\n");
    } else {
        body.push_str(
            r#"<div class="filter" role="group" aria-label="Show">
<button type="button" data-filter="all" aria-pressed="true">Everything</button>
<button type="button" data-filter="new" aria-pressed="false">Rumours</button>
<button type="button" data-filter="change" aria-pressed="false">Revisions</button>
<button type="button" data-filter="fix" aria-pressed="false">Errata</button>
</div>
<div class="news-list">
"#,
        );
        for (overlay, n) in news {
            body.push_str(&news_item(overlay, n, "h2"));
        }
        body.push_str("</div>\n");
    }
    shell(
        site,
        "news.html",
        "The chronicle",
        "Changes to the world, and rumours of what is new.",
        &[],
        &body,
    )
}
