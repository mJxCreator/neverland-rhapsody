# 纹理资源导入规范（v0.16）

本目录存放独立 PNG 源纹理。游戏启动时会自动生成 Texture Atlas，开发者不需要维护图集图片或 UV 坐标。

## 目录结构

```
assets/
└── textures/
    ├── blocks/      方块纹理（16×16 PNG）
    ├── items/       物品/工具图标（16×16 PNG，允许透明背景）
    ├── entities/    生物纹理预留目录
    └── manifests/
        ├── blocks.json  方块六面映射
        └── items.json   物品图标映射
```

资源根目录默认是 `assets`，可通过环境变量 `NEVERLAND_ASSET_ROOT` 指向其他目录。

## PNG 规格

- 尺寸支持 16×16、32×32、64×64、128×128 的正方形；其他尺寸会明确报错并回退到占位纹理。
- 同一图集内可以混用不同尺寸，打包器会按纹理边长自动排布。
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

## 纹理规格速查

- 单张纹理：16×16 / 32×32 / 64×64 / 128×128 正方形，RGBA8。
- 图集采样：最近邻，不使用 mipmap，避免像素艺术发糊。
- 每张纹理四周自动加入 1 像素间隔，并复制边缘像素，防止采样串色。
- 图集按“边长从大到小、名称升序”确定性排布，最大边长 2048；容量不足会返回明确错误，不会静默丢弃纹理。
