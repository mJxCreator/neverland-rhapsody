# src/ui — UI 贴图比例与绘制规范

本文档是 UI 贴图“长宽比契约”的人工可读版本：**每个控件按什么比例出图、比例写在代码哪里、布局尺寸在哪里改**。
代码是唯一事实源，文档只做索引；两者不一致时同步更新。

## 1. 规则

- **图片比例优先**：以你实际出图的比例为准（如 16×16 取最简 1:1），再把该比例写回 `texture_ratio.rs` 的常量；
  代码据此把 1:1 等比映射到 44×44、48×48 等任意目标尺寸，不做单方向拉伸。
- **代码比例可改**：比例常量集中暴露在 [texture_ratio.rs](texture_ratio.rs)，改常量即可适配新图片，无需改布局。
- **固定框控件**：比例在 [texture_ratio.rs](texture_ratio.rs) 的 `expected_ratio` 中集中规定。
  绘制时先把目标矩形收敛到该比例（等比 contain、居中留白）再采样，**整图拉伸与九宫格都适用**，
  不会出现任何单方向拉伸。你按表内比例出图，代码就按同一比例绘制。
- **九宫格做到 1:1**：按“目标像素尺寸”出图（如 `hotbar_bg` 184×24）时，切边与中心都能与屏幕像素一一对应，完全不变形；
  按整数倍放大出图时，请同步按倍数修改 `slice`。
- **快捷栏三图关系固定**：`hotbar_slot` / `hotbar_slot_selected` / `hotbar_bg` 的比例与位置关系
  集中在 [modules/hotbar_geometry.rs](modules/hotbar_geometry.rs)：槽位 `16n`、选中 `18n`、间距 `4n`、外侧留白 `4n`、
  底板 `184n×24n`，选中与槽位同心。只改 `SLOT_SIZE`（即 n）即可整体缩放，不要在界面里另写常数。
- **动态比例控件**：比例随文字内容、进度或窗口尺寸变化（提示框、进度填充、合成行、全屏遮罩、共用面板等），
  在代码中返回 `None`，随目标矩形铺满；这类控件无法用单一比例约束。
- 启动时若 PNG 实际比例与规定不一致（偏差 > 1%），会输出带控件名和规定比例的警告。

## 2. 受比例约束的控件

| 控件键 | 规定比例 | 推荐出图尺寸 | 绘制位置 |
| --- | --- | --- | --- |
| `hotbar_bg` | 23 : 3 | **184×24**（或 368×48 / 552×72，`slice` 同比放大） | `modules/hotbar_geometry.rs` `background_rect`，由 `hud.rs` `push_hotbar` 调用 |
| `progress_bg` | 20 : 1 | **240×12**（或 480×24，`slice` 改 8） | `hud.rs` `push_progress`（`PROGRESS_WIDTH/HEIGHT`） |
| `hotbar_slot` | 1 : 1 | 16×16（游戏内 16n） | `widget.rs` `push_slot`，快捷栏 `hud.rs` `SLOT_SIZE` |
| `hotbar_slot_selected` | 1 : 1 | 18×18（游戏内 18n，与槽位同心） | `modules/hotbar_geometry.rs` `selected_rect`，由 `widget.rs` `skin_rect` 调用 |
| `inventory_slot` | 1 : 1 | 同上 | `widget.rs` `push_slot`，背包/合成/烹饪槽位 |
| `inventory_slot_hover` | 1 : 1 | 同上 | 同上 |
| `inventory_slot_selected` | 1 : 1 | 同上 | 同上 |
| `crosshair` | 1 : 1 | 16×16 / 32×32 / 64×64 | `hud.rs` `push_crosshair`（边长 `(CROSSHAIR_ARM + 4) × 2`） |
| `heart_full` | 1 : 1 | 16×16 / 32×32 / 64×64 | `hud.rs` `push_heart`（`HEART_PATTERN.len() × HEART_PIXEL`） |
| `heart_half` | 1 : 1 | 同上 | 同上 |
| `heart_empty` | 1 : 1 | 同上 | 同上 |

修改比例：编辑 `texture_ratio.rs` 中的 `HOTBAR_BG_RATIO` / `PROGRESS_BG_RATIO` / `SLOT_RATIO` /
`CROSSHAIR_RATIO` / `HEART_RATIO`，或在 `expected_ratio` 的 `match` 中增删控件键。
返回 `None` 表示随目标矩形自由铺满。

> 例：`hotbar_bg` 当前按 23:3（184×24）绘制，底板矩形由快捷栏几何直接生成，与 9 格槽位区严丝合缝；
> 若换成 8:1 出图，需要同时改 `modules/hotbar_geometry.rs` 的单位与 `BACKGROUND_RATIO`，否则启动会告警。
>
> 快捷栏几何（[modules/hotbar_geometry.rs](modules/hotbar_geometry.rs)，n = 槽位贴图宽 / 16，当前 `SLOT_SIZE = 48`，即 n = 3）：
> 槽位 `16n×16n`、间距 `4n`、外侧留白 `4n`、底板 `184n×24n`（552×72）；
> 选中槽位 `18n×18n`，与普通槽位同心，因此会比槽位每边多出 `1n`（3px）。
> 三者的相对关系固定，改 n 即整体缩放。

