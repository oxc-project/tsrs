use std::collections::BTreeSet;
use std::ops::Bound;
use std::sync::Arc;
use std::time::SystemTime;

use tsrs_core::tspath;
use tsrs_vfs::{Entries, FS, FileInfo, FileMode};

use crate::Files;

pub fn normalize(path: &str) -> String {
    tspath::normalize_path(path)
}

/// Read-only overlay. Original emitted source files are hidden from the second
/// Program so resolution cannot accidentally bind original .ts instead of .d.ts.
pub struct DeclarationFs {
    pub base: Arc<dyn FS>,
    pub files: Files,
    pub hidden: BTreeSet<String>,
}

impl DeclarationFs {
    pub fn key(&self, path: &str) -> String {
        tspath::get_canonical_file_name(&normalize(path), self.use_case_sensitive_file_names())
    }

    fn virtual_directory(&self, path: &str) -> bool {
        let prefix = format!("{}/", self.key(path).trim_end_matches('/'));
        self.files
            .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
            .next()
            .is_some_and(|(name, _)| name.starts_with(&prefix))
    }
}

impl FS for DeclarationFs {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.base.use_case_sensitive_file_names()
    }
    fn file_exists(&self, path: &str) -> bool {
        let path = self.key(path);
        self.files.contains_key(&path)
            || (!self.hidden.contains(&path) && self.base.file_exists(&path))
    }
    fn read_file(&self, path: &str) -> Option<String> {
        let path = self.key(path);
        self.files.get(&path).cloned().or_else(|| {
            (!self.hidden.contains(&path)).then(|| self.base.read_file(&path)).flatten()
        })
    }
    fn directory_exists(&self, path: &str) -> bool {
        self.virtual_directory(path) || self.base.directory_exists(path)
    }
    fn get_accessible_entries(&self, path: &str) -> Entries {
        let path = self.key(path);
        let mut entries = self.base.get_accessible_entries(&path);
        entries.files.retain(|name| {
            !self.hidden.contains(&self.key(&tspath::combine_paths(&path, &[name])))
        });
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let mut previous_directory = None;
        for (file, _) in self
            .files
            .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
            .take_while(|(name, _)| name.starts_with(&prefix))
        {
            let tail = &file[prefix.len()..];
            if let Some((directory, _)) = tail.split_once('/') {
                if previous_directory != Some(directory) {
                    entries.directories.push(directory.into());
                    previous_directory = Some(directory);
                }
            } else {
                entries.files.push(tail.into());
            }
        }
        entries.files.sort();
        entries.files.dedup();
        entries.directories.sort();
        entries.directories.dedup();
        entries
    }
    fn stat(&self, path: &str) -> Option<FileInfo> {
        let path = self.key(path);
        if let Some(text) = self.files.get(&path) {
            return Some(FileInfo {
                name: tspath::get_base_file_name(&path),
                size: text.len().try_into().ok()?,
                mode: FileMode::None,
                mod_time: None,
            });
        }
        if self.hidden.contains(&path) {
            return None;
        }
        self.base.stat(&path).or_else(|| {
            self.virtual_directory(&path).then(|| FileInfo {
                name: tspath::get_base_file_name(&path),
                size: 0,
                mode: FileMode::Dir,
                mod_time: None,
            })
        })
    }
    fn realpath(&self, path: &str) -> String {
        let path = self.key(path);
        if self.files.contains_key(&path) || self.virtual_directory(&path) {
            path
        } else {
            self.base.realpath(&path)
        }
    }
    fn write_file(&self, _: &str, _: &str) -> std::result::Result<(), String> {
        Err("declaration filesystem is read-only".into())
    }
    fn append_file(&self, _: &str, _: &str) -> std::result::Result<(), String> {
        Err("declaration filesystem is read-only".into())
    }
    fn remove(&self, _: &str) -> std::result::Result<(), String> {
        Err("declaration filesystem is read-only".into())
    }
    fn chtimes(&self, _: &str, _: SystemTime, _: SystemTime) -> std::result::Result<(), String> {
        Err("declaration filesystem is read-only".into())
    }
}
