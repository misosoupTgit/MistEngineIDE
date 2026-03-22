use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
    pub expanded: bool,
}

impl FileNode {
    pub fn from_dir(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_string();
        let mut dirs  = Vec::new();
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            for e in entries.flatten() {
                let ep = e.path();
                let en = ep.file_name().unwrap_or_default().to_string_lossy().to_string();
                if en.starts_with('.') || en == "target" || en == ".mist_cache" { continue; }
                if ep.is_dir() {
                    dirs.push(FileNode::from_dir(&ep).unwrap_or(FileNode {
                        name: en, path: ep, is_dir: true, children: vec![], expanded: false,
                    }));
                } else {
                    files.push(FileNode { name: en, path: ep, is_dir: false, children: vec![], expanded: false });
                }
            }
        }
        dirs.sort_by(|a,b| a.name.cmp(&b.name));
        files.sort_by(|a,b| a.name.cmp(&b.name));
        let mut children = dirs;
        children.extend(files);
        Some(FileNode { name, path: path.to_path_buf(), is_dir: true, children, expanded: true })
    }
}

pub struct ExplorerState {
    pub root: Option<FileNode>,
    pub selected: Option<PathBuf>,
}

impl ExplorerState {
    pub fn new() -> Self { ExplorerState { root: None, selected: None } }
    pub fn set_root(&mut self, path: &Path) { self.root = FileNode::from_dir(path); }
}
