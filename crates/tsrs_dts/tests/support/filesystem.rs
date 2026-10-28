use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::SystemTime;
use tsrs_vfs::{Entries, FS, FileInfo, vfstest};

pub fn memory_fs(files: &tsrs_dts::Files) -> (Arc<dyn FS>, Arc<AtomicUsize>) {
    let base: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(vfstest::from_map(
        files.iter().map(|(p, text)| (p.as_str(), text.as_str())),
        true,
    )));
    let writes = Arc::new(AtomicUsize::new(0));
    (Arc::new(NoWrites { base, writes: Arc::clone(&writes) }), writes)
}

pub struct NoWrites {
    pub base: Arc<dyn FS>,
    pub writes: Arc<AtomicUsize>,
}
impl NoWrites {
    fn deny(&self) -> std::result::Result<(), String> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Err("corpus filesystem rejects writes".into())
    }
}
impl FS for NoWrites {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.base.use_case_sensitive_file_names()
    }
    fn file_exists(&self, p: &str) -> bool {
        self.base.file_exists(p)
    }
    fn read_file(&self, p: &str) -> Option<String> {
        self.base.read_file(p)
    }
    fn directory_exists(&self, p: &str) -> bool {
        self.base.directory_exists(p)
    }
    fn get_accessible_entries(&self, p: &str) -> Entries {
        self.base.get_accessible_entries(p)
    }
    fn stat(&self, p: &str) -> Option<FileInfo> {
        self.base.stat(p)
    }
    fn realpath(&self, p: &str) -> String {
        self.base.realpath(p)
    }
    fn write_file(&self, _: &str, _: &str) -> std::result::Result<(), String> {
        self.deny()
    }
    fn append_file(&self, _: &str, _: &str) -> std::result::Result<(), String> {
        self.deny()
    }
    fn remove(&self, _: &str) -> std::result::Result<(), String> {
        self.deny()
    }
    fn chtimes(&self, _: &str, _: SystemTime, _: SystemTime) -> std::result::Result<(), String> {
        self.deny()
    }
}
