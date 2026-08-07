use crate::model::{Group, Server};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// What lives on disk. Hand-editable on purpose.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    #[serde(default)]
    pub servers: Vec<Server>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

pub fn load() -> Document {
    let path = paths::servers_file();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Document::default();
    };

    if let Some(doc) = parse(&text) {
        return doc;
    }

    // Never silently discard a file we couldn't parse — move it aside so the
    // user can recover whatever was in there.
    eprintln!("cucina: {} is not valid JSON", path.display());
    let _ = std::fs::rename(&path, path.with_extension("json.broken"));
    Document::default()
}

/// The current shape, or the one 0.1 wrote. `None` means neither, which the
/// caller treats as a file worth preserving rather than overwriting.
fn parse(text: &str) -> Option<Document> {
    if let Ok(doc) = serde_json::from_str::<Document>(text) {
        return Some(doc);
    }
    // 0.1 stored a bare array of servers. Read it rather than lose it; the
    // next save writes the current shape.
    serde_json::from_str::<Vec<Server>>(text)
        .ok()
        .map(|servers| Document {
            servers,
            groups: Vec::new(),
        })
}

/// Drop records for projects nothing belongs to any more.
fn prune_groups(servers: &[Server], groups: &[Group]) -> Vec<Group> {
    groups
        .iter()
        .filter(|g| servers.iter().any(|s| s.group == g.name))
        .cloned()
        .collect()
}

/// Write atomically: a crash mid-save must not leave a truncated file.
pub fn save(servers: &[Server], groups: &[Group]) -> std::io::Result<()> {
    let path = paths::servers_file();
    let tmp = path.with_extension("json.tmp");
    let doc = Document {
        servers: servers.to_vec(),
        groups: prune_groups(servers, groups),
    };
    let text = serde_json::to_string_pretty(&doc)?;
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::{parse, prune_groups};
    use crate::model::{Group, Server};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn server(id: &str, group: &str) -> Server {
        Server {
            id: id.into(),
            name: id.into(),
            dir: PathBuf::from("/tmp"),
            command: "true".into(),
            group: group.into(),
            tile: 0,
            env: BTreeMap::new(),
            auto_restart: false,
            auto_start: false,
            created_at: 0,
        }
    }

    #[test]
    fn reads_the_current_document_shape() {
        let doc = parse(
            r#"{"servers":[{"id":"api","name":"API","dir":"/tmp","command":"npm run dev"}],
                "groups":[{"name":"acme","icon":""}]}"#,
        )
        .expect("should parse");
        assert_eq!(doc.servers.len(), 1);
        assert_eq!(doc.servers[0].id, "api");
        assert_eq!(doc.groups.len(), 1);
    }

    /// 0.1 wrote a bare array. Upgrading must not lose someone's servers.
    #[test]
    fn migrates_the_bare_array_written_by_0_1() {
        let doc = parse(r#"[{"id":"api","name":"API","dir":"/tmp","command":"npm run dev"}]"#)
            .expect("should migrate");
        assert_eq!(doc.servers.len(), 1);
        assert_eq!(doc.servers[0].id, "api");
        assert!(doc.groups.is_empty());
    }

    /// Fields added after a file was written must not make it unreadable.
    #[test]
    fn tolerates_records_missing_newer_fields() {
        let doc = parse(r#"{"servers":[{"id":"a","name":"A","dir":"/tmp","command":"true"}]}"#)
            .expect("should parse");
        let s = &doc.servers[0];
        assert_eq!(s.group, "");
        assert_eq!(s.tile, 0);
        assert!(!s.auto_restart);
        assert!(s.env.is_empty());
    }

    #[test]
    fn refuses_anything_it_cannot_understand() {
        // The caller moves the file aside rather than overwriting it.
        assert!(parse("not json at all").is_none());
        assert!(parse("").is_none());
        assert!(parse(r#"{"servers":"nope"}"#).is_none());
    }

    #[test]
    fn forgets_groups_that_have_no_members_left() {
        let servers = vec![server("api", "acme"), server("docs", "")];
        let groups = vec![
            Group {
                name: "acme".into(),
                icon: String::new(),
            },
            Group {
                name: "gone".into(),
                icon: String::new(),
            },
        ];
        let kept = prune_groups(&servers, &groups);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].name, "acme");
    }
}
