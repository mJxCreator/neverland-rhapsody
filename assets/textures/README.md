# 纹理资源导入规范（v0.16）

本目录存放独立 PNG 源纹理。游戏启动时会自动生成 Texture Atlas，开发者不需要维护图集图片或 UV 坐标。

## 目录结构

```
assets/
└── textures/
    ├── blocks/      方块纹理（16×16 PNG）
    ├── items/       物品/工具图标（16×16 PNG，允许透明背景）
    ├── ui/          HUD 与界面部件（规范已定，代码待接入）
    ├── entities/    生物纹理预留目录
    └── manifests/
        ├── blocks.json  方块六面映射
        ├── items.json   物品图标映射
        └── ui.json      界面部件映射（规范已定，代码待接入）
```

资源根目录默认是 `assets`，可通过环境变量 `NEVERLAND_ASSET_ROOT` 指向其他目录。

## PNG 规格

- 方块与物品：宽高各自为 16 / 32 / 64 / 128 / 256 / 512 / 1024 像素，建议正方形。
- UI 控件：宽高各自为 1–2048 像素的**任意尺寸**，不要求 2 的幂，也不要求正方形（如 1040×80、1024×256）。
- 尺寸越界会明确报错并回退到占位纹理；同一图集内可以混用不同尺寸，打包器会按纹理实际宽高自动排布。
- 颜色空间为 RGBA8；RGB、灰度、调色板 PNG 会自动转换为 RGBA8，16 位深度会被拒绝。
- 物品图标建议保留透明背景；方块纹理通常不透明。
- 文件名为小写蛇形命名，例如 `grass_top.png`、`wooden_pickaxe.png`。

## 缺失与热重载

- 文件缺失或解码失败时使用程序生成的棋盘格占位纹理，并输出带路径的警告。
- 资源目录中的 PNG 会被自动扫描并加入图集；manifest 只负责“哪个方块/物品用哪张图”。
- 运行时约每秒轮询一次文件变化。重新装配成功后会刷新 GPU 图集并重建区块网格；若装配出现硬错误（解码失败、图集容量不足、manifest 冲突），则保留上一份有效纹理。

## 方块 manifest

```json
{
  "blocks": [
    {
      "id": 3,
      "name": "grass",
      "top": "grass_top",
      "bottom": "dirt",
      "east": "grass_side",
      "west": "grass_side",
      "north": "grass_side",
      "south": "grass_side",
      "default": "grass_side"
    }
  ]
}
```

`id` 对应方块注册表；`top` / `bottom` / `east` / `west` / `north` / `south` 指定六面纹理名，未填写的面回退到 `default`。

## 物品 manifest

```json
{
  "items": [
    { "id": 4, "name": "wooden_pickaxe", "icon": "wooden_pickaxe", "display_mode": "icon", "alpha": "preserve" }
  ]
}
```

- `icon` 指定 `items/` 下的 PNG 文件名（不含扩展名）。
- `display_mode` 可为 `icon` 或 `none`；`none` 表示该物品不显示图标。
- `alpha` 可为 `preserve` 或 `force_opaque`；`preserve` 保留 PNG 透明通道，`force_opaque` 忽略透明通道。

## UI 纹理规范（已接入）

> v0.17 起 UI 控件使用**独立图集**，支持非正方形大图（如 1024×256 长条面板）。
> 放入 `ui/` 的 PNG 会立即生效；缺失控件自动回退到代码绘制的纯色样式。
> 各控件的**规定长宽比、推荐出图尺寸与比例代码位置**以 [`src/ui/README.md`](../../src/ui/README.md) 为准。

UI 采用**可组合部件**：每个控件一张独立小图，面板/底板/进度条用九宫格拉伸，不在图片里烘焙文字或动态数值。
UI 纹理走独立图集，物品图标图集不受影响；宽高各自允许 1–2048 像素的任意尺寸，可以放心按实际版面导出 1040×80、1024×256 等非正方形图片。

### 目录与文件名

`ui/` 为单层目录（资源扫描器不递归子目录），同名控件用模块前缀区分：

