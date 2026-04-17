/// コンパイルキャッシュ
/// ソースのハッシュとタイムスタンプでキャッシュを管理する

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub source_hash: String,
    pub binary_path: PathBuf,
    pub timestamp: u64,
}

pub struct CompileCache {
    entries: HashMap<PathBuf, CacheEntry>,
    cache_dir: PathBuf,
}

impl CompileCache {
    pub fn new(project_dir: &Path) -> Self {
        let cache_dir = project_dir.join(".mist_cache");
        CompileCache {
            entries: HashMap::new(),
            cache_dir,
        }
    }

    /// ソースファイルのSHA-256ハッシュを計算
    pub fn hash_source(source: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        source.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    /// キャッシュが有効かどうかを確認
    pub fn is_valid(&self, source_path: &Path, source_hash: &str) -> bool {
        if let Some(entry) = self.entries.get(source_path) {
            entry.source_hash == source_hash && entry.binary_path.exists()
        } else {
            false
        }
    }

    /// キャッシュエントリを取得
    pub fn get(&self, source_path: &Path) -> Option<&CacheEntry> {
        self.entries.get(source_path)
    }

    /// キャッシュエントリを更新
    pub fn update(&mut self, source_path: PathBuf, source_hash: String, binary_path: PathBuf) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.entries.insert(source_path, CacheEntry {
            source_hash,
            binary_path,
            timestamp,
        });
    }

    /// キャッシュディレクトリのパスを返す
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// キャッシュディレクトリを作成
    pub fn ensure_cache_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.cache_dir)
    }
}
