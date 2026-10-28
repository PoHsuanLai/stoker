//! Where a model's files are in the Hugging Face cache.

use std::path::{Path, PathBuf};

use model_catalog::WeightSource;

/// One path component: the characters a repository or revision name is made of, nothing that
/// could leave the directory it is joined to.
fn component(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_start_matches('.')
        .to_owned()
}

/// A file name with any directory part dropped.
pub(crate) fn file_name(name: &str) -> String {
    component(name.rsplit(['/', '\\']).next().unwrap_or_default())
}

/// `<hf_cache>/models--<org>--<name>`: the cache's directory for one repository.
pub fn repo_dir(hf_cache: &Path, source: &WeightSource) -> PathBuf {
    // A hosted entry has no weights and no engine, so no unit is ever built for it; a directory
    // that cannot exist keeps the function total.
    let name = match source {
        WeightSource::HuggingFace { repo, .. } => repo.0.as_str(),
        WeightSource::Hosted => "hosted/none",
    };
    let parts: Vec<String> = name.split('/').map(component).collect();
    hf_cache.join(format!("models--{}", parts.join("--")))
}

/// `<repo dir>/snapshots/<revision>`: one exact set of files.
pub fn snapshot_dir(repo_dir: &Path, source: &WeightSource) -> PathBuf {
    let revision = match source {
        WeightSource::HuggingFace { revision, .. } => revision.0.as_str(),
        WeightSource::Hosted => "none",
    };
    repo_dir.join("snapshots").join(component(revision))
}
