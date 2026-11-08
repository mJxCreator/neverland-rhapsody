# 模型资源规范（v0.18）

本目录存放生物与静态物体的 `.glb` 模型。游戏启动时扫描 `assets/models/` 并注册到
`ModelRegistry`；注册失败只记录错误，实体继续使用回退几何，不会中断启动。

模型根目录默认是 `assets/models`，随 `NEVERLAND_ASSET_ROOT` 环境变量一起变化。

## 目录结构

```
assets/
├── models/
│   ├── pig.glb       猪模型（可选，缺失时使用内置程序化模型）
│   └── cow.glb       牛模型（可选，缺失时使用内置程序化模型）
└── textures/
    └── entities/     模型外部 PNG 纹理
```

- 同一模型被任意数量的实体共享，不会按实体重复加载。
- 文件名（不含扩展名）即模型注册名；实体按 `pig` / `cow` 名称查找。
- 磁盘上的 GLB 优先；同名内置模型只在目录中不存在该文件时注册。

## 坐标、单位与原点

| 项目 | 约定 |
| --- | --- |
| 坐标系 | 右手系，Y 轴向上 |
| 前方 | 模型本地 +Z |
| 单位 | 1 单位 = 1 米 = 1 方块 |
| 原点 | 双脚中心，落在 `y = 0` 平面 |
| 水平居中 | 模型在 X/Z 轴以原点为中心 |

glTF 与引擎坐标约定一致，导入阶段**不做旋转或镜像**。若模型在 Blender 中制作，
导出前请将 Y 设为向上，并保证角色朝向 +Z；否则游戏内会出现朝向或上下颠倒。

实体碰撞盒由实体注册表单独定义，模型外观不改变物理规则。模型尺寸建议与碰撞盒接近：
猪约 0.9×0.9×1.0，牛约 1.1×1.4×1.2。

## 网格与材质

- 仅支持 `TRIANGLES` 图元；需要 `POSITION`，可选 `NORMAL`、`TEXCOORD_0`、`TANGENT`。
- 缺少 `NORMAL` 时按三角形平面计算法线；缺少 UV 时使用 `(0, 0)`。
- 支持多个网格节点、每个网格多个图元、多个材质；材质通过 `material` 下标引用。
- `baseColorFactor`、`baseColorTexture`、`metallicFactor`、`roughnessFactor` 会被读取。
- `alphaMode` 支持 `OPAQUE` / `MASK`（`alphaCutoff`）/ `BLEND`；未知值按不透明处理。
- `doubleSided` 会记录，但运行时统一关闭背面剔除，避免低模出现缺面。
- 纹理只使用 UV0；`TEXCOORD_1` 及以上会被忽略并标记占位材质。

## 纹理

- 首选把 PNG 内嵌进 GLB（`images[i].bufferView` + `mimeType: image/png`），保证单文件稳定。
- 也可以使用外部 PNG：`images[i].uri` 写相对本目录的路径，例如
  `../textures/entities/pig.png`。
- 纹理尺寸 1–4096 像素，RGBA8；运行时不使用方块图集，允许每张模型纹理独立布局。
- 图片解码失败时该材质标记为占位并在日志中输出具体路径，模型其余部分仍可显示。
- 不使用 data URI（`data:image/png;base64,...`），会明确报错。

## 骨骼与蒙皮

- 本版本每个模型只支持一个 `skin`；多个 skin 会返回可读错误。
- 关节使用 `JOINTS_0` + `WEIGHTS_0`；每个顶点最多 4 个骨骼权重，权重会自动归一化。
- 骨骼层级由节点父子关系推导；中间的非关节节点变换会折叠进关节局部变换。
- 提供 `inverseBindMatrices` 时直接使用，否则按节点世界矩阵求逆生成。
- 没有 `JOINTS_0` / `WEIGHTS_0` 的网格按静态网格渲染，节点变换烘焙进顶点。

推荐骨骼命名：

```
root
├── body
│   ├── head
│   └── tail
└── leg_front_left / leg_front_right
    leg_back_left  / leg_back_right
```

## 动画

- 动画片段名建议使用小写：`idle`、`walk`、`run`（AI 状态映射：Idle→idle、Wander→walk、Flee→run）。
- 支持 `translation` / `rotation` / `scale` 通道与 `LINEAR` / `STEP` / `CUBICSPLINE` 插值。
- 片段时长取所有通道最后一个关键帧时间的最大值；循环片段首尾姿态应一致。
- 每只生物拥有独立姿态；切换片段时会在 0.2 秒内从当前姿态淡入，避免跳变。
- 没有动画的模型正常显示为静态姿态。

## 加载失败与回退

| 情况 | 行为 |
| --- | --- |
| GLB 损坏、JSON 非法、版本不符 | 记录错误，使用旧盒状占位几何 |
| 缺少 `POSITION`、数据越界 | 记录错误，使用旧盒状占位几何 |
| 纹理解码失败 | 仅该材质回退到占位材质（白色 × baseColorFactor） |
| 动画通道引用非关节节点 | 跳过该通道并输出警告 |
| 未知插值方式 | 返回可读错误，不使用错误动画数据 |

所有错误都通过 `ModelLoadError` 转成带上下文的文本，启动日志汇总模型数量与失败项。
