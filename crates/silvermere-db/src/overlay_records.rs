//! `overlay_records`: which content overlay created each record it added. An overlay may
//! rewrite a record it created itself on a later apply, and never one the realm's own files or
//! another overlay made; [`owner`] is how it tells.
//!
//! `overlay_changes`: what an overlay last set on a record it changed. A revised change is let
//! through while the record still holds those values ([`last_change`]).

use crate::Result;
use crate::pool::Db;

/// Who made the record at `kind`/`key`, as far as overlays know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    /// No overlay created it: the realm's own record, or none at all.
    Realm,
    /// The overlay with this name created it.
    Overlay(String),
}

/// The owner of the record `kind`/`key`.
pub async fn owner(db: &Db, kind: &str, key: &str) -> Result<Owner> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT overlay FROM overlay_records WHERE kind = $1 AND rec_key = $2")
            .bind(kind)
            .bind(key)
            .fetch_optional(&db.pool())
            .await?;
    Ok(row.map_or(Owner::Realm, |(name,)| Owner::Overlay(name)))
}

/// Records that `overlay` wrote the record `kind`/`key` it created.
pub async fn record(db: &Db, kind: &str, key: &str, overlay: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO overlay_records (kind, rec_key, overlay, applied_at) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (kind, rec_key) DO UPDATE SET overlay = excluded.overlay, \
         applied_at = excluded.applied_at",
    )
    .bind(kind)
    .bind(key)
    .bind(overlay)
    .bind(jiff::Timestamp::now().to_string())
    .execute(&db.pool())
    .await?;
    Ok(())
}

/// Every `(kind, key)` the overlay `name` created, in key order.
pub async fn created_by(db: &Db, name: &str) -> Result<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT kind, rec_key FROM overlay_records WHERE overlay = $1 ORDER BY kind, rec_key",
    )
    .bind(name)
    .fetch_all(&db.pool())
    .await?)
}

/// The fields (a JSON object) the overlay `name` last set on `kind`/`key`, if it changed it.
pub async fn last_change(db: &Db, kind: &str, key: &str, name: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT fields FROM overlay_changes WHERE kind = $1 AND rec_key = $2 AND overlay = $3",
    )
    .bind(kind)
    .bind(key)
    .bind(name)
    .fetch_optional(&db.pool())
    .await?;
    Ok(row.map(|(f,)| f))
}

/// Records the fields (a JSON object) the overlay `name` set on `kind`/`key`.
pub async fn record_change(db: &Db, kind: &str, key: &str, name: &str, fields: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO overlay_changes (kind, rec_key, overlay, fields, applied_at) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (kind, rec_key, overlay) DO UPDATE SET fields = excluded.fields, \
         applied_at = excluded.applied_at",
    )
    .bind(kind)
    .bind(key)
    .bind(name)
    .bind(fields)
    .bind(jiff::Timestamp::now().to_string())
    .execute(&db.pool())
    .await?;
    Ok(())
}

/// Whether the overlay `name` has written anything here: a record it created or a change.
pub async fn applied(db: &Db, name: &str) -> Result<bool> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM overlay_records WHERE overlay = $1 \
         UNION ALL SELECT 1 FROM overlay_changes WHERE overlay = $1 LIMIT 1",
    )
    .bind(name)
    .fetch_optional(&db.pool())
    .await?;
    Ok(row.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn owner_follows_the_last_record() {
        let t = crate::TestDb::from_env().await.expect("test database");
        let db = t.db();
        assert_eq!(owner(db, "room", "19/1").await.unwrap(), Owner::Realm);
        record(db, "room", "19/1", "ashfall").await.unwrap();
        record(db, "monster", "2100", "ashfall").await.unwrap();
        assert_eq!(
            owner(db, "room", "19/1").await.unwrap(),
            Owner::Overlay("ashfall".into())
        );
        // the same key under another kind is another record
        assert_eq!(owner(db, "monster", "19/1").await.unwrap(), Owner::Realm);
        record(db, "room", "19/1", "ashfall").await.unwrap();
        assert_eq!(
            created_by(db, "ashfall").await.unwrap(),
            vec![
                ("monster".to_string(), "2100".to_string()),
                ("room".to_string(), "19/1".to_string())
            ]
        );
        t.close().await;
    }

    #[tokio::test]
    async fn the_last_change_is_kept_per_overlay() {
        let t = crate::TestDb::from_env().await.expect("test database");
        let db = t.db();
        assert_eq!(
            last_change(db, "monster", "520", "ashfall").await.unwrap(),
            None
        );
        record_change(db, "monster", "520", "ashfall", r#"{"maxhp":2400}"#)
            .await
            .unwrap();
        record_change(db, "monster", "520", "ashfall", r#"{"maxhp":2300}"#)
            .await
            .unwrap();
        assert_eq!(
            last_change(db, "monster", "520", "ashfall").await.unwrap(),
            Some(r#"{"maxhp":2300}"#.to_string())
        );
        assert_eq!(
            last_change(db, "monster", "520", "other").await.unwrap(),
            None
        );
        t.close().await;
    }
}
