//! 应用级配置。

/// 影响窗口创建与主循环的基础参数。
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub window_title: String,
    pub window_width: u32,
    pub window_height: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_title: "Neverland Rhapsody".to_owned(),
            window_width: 1280,
            window_height: 720,
        }
    }
}
