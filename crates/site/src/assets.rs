use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Clone)]
pub struct Assets {
    root: PathBuf,
    dev_server: Option<String>,
    chunks: HashMap<String, ViteChunk>,
}

#[derive(Clone, Default)]
pub struct AssetTags {
    pub css: Vec<String>,
    pub scripts: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct ViteChunk {
    file: String,
    #[serde(default)]
    css: Vec<String>,
}

impl Assets {
    pub fn load(root: PathBuf) -> Self {
        let chunks = fs::read_to_string(root.join("manifest.json"))
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();

        Self {
            root,
            dev_server: std::env::var("HEYTEA_VITE_DEV_SERVER").ok(),
            chunks,
        }
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn tags(&self, entry: &str, include_script: bool) -> AssetTags {
        if let Some(dev_server) = &self.dev_server {
            let mut tags = AssetTags {
                css: Vec::new(),
                scripts: vec![format!("{}/@vite/client", dev_server.trim_end_matches('/'))],
            };
            if include_script || entry.ends_with("status.ts") {
                tags.scripts
                    .push(format!("{}/{}", dev_server.trim_end_matches('/'), entry));
            }
            return tags;
        }

        let Some(chunk) = self.chunks.get(entry) else {
            return AssetTags::default();
        };

        AssetTags {
            css: chunk.css.iter().map(|path| format!("/{path}")).collect(),
            scripts: if include_script {
                vec![format!("/{}", chunk.file)]
            } else {
                Vec::new()
            },
        }
    }
}
