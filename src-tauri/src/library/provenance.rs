//! Field provenance, promoted vs user (TSD §4.2): a field the user confirmed from Spotify data
//! (`promoted_fields`) is `promoted`; anything else gets the create default, and any edit is `user`.
use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use super::Prov;
use crate::error::Result;

/// The rule: listed in `promoted_fields` → `promoted`, otherwise `default`.
pub fn source_for(field: &str, promoted_fields: &[String], default: Prov) -> Prov {
    if promoted_fields.iter().any(|f| f == field) {
        Prov::Promoted
    } else {
        default
    }
}

/// Write provenance for `fields` (create: default = `NewItem.provenance`; update: default = `user`).
pub(crate) fn record(
    conn: &Connection,
    id: i64,
    fields: &[&str],
    default: Prov,
    promoted_fields: &[String],
) -> Result<()> {
    let mut st = conn.prepare(
        "INSERT INTO field_provenance(item_id, field, source) VALUES (?1, ?2, ?3)
         ON CONFLICT(item_id, field) DO UPDATE SET source = excluded.source",
    )?;
    for f in fields {
        st.execute(params![id, f, source_for(f, promoted_fields, default)])?;
    }
    Ok(())
}

/// `ItemDetail.provenance`.
pub fn map(conn: &Connection, id: i64) -> Result<BTreeMap<String, Prov>> {
    let mut st = conn.prepare("SELECT field, source FROM field_provenance WHERE item_id = ?1")?;
    let rows = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::ItemType;
    use crate::library::items::{ItemPatch, create, get, update};
    use crate::library::testutil::new;

    #[test]
    fn confirmed_prefill_is_promoted_and_user_edit_is_user() {
        let mut c = open_in_memory().unwrap();
        // Quick add: link pasted, oEmbed title prefilled and saved as is; notes typed by the user.
        let mut n = new(ItemType::Album, "Con Todo El Mundo");
        n.spotify_uri = Some("spotify:album:BBBBBBBBBBBBBBBBBBBBBB".into());
        n.notes = Some("from the radio".into());
        n.promoted_fields = vec!["display_name".into(), "spotify_uri".into()];
        let d = create(&mut c, n).unwrap();
        assert_eq!(d.provenance["display_name"], Prov::Promoted);
        assert_eq!(d.provenance["spotify_uri"], Prov::Promoted);
        assert_eq!(d.provenance["notes"], Prov::User);
        assert!(
            !d.provenance.contains_key("recommended_by"),
            "unset fields have no provenance"
        );

        // The user renames it → user.
        let p = ItemPatch {
            display_name: Some(Some("Con Todo".into())),
            ..Default::default()
        };
        let d = update(&mut c, d.summary.id, p).unwrap();
        assert_eq!(d.provenance["display_name"], Prov::User);
        assert_eq!(d.provenance["spotify_uri"], Prov::Promoted);

        // Naming a name-needed item from a confirmed prefill → promoted again.
        let p = ItemPatch {
            display_name: Some(Some("Con Todo El Mundo".into())),
            promoted_fields: vec!["display_name".into()],
            ..Default::default()
        };
        let d = update(&mut c, d.summary.id, p).unwrap();
        assert_eq!(
            get(&c, d.summary.id).unwrap().provenance["display_name"],
            Prov::Promoted
        );

        // A pasted link's URI is promoted even when the client didn't list it (TSD §4.2).
        let mut n = new(ItemType::Track, "Typed");
        n.spotify_uri = Some("spotify:track:CCCCCCCCCCCCCCCCCCCCCC".into());
        let d = create(&mut c, n).unwrap();
        assert_eq!(d.provenance["spotify_uri"], Prov::Promoted);
        assert_eq!(d.provenance["display_name"], Prov::User);
    }

    #[test]
    fn provenance_promoted_default_applies_to_unlisted_fields() {
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Track, "T");
        n.provenance = Prov::Promoted;
        n.artist_name = Some("A".into());
        let d = create(&mut c, n).unwrap();
        assert_eq!(d.provenance["display_name"], Prov::Promoted);
        assert_eq!(d.provenance["artist"], Prov::Promoted);
        let artist = get(&c, d.links[0].item.id).unwrap();
        assert_eq!(artist.provenance["display_name"], Prov::Promoted);
    }
}
