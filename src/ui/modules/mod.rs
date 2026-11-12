//! 封装好的 UI 模块：一个模块 = 一个界面/控件的完整封装（几何、状态、绘制）。
//!
//! 约定：
//! - 跨界面共享的基础设施（`layout`、`draw`、`widget`、`theme`、`font`、`texture_ratio`）留在 `src/ui` 根。
//! - 模块只暴露少量 `pub` API，内部实现不跨模块引用。
//! - 新增模块必须同步 `src/ui/codemap.md` 与本文件列表。

pub mod hotbar_geometry;
