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

    if let Ok(doc) = serde_json::from_str::<Document>(&text) {
        return doc;
    }
    // 0.1 stored a bare array of servers. Read it rather than lose it; the
    // next save writes the current shape.
    if let Ok(servers) = serde_json::from_str::<Vec<Server>>(&text) {
        return Document { servers, groups: Vec::new() };
    }

    // Never silently discard a file we couldn't parse — move it aside so the
    // user can recover whatever was in there.
    eprintln!("cucina: {} is not valid JSON", path.display());
    let _ = std::fs::rename(&path, path.with_extension("json.broken"));
    Document::default()
}

/// Write atomically: a crash mid-save must not leave a truncated file.
pub fn save(servers: &[Server], groups: &[Group]) -> std::io::Result<()> {
    let path = paths::servers_file();
    let tmp = path.with_extension("json.tmp");
    let doc = Document {
        servers: servers.to_vec(),
        // Drop records for projects nothing belongs to any more.
        groups: groups
            .iter()
            .filter(|g| servers.iter().any(|s| s.group == g.name))
            .cloned()
            .collect(),
    };
    let text = serde_json::to_string_pretty(&doc)?;
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, &path)
}
