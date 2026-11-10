//! HTML helpers: escaping, the page shell, and the little markup news and notes allow.

use std::fmt::Write as _;

use crate::Site;

/// The pages, in navigation order: (file, the chapter name the masthead shows). The home page
/// is the realm's name.
pub const PAGES: [(&str, &str); 6] = [
    ("play.html", "How to play"),
    ("classes.html", "Classes"),
    ("races.html", "Races"),
    ("world.html", "The world"),
    ("news.html", "Chronicle"),
    ("tools.html", "Tools"),
];

/// Text escaped for HTML.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Paragraphs from text whose paragraphs are separated by blank lines. Within a paragraph,
/// text between backticks is a command (`<kbd>`), and line breaks are kept as spaces.
pub fn paragraphs(text: &str) -> String {
    let mut out = String::new();
    for para in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        let joined = para.lines().map(str::trim).collect::<Vec<_>>().join(" ");
        let _ = write!(out, "<p>{}</p>", inline(&joined));
    }
    out
}

/// Inline markup: `command` becomes `<kbd>command</kbd>`.
pub fn inline(text: &str) -> String {
    let mut out = String::new();
    for (i, part) in text.split('`').enumerate() {
        if i % 2 == 1 {
            let _ = write!(out, "<kbd>{}</kbd>", esc(part));
        } else {
            out.push_str(&esc(part));
        }
    }
    out
}

/// An anchor id from a heading.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// A whole page: the masthead (the realm's name and the chapters), the page's title and lede,
/// a contents line for `sections` (anchor, label; empty for none), then `body`.
pub fn shell(
    site: &Site,
    file: &str,
    title: &str,
    lede: &str,
    sections: &[(String, String)],
    body: &str,
) -> String {
    let mut out = String::with_capacity(body.len() + 4096);
    let realm = if site.realm.is_empty() {
        "silvermere"
    } else {
        &site.realm
    };
    let page_title = if file == "index.html" {
        esc(realm)
    } else {
        format!("{} · {}", esc(title), esc(realm))
    };
    let _ = write!(
        out,
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{page_title}</title>
<meta name="description" content="{desc}">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Alegreya:ital,wght@0,400;0,500;0,700;1,400&family=Alegreya+SC:wght@500;700&family=IBM+Plex+Mono:wght@400&display=swap">
<link rel="stylesheet" href="assets/site.css">
</head>
<body>
<a class="skip" href="#main">Skip to the page</a>
<div class="page">
<header class="masthead">
<a class="realm" href="index.html">{realm}</a>
<nav class="chapters" aria-label="Chapters">"##,
        desc = esc(lede),
        realm = esc(realm),
    );
    for (href, word) in PAGES {
        if href == file {
            let _ = write!(out, r#"<a href="{href}" aria-current="page">{word}</a>"#);
        } else {
            let _ = write!(out, r#"<a href="{href}">{word}</a>"#);
        }
    }
    out.push_str("</nav>\n</header>\n<main id=\"main\">\n");
    let _ = writeln!(out, "<h1>{}</h1>", esc(title));
    if !lede.is_empty() {
        let _ = writeln!(out, "<p class=\"lede\">{}</p>", inline(lede));
    }
    if !sections.is_empty() {
        out.push_str("<nav class=\"contents\" aria-label=\"On this page\"><span class=\"label\">Contents</span> ");
        for (i, (anchor, label)) in sections.iter().enumerate() {
            if i > 0 {
                out.push_str(" · ");
            }
            let _ = write!(out, r##"<a href="#{}">{}</a>"##, esc(anchor), esc(label));
        }
        out.push_str("</nav>\n");
    }
    out.push_str(body);
    let _ = write!(
        out,
        r#"
</main>
<footer class="foot">
<p>{realm} runs on <a href="https://github.com/fatavatar/silvermere">silvermere</a> {version}, a server for MajorMUD 1.11p. MajorMUD is by Metropolis, Inc.; the world's sources are credited on the <a href="world.html">world</a> page. Pages made {built}.</p>
</footer>
</div>
<script src="assets/site.js" defer></script>
</body>
</html>
"#,
        realm = esc(realm),
        version = esc(&site.version),
        built = esc(&site.built),
    );
    out
}

/// Markup without its tags and with its entities read, for counting what a line shows.
#[cfg(test)]
pub fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// A number with thousands separators.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A signed percentage as players read it: `+25%`, `-20%`, `±0%`.
pub fn percent(n: i32) -> String {
    match n {
        0 => "±0%".into(),
        n if n > 0 => format!("+{n}%"),
        n => format!("−{}%", -n),
    }
}
