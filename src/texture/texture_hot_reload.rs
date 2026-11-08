//! 纹理热重载监听：轮询 PNG 与 manifest 的修改时间，发现变化后由上层重建资源。
//!
//! 不引入额外文件系统监听依赖；轮询频率由上层控制，热重载失败时由上层保留
//! 上一份有效纹理，避免开发期改图导致画面丢失。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::texture_assets::watched_png_files;

/// 记录资源文件修改时间的轻量监听器。
#[derive(Debug)]
pub struct TextureWatcher {
    asset_root: PathBuf,
    fingerprints: BTreeMap<PathBuf, Option<SystemTime>>,
}

impl TextureWatcher {
    /// 创建监听器并记录当前资源状态。
    pub fn new(asset_root: impl AsRef<Path>) -> Self {
        let asset_root = asset_root.as_ref().to_path_buf();
        let fingerprints = collect(&asset_root);
        Self {
            asset_root,
            fingerprints,
        }
    }

    /// 轮询一次；资源发生新增、删除或修改时返回 `true`。
    pub fn poll(&mut self) -> bool {
        let current = collect(&self.asset_root);
        let changed = current != self.fingerprints;
        if changed {
            self.fingerprints = current;
        }
        changed
    }

    /// 当前被监听的文件数量，用于诊断。
    pub fn watched_count(&self) -> usize {
        self.fingerprints.len()
    }
}

fn collect(asset_root: &Path) -> BTreeMap<PathBuf, Option<SystemTime>> {
    watched_png_files(asset_root)
        .into_iter()
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok();
            (path, modified)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "neverland-rhapsody-watch-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("textures/blocks")).expect("创建目录");
        fs::create_dir_all(dir.join("textures/manifests")).expect("创建目录");
        dir
    }

    #[test]
    fn unchanged_assets_report_no_reload() {
        let dir = unique_dir("stable");
        fs::write(dir.join("textures/blocks/stone.png"), b"placeholder").expect("写文件");
        let mut watcher = TextureWatcher::new(&dir);
        assert!(!watcher.poll(), "未修改时不应触发重载");
        assert_eq!(watcher.watched_count(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn new_files_and_manifest_changes_trigger_reload() {
        let dir = unique_dir("change");
        let mut watcher = TextureWatcher::new(&dir);

        fs::write(dir.join("textures/blocks/stone.png"), b"one").expect("写文件");
        assert!(watcher.poll(), "新增纹理应触发重载");
        assert!(!watcher.poll(), "刚读取过的状态不应再次触发");

        fs::write(dir.join("textures/manifests/items.json"), b"{}").expect("写 manifest");
        assert!(watcher.poll(), "manifest 变化应触发重载");

        fs::remove_dir_all(&dir).ok();
    }
}
