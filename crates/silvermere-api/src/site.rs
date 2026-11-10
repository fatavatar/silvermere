//! The realm's public web site: the files under one directory, served as they are.
//!
//! This is not the admin API: it has no tokens and no routes of its own, it listens on its own
//! address (`site.bind`), and it serves only what `silvermere site build` (or the server's
//! start) wrote. Nothing outside the directory can be reached: a path with `..` or any other
//! component that is not a plain name is not found.

use std::future::Future;
use std::path::{Component, Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The router serving the files under `root`.
pub fn site_router(root: PathBuf) -> Router {
    Router::new()
        .fallback(file)
        .with_state(std::sync::Arc::new(root))
}

/// Serves the site under `root` on `listener` until `shutdown` completes.
pub async fn serve_site(
    listener: tokio::net::TcpListener,
    root: PathBuf,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, site_router(root))
        .with_graceful_shutdown(shutdown)
        .await
}

async fn file(State(root): State<std::sync::Arc<PathBuf>>, req: Request) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return (StatusCode::METHOD_NOT_ALLOWED, "only GET").into_response();
    }
    let Some(path) = resolve(&root, req.uri().path()) else {
        return not_found(&root).await;
    };
    match read(path.clone()).await {
        Ok(bytes) => {
            let mut resp = if req.method() == Method::HEAD {
                let mut r = Response::new(Body::empty());
                r.headers_mut()
                    .insert(header::CONTENT_LENGTH, HeaderValue::from(bytes.len()));
                r
            } else {
                Response::new(Body::from(bytes))
            };
            let headers = resp.headers_mut();
            let kind = content_type(&path);
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
            headers.insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            // pages change when the world does; let browsers check
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            if kind == MDB {
                headers.insert(
                    header::CONTENT_DISPOSITION,
                    HeaderValue::from_static("attachment"),
                );
            }
            resp
        }
        Err(_) => not_found(&root).await,
    }
}

/// A file's bytes, read off the async threads.
async fn read(path: PathBuf) -> std::io::Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .unwrap_or_else(|e| Err(std::io::Error::other(e)))
}

/// The file a URL path names under `root`: `/` and directories are their `index.html`.
fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path)?;
    let mut path = root.to_path_buf();
    for component in Path::new(decoded.trim_start_matches('/')).components() {
        match component {
            Component::Normal(name) => path.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if decoded.ends_with('/') || path == root || path.is_dir() {
        path.push("index.html");
    }
    Some(path)
}

/// `%XX` escapes decoded; `None` for a malformed escape or one that is not UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let s = String::from_utf8(out).ok()?;
    // a decoded NUL or backslash is never part of a file name we serve
    (!s.contains('\0') && !s.contains('\\')).then_some(s)
}

const MDB: &str = "application/x-msaccess";

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        Some("mdb") => MDB,
        _ => "application/octet-stream",
    }
}

async fn not_found(root: &Path) -> Response {
    let body = read(root.join("404.html"))
        .await
        .unwrap_or_else(|_| b"Not found".to_vec());
    let mut resp = (StatusCode::NOT_FOUND, body).into_response();
    let kind = if root.join("404.html").exists() {
        "text/html; charset=utf-8"
    } else {
        "text/plain; charset=utf-8"
    };
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_under_the_root() {
        let root = Path::new("/srv/site");
        assert_eq!(
            resolve(root, "/"),
            Some(PathBuf::from("/srv/site/index.html"))
        );
        assert_eq!(
            resolve(root, "/play.html"),
            Some(PathBuf::from("/srv/site/play.html"))
        );
        assert_eq!(
            resolve(root, "/assets/site.css"),
            Some(PathBuf::from("/srv/site/assets/site.css"))
        );
        assert_eq!(resolve(root, "/../etc/passwd"), None);
        assert_eq!(resolve(root, "/assets/%2e%2e/%2e%2e/etc/passwd"), None);
        assert_eq!(resolve(root, "/a%5cb"), None);
        assert_eq!(resolve(root, "/bad%zz"), None);
    }

    #[test]
    fn types_by_extension() {
        assert_eq!(
            content_type(Path::new("a.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(content_type(Path::new("realm.mdb")), MDB);
        assert_eq!(content_type(Path::new("x")), "application/octet-stream");
    }
}