| 文件 | 用途 | 建议尺寸 | 绘制方式 |
| --- | --- | --- | --- |
| `hotbar_bg.png` | 快捷栏整体底板 | 64×64 或 256×64 | 九宫格，slice 8 |
| `hotbar_slot.png` | 未选中槽位 | 32×32 或 64×64 | 整图拉伸 |
| `hotbar_slot_selected.png` | 选中槽位高亮 | 32×32 或 64×64 | 整图拉伸 |
| `inventory_panel.png` | 背包面板 | 128×128 或 1024×512 | 九宫格，slice 12 |
| `inventory_slot.png` | 背包槽位 | 32×32 或 64×64 | 整图拉伸 |
| `inventory_slot_hover.png` | 槽位悬停 | 32×32 或 64×64 | 整图拉伸 |
| `inventory_slot_selected.png` | 快捷栏槽位当前选中标记 | 32×32 或 64×64 | 整图拉伸 |
| `tooltip_bg.png` | 悬停详情背景 | 64×64 或 512×256 | 九宫格，slice 8 |
| `crafting_row.png` | 合成配方行 | 64×64 或 1024×128 | 九宫格，slice 6 |
| `crafting_row_hover.png` | 合成行悬停 | 64×64 或 1024×128 | 九宫格，slice 6 |
| `crafting_row_selected.png` | 合成行选中 | 64×64 或 1024×128 | 九宫格，slice 6 |
| `crosshair.png` | 准星 | 16×16 或 32×32 | 整图拉伸 |
| `heart_full.png` / `heart_half.png` / `heart_empty.png` | 生命值心形三态 | 16×16 或 64×64 | 整图拉伸 |
| `progress_bg.png` / `progress_fill.png` | 挖掘进度条底/填充 | 32×32 或 256×16 | 九宫格，slice 4 |
| `damage_vignette.png` | 受伤红边叠加层 | 64×64 或 1024×1024 | 九宫格，slice 16 |
| `death_overlay.png` | 死亡全屏遮罩 | 16×16 或 1024×1024 | 整图拉伸（纯半透明） |
| `death_panel.png` | 死亡面板 | 64×64 或 1024×512 | 九宫格，slice 8 |
| `button.png` / `button_hover.png` | 按钮常态/悬停 | 32×32 或 256×64 | 九宫格，slice 6 |

### manifest 格式

```json
{
  "ui": [
    { "name": "hotbar_bg", "image": "hotbar_bg", "draw": "nine_slice", "slice": [8, 8, 8, 8] },
    { "name": "hotbar_slot", "image": "hotbar_slot", "draw": "stretch" },
    { "name": "heart_full", "image": "heart_full", "draw": "stretch" }
  ]
}
```

- `name`：代码引用的控件键，必须唯一。
- `image`：`ui/` 下 PNG 文件名（不含扩展名；写 `.png` 也会被自动去除）。
- `draw`：`stretch` 整图缩放到目标矩形；`nine_slice` 四角保真、边与中心拉伸。
- `slice`：`[左, 上, 右, 下]` 像素，仅 `nine_slice` 必填；其余填 `[0, 0, 0, 0]` 或省略。

### 制作规则

- 宽高各自允许 1–2048 像素的任意尺寸，**不要求正方形或 2 的幂**（如 1040×80、1024×256）；RGBA8。
- UI 使用独立图集，单张最大 2048 边长；图集上限 4096，容量不足会明确报错。
- 九宫格的切边之和必须小于图片对应的边长，四边各至少 1 像素。
- 采样为最近邻且不做插值，像素艺术请按整数倍放大使用；代码按布局像素矩形绘制。
- 透明背景保留；遮罩、悬停高亮、受伤红边等叠加层用半透明像素。
- 数量、名称、提示、悬停详情等文字继续由内联位图字体绘制，不要烘焙进图片。
- 图片内不要包含随状态变化的数字或进度，进度填充由代码裁剪目标矩形。

### 已实现的绘制行为

1. `TextureType::Ui` 独立图集与 `UiTextureMap` 解析 `manifests/ui.json`，缺失清单时使用内置控件表。
2. HUD 着色器同时绑定物品图集与 UI 图集，顶点 `atlas` 属性区分采样源；纯色顶点仍使用 UV 哨兵。
3. 快捷栏/背包/合成/生存 HUD/死亡界面的对应控件按 UV 采样，九宫格按目标矩形切分为 9 个四边形。
4. 未提供 PNG 的控件自动回退纯色样式；`ui/` 下额外 PNG 会自动登记为整图拉伸控件。
5. 文字层与图标层的绘制顺序保持不变；热重载会重新打包 UI 图集并立即生效。

## 纹理规格速查

- 单张纹理：方块与物品宽高各自 16 / 32 / 64 / 128 / 256 / 512 / 1024；UI 宽高各自 1–2048 任意像素；RGBA8。
- 图集采样：最近邻，不使用 mipmap，避免像素艺术发糊。
- 每张纹理四周自动加入 1 像素间隔，并复制边缘像素，防止采样串色。
- 图集按“高、宽从大到小、名称升序”确定性排布，最大边长 4096；容量不足会返回明确错误，不会静默丢弃纹理。
