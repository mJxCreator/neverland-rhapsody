//! 挖掘/进食进度条模块：优先采样 `progress_bg` / `progress_fill` 贴图。

use crate::texture::UiTextureMap;
use crate::ui::core::draw::{push_quad, push_ui_view, UiVertex};
use crate::ui::core::layout::UiRect;
use crate::ui::core::theme;

/// 进度条参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressConfig {
    /// 进度条宽度（像素）。
    pub width: f32,
    /// 进度条高度（像素）。
    pub height: f32,
    /// 与屏幕中心的垂直偏移（像素）。
    pub offset: f32,
    /// 内边距（像素）。
    pub padding: f32,
    /// 缺失底板贴图时的回退底色。
    pub background: [f32; 4],
    /// 缺失填充贴图时的回退色。
    pub fill: [f32; 4],
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            width: 240.0,
            height: 12.0,
            offset: 34.0,
            padding: 2.0,
            background: theme::PROGRESS_BACKGROUND,
            fill: theme::PROGRESS_FILL,
        }
    }
}

/// 进度条运行时状态；进食进度优先于挖掘进度。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProgressBar {
    /// 当前挖掘进度；`None` 表示不显示。
    pub mining_progress: Option<f32>,
    /// 当前进食进度；`None` 表示不显示。
    pub eating_progress: Option<f32>,
    /// 可调参数。
    pub config: ProgressConfig,
}

impl ProgressBar {
    /// 写入挖掘进度；非有限值视为不显示。
    pub fn set_mining(&mut self, progress: Option<f32>) {
        self.mining_progress = progress.filter(|value| value.is_finite());
    }

    /// 写入进食进度；非有限值视为不显示。
    pub fn set_eating(&mut self, progress: Option<f32>) {
        self.eating_progress = progress.filter(|value| value.is_finite());
    }

    /// 当前应显示的进度。
    pub fn visible_progress(&self) -> Option<f32> {
        self.eating_progress.or(self.mining_progress)
    }

    /// 绘制进度条。
    pub fn draw(&self, out: &mut Vec<UiVertex>, width: f32, height: f32, ui: &UiTextureMap) {
        let Some(progress) = self.visible_progress() else {
            return;
        };
        let viewport = (width, height);
        let config = &self.config;
        let progress = progress.clamp(0.0, 1.0);
        let x = (width - config.width) * 0.5;
        let y = height * 0.5 + config.offset;
        let bar = UiRect::new(x, y, config.width, config.height);

        if let Some(view) = ui.get("progress_bg") {
            push_ui_view(out, viewport, bar, view, [1.0; 4]);
        } else {
            push_quad(out, viewport, bar, config.background);
        }

        let fill = UiRect::new(
            x + config.padding,
            y + config.padding,
            (config.width - config.padding * 2.0) * progress,
            config.height - config.padding * 2.0,
        );
        if let Some(view) = ui.get("progress_fill") {
            push_ui_view(out, viewport, fill, view, [1.0; 4]);
        } else {
            push_quad(out, viewport, fill, config.fill);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertices_for(progress: Option<f32>) -> usize {
        let bar = ProgressBar {
            mining_progress: progress,
            ..ProgressBar::default()
        };
        let mut out = Vec::new();
        bar.draw(&mut out, 1280.0, 720.0, &UiTextureMap::empty());
        out.len()
    }

    #[test]
    fn hidden_without_progress_and_eating_wins() {
        assert_eq!(vertices_for(None), 0);
        assert!(vertices_for(Some(0.3)) > 0);
        let bar = ProgressBar {
            mining_progress: Some(0.1),
            eating_progress: Some(0.9),
            ..ProgressBar::default()
        };
        assert_eq!(bar.visible_progress(), Some(0.9));
    }

    #[test]
    fn invalid_values_are_ignored() {
        let mut bar = ProgressBar::default();
        bar.set_mining(Some(f32::NAN));
        assert_eq!(bar.mining_progress, None);
    }
}
