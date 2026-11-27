//! The bundled catalog: `assets/models/catalog.json`, written by
//! `tools/models/build.mjs`, with each model's titles, credits and figures.

use crate::text::Lang;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct Titles {
    pub en: String,
    #[serde(rename = "zh-CN")]
    pub zh: String,
}

impl Titles {
    pub fn get(&self, lang: Lang) -> &str {
        match lang {
            Lang::En => &self.en,
            Lang::Zh => &self.zh,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Category {
    pub id: String,
    pub title: Titles,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Stats {
    pub triangles: u32,
    pub vertices: u32,
    pub materials: u32,
    pub textures: u32,
    pub joints: u32,
    pub animations: Vec<String>,
    pub bytes: u64,
    /// Width, height and depth in the model's units, metres by convention.
    pub size: [f32; 3],
}

#[derive(Clone, Debug, Deserialize)]
pub struct Model {
    pub id: String,
    pub title: Titles,
    pub category: String,
    pub collection: String,
    pub authors: Vec<String>,
    pub licenses: Vec<String>,
    pub source: String,
    pub changes: Titles,
    pub stats: Stats,
}

impl Model {
    pub fn license_text(&self) -> String {
        self.licenses
            .iter()
            .map(|l| l.replace("-1.0", " 1.0").replace("-4.0", " 4.0").replace('-', " "))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    pub categories: Vec<Category>,
    pub models: Vec<Model>,
}

impl Catalog {
    pub fn parse(json: &str) -> Result<Catalog, String> {
        let c: Catalog = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if c.schema != 1 {
            return Err(format!("unsupported catalog schema {}", c.schema));
        }
        if let Some(m) = c.models.iter().find(|m| !c.categories.iter().any(|k| k.id == m.category)) {
            return Err(format!("{} names an unknown category {}", m.id, m.category));
        }
        Ok(c)
    }

    pub fn load(dir: &Path) -> Result<Catalog, String> {
        let path = dir.join("models/catalog.json");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Catalog::parse(&text)
    }

    /// The models of a tab: `None` is every model.
    pub fn in_category(&self, category: Option<usize>) -> Vec<usize> {
        let id = category.map(|c| self.categories[c].id.as_str());
        (0..self.models.len()).filter(|&i| id.is_none_or(|id| self.models[i].category == id)).collect()
    }
}

/// Where the app's assets are: beside the binary in a package
/// (`bin/modelsviewer` and `assets/`), else in the source tree.
pub fn assets_dir() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent().and_then(Path::parent) {
            candidates.push(dir.join("assets"));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));
    candidates.push(PathBuf::from("assets"));
    candidates.into_iter().find(|p| p.join("models/catalog.json").is_file()).unwrap_or_else(|| PathBuf::from("assets"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled() -> Catalog {
        Catalog::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")).expect("catalog")
    }

    #[test]
    fn every_model_has_a_file_credits_and_an_open_license() {
        let c = bundled();
        assert!(c.models.len() >= 40);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/models");
        for m in &c.models {
            let file = dir.join(format!("{}.glb", m.id));
            let size = std::fs::metadata(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display())).len();
            assert_eq!(size, m.stats.bytes, "{}", m.id);
            assert!(!m.authors.is_empty() && !m.title.en.is_empty() && !m.title.zh.is_empty(), "{}", m.id);
            assert!(m.source.starts_with("https://"), "{}", m.id);
            assert!(m.licenses.iter().all(|l| l == "CC0-1.0" || l == "CC-BY-4.0"), "{}: {:?}", m.id, m.licenses);
        }
        for k in 0..c.categories.len() {
            assert!(!c.in_category(Some(k)).is_empty(), "{} is empty", c.categories[k].id);
        }
        assert_eq!(c.in_category(None).len(), c.models.len());
    }

    #[test]
    fn license_names_read_well() {
        let mut m = bundled().models[0].clone();
        m.licenses = vec!["CC0-1.0".into(), "CC-BY-4.0".into()];
        assert_eq!(m.license_text(), "CC0 1.0 + CC BY 4.0");
    }
}
