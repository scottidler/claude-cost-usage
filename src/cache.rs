use chrono::NaiveDate;
use eyre::{Context, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::scanner::SessionFile;

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedDay {
    pub cost: f64,
    pub sessions: usize,
    pub mtime_hash: u64,
}

/// Compute a hash of file paths, mtimes, and sizes for cache invalidation
pub fn compute_mtime_hash(files: &[&SessionFile]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for f in files {
        f.path.to_string_lossy().hash(&mut hasher);
        let mtime_secs = f
            .mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        mtime_secs.hash(&mut hasher);
        f.size.hash(&mut hasher);
    }
    hasher.finish()
}

/// Get the cache directory (~/.cache/ccu/)
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("ccu"))
}

/// Try to load a cached day summary
pub fn load_cached_day(date: NaiveDate, mtime_hash: u64) -> Option<CachedDay> {
    let dir = cache_dir()?;
    let path = dir.join(format!("{}.json", date));

    let content = fs::read_to_string(&path).ok()?;
    let cached: CachedDay = serde_json::from_str(&content).ok()?;

    if cached.mtime_hash == mtime_hash {
        info!("Cache hit for {}", date);
        Some(cached)
    } else {
        info!("Cache miss for {} (hash mismatch)", date);
        None
    }
}

/// Save a day summary to the cache
pub fn save_cached_day(date: NaiveDate, cost: f64, sessions: usize, mtime_hash: u64) -> Result<()> {
    let dir = match cache_dir() {
        Some(d) => d,
        None => return Ok(()),
    };

    fs::create_dir_all(&dir).context("Failed to create cache directory")?;

    let cached = CachedDay {
        cost,
        sessions,
        mtime_hash,
    };

    let path = dir.join(format!("{}.json", date));
    let content = serde_json::to_string(&cached).context("Failed to serialize cache")?;
    fs::write(&path, content).context("Failed to write cache file")?;

    info!("Cached day {} to {}", date, path.display());
    Ok(())
}

/// Remove stale cache entries older than the given number of days
pub fn prune_cache(keep_days: u32) -> Result<()> {
    let dir = match cache_dir() {
        Some(d) => d,
        None => return Ok(()),
    };

    if !dir.exists() {
        return Ok(());
    }

    let cutoff = chrono::Local::now().date_naive() - chrono::Duration::days(i64::from(keep_days));

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && let Ok(date) = stem.parse::<NaiveDate>()
            && date < cutoff
            && let Err(e) = fs::remove_file(&path)
        {
            warn!("Failed to prune cache file {}: {}", path.display(), e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::SessionFile;
    use std::time::SystemTime;

    #[test]
    fn test_compute_mtime_hash_deterministic() {
        let files = [SessionFile {
            path: PathBuf::from("/tmp/test.jsonl"),
            mtime: SystemTime::UNIX_EPOCH,
            size: 1024,
        }];
        let refs: Vec<&SessionFile> = files.iter().collect();
        let h1 = compute_mtime_hash(&refs);
        let h2 = compute_mtime_hash(&refs);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_mtime_hash_changes_with_size() {
        let f1 = [SessionFile {
            path: PathBuf::from("/tmp/test.jsonl"),
            mtime: SystemTime::UNIX_EPOCH,
            size: 1024,
        }];
        let f2 = [SessionFile {
            path: PathBuf::from("/tmp/test.jsonl"),
            mtime: SystemTime::UNIX_EPOCH,
            size: 2048,
        }];
        let r1: Vec<&SessionFile> = f1.iter().collect();
        let r2: Vec<&SessionFile> = f2.iter().collect();
        assert_ne!(compute_mtime_hash(&r1), compute_mtime_hash(&r2));
    }

    #[test]
    fn test_load_cached_day_miss() {
        let result = load_cached_day(NaiveDate::from_ymd_opt(1900, 1, 1).expect("valid date"), 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_load_cached_day() {
        let date = NaiveDate::from_ymd_opt(2099, 12, 31).expect("valid date");
        let hash = 42;

        save_cached_day(date, 14.23, 3, hash).expect("save");

        let loaded = load_cached_day(date, hash);
        assert!(loaded.is_some());
        let cached = loaded.expect("should be Some");
        assert!((cached.cost - 14.23).abs() < f64::EPSILON);
        assert_eq!(cached.sessions, 3);

        let loaded = load_cached_day(date, 999);
        assert!(loaded.is_none());

        // Cleanup
        if let Some(dir) = cache_dir() {
            let _ = fs::remove_file(dir.join(format!("{}.json", date)));
        }
    }
}
