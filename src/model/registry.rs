//! 模型注册表：按名称加载、缓存并分发稳定模型 ID。
//!
//! 相同名称重复插入会复用原 ID 并替换资源内容，保证实体/实例引用不失效。
//! 加载失败只记录在 `errors`，由调用方决定回退策略（内置模型或旧占位几何）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::loader::load_model;
use super::types::{ModelAsset, ModelId};
use super::ModelLoadError;

/// 已加载模型的缓存与错误记录。
#[derive(Debug, Default)]
pub struct ModelRegistry {
    models: Vec<Arc<ModelAsset>>,
    by_name: HashMap<String, ModelId>,
    errors: Vec<ModelLoadError>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 扫描目录下所有 `.glb` 文件（按名称排序，保证稳定加载顺序）。
    pub fn load_from_dir(root: &Path) -> Self {
        let mut registry = Self::new();
        if !root.is_dir() {
            tracing::warn!(path = %root.display(), "模型目录不存在，将使用内置模型");
            return registry;
        }
        let mut entries: Vec<_> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"))
            })
            .collect();
        entries.sort();
        for path in entries {
            if let Err(error) = registry.load_file(&path) {
                tracing::error!(path = %path.display(), "模型加载失败: {error}");
                registry.errors.push(error);
            }
        }
        registry
    }

    /// 加载单个模型文件。
    pub fn load_file(&mut self, path: &Path) -> Result<ModelId, ModelLoadError> {
        let asset = load_model(path)?;
        Ok(self.insert(asset))
    }

    /// 插入或替换模型；返回稳定 ID。
    pub fn insert(&mut self, asset: ModelAsset) -> ModelId {
        let offset = asset.ground_offset();
        if offset.length() > 0.05 {
            tracing::warn!(
                model = %asset.name,
                offset = %offset,
                "模型脚底不在原点平面，已自动贴地补偿；建议在建模软件中把原点放到脚底"
            );
        }
        if let Some(id) = self.by_name.get(&asset.name).copied() {
            if let Some(slot) = self.models.get_mut(id.raw() as usize) {
                *slot = Arc::new(asset);
            }
            return id;
        }
        let id = ModelId::new(self.models.len() as u32);
        self.by_name.insert(asset.name.clone(), id);
        self.models.push(Arc::new(asset));
        id
    }

    /// 若名称尚未加载则注册内置模型，否则保留磁盘加载结果。
    pub fn register_builtin(&mut self, name: &str, asset: ModelAsset) -> bool {
        if self.by_name.contains_key(name) {
            return false;
        }
        self.insert(asset);
        true
    }

    pub fn get(&self, id: ModelId) -> Option<Arc<ModelAsset>> {
        self.models.get(id.raw() as usize).cloned()
    }

    /// 遍历全部已加载模型及其 ID，顺序与 `ModelId` 编号一致。
    pub fn assets(&self) -> impl Iterator<Item = (ModelId, &Arc<ModelAsset>)> {
        self.models
            .iter()
            .enumerate()
            .map(|(index, asset)| (ModelId::new(index as u32), asset))
    }

    pub fn by_name(&self, name: &str) -> Option<ModelId> {
        self.by_name.get(name).copied()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn errors(&self) -> &[ModelLoadError] {
        &self.errors
    }

    /// 已加载模型的统计信息，用于启动日志。
    pub fn summary(&self) -> String {
        let triangles: usize = self.models.iter().map(|asset| asset.triangle_count()).sum();
        format!(
            "{} 个模型、{} 个三角形、{} 个动画片段",
            self.models.len(),
            triangles,
            self.models
                .iter()
                .map(|asset| asset.animations.len())
                .sum::<usize>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy(name: &str) -> ModelAsset {
        ModelAsset {
            name: name.to_string(),
            meshes: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            skeleton: None,
            animations: Vec::new(),
        }
    }

    #[test]
    fn ids_are_stable_and_deduplicated_by_name() {
        let mut registry = ModelRegistry::new();
        let first = registry.insert(dummy("pig"));
        let second = registry.insert(dummy("pig"));
        assert_eq!(first, second);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.by_name("pig"), Some(first));
        assert!(registry.get(first).is_some());
    }

    #[test]
    fn builtin_does_not_override_loaded_asset() {
        let mut registry = ModelRegistry::new();
        let loaded = registry.insert(dummy("pig"));
        assert!(!registry.register_builtin("pig", dummy("pig")));
        assert_eq!(registry.by_name("pig"), Some(loaded));
        assert!(registry.register_builtin("cow", dummy("cow")));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn missing_directory_is_reported_but_empty() {
        let registry = ModelRegistry::load_from_dir(Path::new("definitely-missing-models"));
        assert!(registry.is_empty());
        assert!(registry.errors().is_empty());
    }
}