## 3. 不锁定比例的控件

这些控件在不同界面/内容下目标比例不同，强行锁定会破坏排版，因此随矩形铺满；九宫格由四角像素保形。

| 控件键 | 推荐出图尺寸 | `slice` | 比例变化原因 |
| --- | --- | --- | --- |
| `inventory_panel` | 532×298（背包）与 900×620（合成） | 12 | 同一张图被背包与合成面板复用，两者比例不同 |
| `tooltip_bg` | 128×64 起 | 8 | 宽高随提示文字行数与长度变化 |
| `crafting_row` | 高度 44，宽度随面板 | 6 | 列表宽度随面板缩放，高度固定 |
| `crafting_row_hover` | 同上 | 6 | 同上 |
| `crafting_row_selected` | 同上 | 6 | 同上 |
| `progress_fill` | 236×8（满进度） | 4 | 填充宽度随挖掘/进食进度变化 |
| `damage_vignette` | 512×512 / 1024×1024 | 16 | 全屏叠加，比例随窗口 |
| `death_overlay` | 任意，建议方形纯色/渐变 | 无 | 全屏遮罩，比例随窗口 |
| `death_panel` | 720×446（1280×720 下） | 8 | 宽随视口、高为视口 62% |
| `button` | 250×48（死亡）/ 180×44（合成） | 6 | 同一张图被两种按钮尺寸复用 |
| `button_hover` | 同上 | 6 | 同上 |

九宫格只要求切边之和小于图片对应边长（四边各至少 1 像素）；切边不合法会自动降级为整图拉伸并告警。

## 4. 代码位置索引

| 要改什么 | 文件 | 位置 |
| --- | --- | --- |
| 控件的比例 | `src/ui/texture_ratio.rs` | `HOTBAR_BG_RATIO`、`PROGRESS_BG_RATIO`、`SLOT_RATIO`、`CROSSHAIR_RATIO`、`HEART_RATIO`、`expected_ratio` |
| 比例收敛算法（contain + 居中） | `src/ui/texture_ratio.rs` | `fit_rect` |
| 比例如何应用到绘制 | `src/ui/draw.rs` | `push_ui_view`（整图拉伸与九宫格都先收敛比例） |
| 九宫格切边与绘制方式 | `src/texture/ui_texture.rs` | `builtin_manifest`；运行时以 `assets/textures/manifests/ui.json` 为准 |
| 比例不符告警 | `src/texture/ui_texture.rs` | `UiTextureMap::from_manifest`、`ratio_mismatch` |
| 快捷栏三图的固定比例与位置关系 | `src/ui/modules/hotbar_geometry.rs` | `SLOT_UNITS`、`SELECTED_UNITS`、`GAP_UNITS`、`MARGIN_UNITS`、`BACKGROUND_RATIO`、`HotbarGeometry` |
| HUD 尺寸 | `src/ui/hud.rs` | `SLOT_SIZE`（决定 n）、`BOTTOM_MARGIN`、`PROGRESS_WIDTH/HEIGHT`、`HEART_PIXEL`、`CROSSHAIR_ARM` |
| 背包面板尺寸 | `src/ui/inventory.rs` | `BASE_SLOT`、`BASE_GAP`、`BASE_PADDING`、`BASE_TITLE` 等 `Layout::compute` |
| 合成面板尺寸 | `src/ui/crafting.rs` | `BASE_PANEL_WIDTH/HEIGHT`、`ROW_HEIGHT`、`CRAFT_WIDTH/HEIGHT` |
| 烹饪面板尺寸 | `src/ui/cooking.rs` | `layout`（面板 560×320、槽位 56、进度条高 16） |
| 死亡界面尺寸 | `src/ui/death.rs` | `BUTTON_WIDTH/HEIGHT`、`DeathUi::build` 中的面板比例 |

`assets/textures/manifests/ui.json` 只负责“用哪张图、切边多少、是否九宫格”，不参与长宽比约束；
新增 PNG 若未在清单中登记，会自动按“整图拉伸、不约束比例”处理。

## 5. 出图与验收建议

- 受约束控件严格按表内比例导出；想完全 1:1 就按“推荐出图尺寸”导出。
- 九宫格控件若放大出图，`slice` 要同比放大，否则四角与中心的像素映射会不一致。
- 透明背景保留；文字与动态数字不要烘焙进图片；像素风用整数倍放大并保持最近邻采样。
- 改动比例或切边后运行 `cargo test` 与 `cargo clippy --all-targets`，并实机确认快捷栏底板、进度条、槽位、心形、准星与各面板无变形、无接缝。
