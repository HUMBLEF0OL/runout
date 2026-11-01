//! User library logic (TSD §3, §4.2). Functions take `&Connection` / `&mut Connection`; commands.rs wraps them.
//! Serde structs mirror API.md §2 field-for-field (snake_case, no `rename_all` on structs).
use serde::{Deserialize, Serialize};

pub mod backlog;
pub mod dashboard;
pub mod filters;
pub mod items;
pub mod links;
pub mod provenance;
pub mod rediscover;
pub mod search;
pub mod status;
pub mod tags;

/// SQL expression for "now" in the stored ISO-8601 UTC format (matches the column defaults).
pub(crate) const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ','now')";

/// A text enum stored as TEXT: serde snake_case, `as_str`/`parse`, rusqlite To/FromSql.
macro_rules! sql_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($var),+ }
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$var => $s),+ }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some(Self::$var),)+ _ => None }
            }
        }
        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(self.as_str().into())
            }
        }
        impl rusqlite::types::FromSql for $name {
            fn column_result(v: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                let s = v.as_str()?;
                Self::parse(s).ok_or_else(|| {
                    rusqlite::types::FromSqlError::Other(format!("bad {}: {s}", stringify!($name)).into())
                })
            }
        }
    };
}

sql_enum!(ItemType { Album = "album", Track = "track", Artist = "artist" });
sql_enum!(Status {
    Planned = "planned",
    InProgress = "in_progress",
    Explored = "explored",
    NotForMe = "not_for_me",
    Abandoned = "abandoned",
});
sql_enum!(Priority { High = "high", Medium = "medium", Low = "low" });
sql_enum!(LinkKind { ByArtist = "by_artist", OnAlbum = "on_album", VersionOf = "version_of" });
sql_enum!(
    /// Field provenance (TSD §4.2).
    Prov { User = "user", Promoted = "promoted" }
);

/// API.md §1 `Page<T>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Page<T> {
    pub rows: Vec<T>,
    pub total: i64,
    pub offset: i64,
}

#[cfg(test)]
pub(crate) mod testutil {
    use rusqlite::Connection;

    use super::items::{self, NewItem};
    use super::{ItemType, Prov};

    pub fn new(kind: ItemType, name: &str) -> NewItem {
        NewItem {
            r#type: kind,
            display_name: Some(name.into()),
            spotify_uri: None,
            status: None,
            priority: None,
            in_backlog: None,
            tags: vec![],
            recommended_by: None,
            provenance: Prov::User,
            promoted_fields: vec![],
            capture_ms: None,
            artist_name: None,
            notes: None,
            album_id: None,
        }
    }

    pub fn add(c: &mut Connection, kind: ItemType, name: &str) -> i64 {
        items::create(c, new(kind, name)).unwrap().summary.id
    }
}
