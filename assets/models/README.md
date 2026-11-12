# 模型资源规范（v0.22）

本目录存放生物、玩家角色、装备与静态物体的 `.glb` 模型。游戏启动时递归扫描
`assets/models/` 并注册到 `ModelRegistry`；注册失败只记录错误，对应对象继续使用
内置回退几何，不会中断启动。

模型根目录默认是 `assets/models`，随 `NEVERLAND_ASSET_ROOT` 环境变量一起变化，
子目录深度上限为 4 层。

## 目录结构

```
assets/
├── models/
│   ├── pig.glb                    猪（可选，缺失时使用内置程序化模型）
│   ├── cow.glb                    牛（可选）
│   ├── player/
│   │   ├── player.glb             玩家角色（可选，需满足下方骨骼契约）
│   │   └── player_fp_arms.glb     第一人称手臂（可选）
│   └── equipment/
│       ├── tools/wooden_pickaxe.glb
│       ├── weapons/iron_sword.glb
│       └── armor/iron_helmet.glb
└── textures/
    └── entities/                  模型外部 PNG 纹理
```

- 同一模型被任意数量的对象共享，不会按对象重复加载。
- 文件名（不含扩展名）即模型注册名，与所在子目录无关；实体按 `pig` / `cow`、
  玩家按 `player`、装备按物品名（如 `iron_sword`）查找。
- 磁盘上的 GLB 优先；同名内置模型只在注册表中不存在该名称时注册。
- 装备模型缺失只会跳过该部件并告警一次，玩家本体仍正常显示。

## 坐标、单位与原点

| 项目 | 约定 |
| --- | --- |
| 坐标系 | 右手系，Y 轴向上 |
| 前方 | 模型本地 +Z |
| 单位 | 1 单位 = 1 米 = 1 方块 |
| 原点 | 双脚中心，落在 `y = 0` 平面 |
| 水平居中 | 模型在 X/Z 轴以原点为中心 |

导入时若包围盒最低点明显偏离 `y = 0`，引擎会自动做一次垂直贴地补偿，并在日志中输出警告。
补偿只校正高度、不做水平居中（生物头部前伸会让包围盒在 Z 轴天然不对称）。
补偿只是兜底，建议仍在建模软件里把原点放到脚底，避免动画位移与碰撞盒对不上。

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

## 内置猪/牛贴图覆盖

没有 GLB 时也可以直接替换内置模型的贴图：

| 项目 | 约定 |
| --- | --- |
| 路径 | `assets/textures/entities/pig.png`、`assets/textures/entities/cow.png` |
| 格式 | PNG，RGBA8，建议 64×64；其他尺寸按归一化 UV 缩放 |
| 生效时机 | 启动时读取；同名 GLB 存在时 GLB 优先，覆盖图不生效 |
| 热重载 | 不支持，需重启游戏 |
| 缺失/损坏 | 静默或告警后继续使用内置生成图集 |

图集以左上角为 (0, 0)，分区如下：

| UV 范围 | 用途 |
| --- | --- |
| (0, 0)–(0.5, 0.5) | 身体正/背面 |
| (0.5, 0)–(1, 0.5) | 身体侧面强调色；牛为斑点区 |
| (0, 0.5)–(0.5, 1) | 四条腿 |
| (0.5, 0.5)–(0.75, 1) | 头部 |
| (0.75, 0.5)–(1, 1) | 脸部（眼睛/嘴） |

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

## 玩家骨骼契约

`player.glb` 必须包含以下 20 根骨骼（名称大小写不敏感）；缺失的骨骼只会导致
依赖它的装备挂载被跳过，不会让角色消失：

```
Root
└── Pelvis
    ├── Spine → Chest
    │   ├── Neck → Head
    │   ├── LeftShoulder → LeftArm → LeftForearm → LeftHand
    │   └── RightShoulder → RightArm → RightForearm → RightHand
    ├── LeftLeg → LeftKnee → LeftFoot
    └── RightLeg → RightKnee → RightFoot
```

| 挂载点 | 骨骼 | 用途 |
| --- | --- | --- |
| RightHand | RightHand | 工具、武器、食物 |
| LeftHand | LeftHand | 预留左手物品 |
| Head | Head | 头盔 |
| Chest | Chest | 胸甲 |
| Legs | Pelvis | 护腿髋部 |
| LeftLeg / RightLeg | LeftLeg / RightLeg | 左右护腿 |
| LeftFoot / RightFoot | LeftFoot / RightFoot | 左右靴子 |

装备模型使用**骨骼局部坐标**：模型原点放在骨骼原点即可，缩放与偏移由视觉注册表
给出，外部 GLB 不需要为不同身高重新对齐。工具默认握把位于原点、朝向本地 `+Y`。
装备必须是静态网格（不含 `JOINTS_0`/`WEIGHTS_0`），由骨骼变换整体驱动。

| 类别 | 注册名示例 |
| --- | --- |
| 工具 | `wooden_pickaxe`、`stone_axe`、`iron_shovel` |
| 武器 | `wooden_sword`、`stone_sword`、`iron_sword`、`diamond_sword` |
| 食物 | `apple`、`bread`、`raw_beef`、`cooked_beef` |
| 头盔 | `leather_helmet`、`iron_helmet`、`diamond_helmet` |
| 胸甲 | `leather_chestplate`、`iron_chestplate`、`diamond_chestplate` |
| 护腿 | `<材质>_leggings_hip`、`<材质>_leggings_left`、`<材质>_leggings_right` |
| 靴子 | `<材质>_boot_left`、`<材质>_boot_right` |

材质取 `leather` / `iron` / `diamond`。没有对应 GLB 时使用内置程序化外形。

玩家模型片段名与优先级：`death` > `hurt` > `attack` > `land` > `jump`/`fall` >
`mining` > `eat` > `run` > `walk` > `idle`。缺少片段时先回退 `idle`，再回退第一个
片段。内置模型已包含全部片段，外部模型建议对齐相同命名。

第一人称使用独立的 `player_fp_arms` 静态模型，本地 `-Z` 为前方；缺失时只隐藏手臂，
不影响第三人称角色。

## 动画

- 动画片段名建议使用小写：`idle`、`walk`、`run`（AI 状态映射：Idle→idle、Wander→walk、Flee→run）。
- 命名不一致时会按别名回退：`idle` ← `stand`/`stop`/`eat`，`walk` ← `move`/`run`，`run` ← `sprint`/`walk`；
  仍找不到则使用资源中的第一个片段，并在日志中提示一次可用片段列表。
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
