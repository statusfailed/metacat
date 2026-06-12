use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::Url;

#[derive(Clone, Debug)]
pub struct ProjectContext {
    pub texts: Vec<String>,
}

impl ProjectContext {
    pub fn standalone(text: String) -> Self {
        Self { texts: vec![text] }
    }
}

pub fn context_for_document(uri: &Url, current_text: &str) -> ProjectContext {
    let Ok(document_path) = uri.to_file_path() else {
        return ProjectContext::standalone(current_text.to_string());
    };

    let Some(project_root) = nearest_manifest_root(&document_path) else {
        return ProjectContext::standalone(current_text.to_string());
    };

    let manifest = project_root.join("metacat.json");
    let Ok(entries) = manifest_entries(&manifest) else {
        return ProjectContext::standalone(current_text.to_string());
    };

    let mut paths = BTreeSet::new();
    for entry in entries {
        for path in expand_manifest_entry(&entry.root, &entry.pattern) {
            paths.insert(normalize_path(path));
        }
    }
    paths.insert(normalize_path(document_path.clone()));

    let document_path = normalize_path(document_path);
    let mut texts = Vec::new();
    for path in paths {
        if path == document_path {
            texts.push(current_text.to_string());
        } else if let Ok(text) = fs::read_to_string(&path) {
            texts.push(text);
        }
    }

    if texts.is_empty() {
        ProjectContext::standalone(current_text.to_string())
    } else {
        ProjectContext { texts }
    }
}

fn nearest_manifest_root(document_path: &Path) -> Option<PathBuf> {
    let mut directory = if document_path.is_dir() {
        document_path
    } else {
        document_path.parent()?
    };

    loop {
        if directory.join("metacat.json").is_file() {
            return Some(directory.to_path_buf());
        }
        directory = directory.parent()?;
    }
}

#[derive(Clone, Debug)]
struct ManifestEntry {
    root: PathBuf,
    pattern: String,
}

fn manifest_entries(path: &Path) -> Result<Vec<ManifestEntry>, String> {
    let manifest_root = path
        .parent()
        .ok_or_else(|| "metacat.json path has no parent".to_string())?;
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;

    let mut entries = Vec::new();
    if let Some(files) = json.get("files").and_then(Value::as_array) {
        entries.extend(files.iter().filter_map(Value::as_str).map(|pattern| {
            ManifestEntry {
                root: manifest_root.to_path_buf(),
                pattern: pattern.to_string(),
            }
        }));
    }

    if let Some(includes) = json.get("include").and_then(Value::as_array) {
        for include in includes {
            let Some(folder) = include.get("folder").and_then(Value::as_str) else {
                continue;
            };
            let Some(files) = include.get("files").and_then(Value::as_array) else {
                continue;
            };
            let root = resolve_manifest_path(manifest_root, folder);
            entries.extend(files.iter().filter_map(Value::as_str).map(|pattern| {
                ManifestEntry {
                    root: root.clone(),
                    pattern: pattern.to_string(),
                }
            }));
        }
    }

    if entries.is_empty() {
        return Err("metacat.json must contain files or include entries".to_string());
    }

    Ok(entries)
}

fn expand_manifest_entry(root: &Path, pattern: &str) -> Vec<PathBuf> {
    if !pattern.contains('*') && !pattern.contains('?') {
        return vec![resolve_manifest_path(root, pattern)];
    }

    let mut result = Vec::new();
    collect_matching_files(root, root, pattern, &mut result);
    result
}

fn resolve_manifest_path(root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn collect_matching_files(root: &Path, directory: &Path, pattern: &str, result: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_matching_files(root, &path, pattern, result);
            continue;
        }

        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if glob_matches(pattern, &relative) {
            result.push(path);
        }
    }
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    glob_parts_match(&pattern_parts, &path_parts)
}

fn glob_parts_match(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.split_first(), path.split_first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some((&"**", rest)), _) => {
            glob_parts_match(rest, path)
                || path
                    .split_first()
                    .is_some_and(|(_, path_rest)| glob_parts_match(pattern, path_rest))
        }
        (Some((part, rest)), Some((path_part, path_rest))) => {
            segment_matches(part, path_part) && glob_parts_match(rest, path_rest)
        }
        (Some(_), None) => false,
    }
}

fn segment_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    segment_matches_from(&pattern, &text)
}

fn segment_matches_from(pattern: &[char], text: &[char]) -> bool {
    match (pattern.split_first(), text.split_first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some((&'*', rest)), _) => {
            segment_matches_from(rest, text)
                || text
                    .split_first()
                    .is_some_and(|(_, text_rest)| segment_matches_from(pattern, text_rest))
        }
        (Some((&'?', rest)), Some((_, text_rest))) => segment_matches_from(rest, text_rest),
        (Some((pattern_char, rest)), Some((text_char, text_rest))) => {
            pattern_char == text_char && segment_matches_from(rest, text_rest)
        }
        (Some(_), None) => false,
    }
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn glob_supports_recursive_hex_patterns() {
        assert!(glob_matches("stdlib/**/*.hex", "stdlib/type/core.hex"));
        assert!(glob_matches("*.hex", "fol.hex"));
        assert!(!glob_matches("*.hex", "nested/fol.hex"));
    }

    #[test]
    fn nearest_manifest_loads_project_files_and_overlays_current_document() {
        let root = std::env::temp_dir().join(format!(
            "metacat-lsp-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let child = root.join("examples");
        fs::create_dir_all(&child).unwrap();
        fs::write(root.join("metacat.json"), r#"{"files":["base.hex"]}"#).unwrap();
        fs::write(root.join("base.hex"), "(theory base nat {})").unwrap();
        let current = child.join("case.hex");
        fs::write(&current, "old").unwrap();

        let uri = Url::from_file_path(&current).unwrap();
        let context = context_for_document(&uri, "new unsaved text");

        assert_eq!(context.texts.len(), 2);
        assert!(context.texts.iter().any(|text| text == "(theory base nat {})"));
        assert!(context.texts.iter().any(|text| text == "new unsaved text"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_include_loads_files_relative_to_included_folder() {
        let root = std::env::temp_dir().join(format!(
            "metacat-lsp-include-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let shared = root.join("shared");
        let project = root.join("project");
        fs::create_dir_all(shared.join("stdlib")).unwrap();
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(
            project.join("metacat.json"),
            r#"{
  "files": ["src/current.hex"],
  "include": [
    {
      "folder": "../shared",
      "files": ["stdlib/**/*.hex"]
    }
  ]
}"#,
        )
        .unwrap();
        fs::write(shared.join("stdlib/base.hex"), "(theory shared nat {})").unwrap();
        let current = project.join("src/current.hex");
        fs::write(&current, "old").unwrap();

        let uri = Url::from_file_path(&current).unwrap();
        let context = context_for_document(&uri, "new current");

        assert_eq!(context.texts.len(), 2);
        assert!(context.texts.iter().any(|text| text == "(theory shared nat {})"));
        assert!(context.texts.iter().any(|text| text == "new current"));

        fs::remove_dir_all(root).unwrap();
    }
}
