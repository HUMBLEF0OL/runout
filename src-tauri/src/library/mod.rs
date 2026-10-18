//! User library logic (TSD §3, §4.2). Functions take `&Connection` / `&mut Connection`; commands.rs wraps them.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Album,
    Track,
    Artist,
}

impl ItemType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Track => "track",
            Self::Artist => "artist",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "album" => Some(Self::Album),
            "track" => Some(Self::Track),
            "artist" => Some(Self::Artist),
            _ => None,
        }
    }
}
