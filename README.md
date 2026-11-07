# Neverland Rhapsody

> A Rust-based Voxel Sandbox Engine

用 Rust 从零实现的 Minecraft 风格体素沙盒引擎，当前处于可交互 Demo 阶段。

**自研引擎核心逻辑，不自研 GPU 驱动与图形 API** —— 底层通过 `wgpu` 调用 GPU，把精力集中在引擎本身的架构与算法上。

## 当前状态

`v0.7` 已可运行：`cargo run` 打开窗口，可第一人称自由观察噪声生成的山地地形（96×96 方块、最高 32 格，草/泥土/石头分层）；准星选中视线前方 8 格内最近的方块并显示深色线框，左键破坏、右键在命中面外侧放置草方块，改动后即时重建受影响区块。

- 当前里程碑：`v0.7 方块交互`（已完成）
- 本轮进度与下一步计划见 [`progress.md`](progress.md)
- 各模块代码位置见分层 [`codemap.md`](codemap.md)

## 第一版目标

一个可运行、可交互的体素沙盒 Demo：

- 第一人称自由移动的 3D 世界
- Minecraft 风格的方块地形
- 挖掘与放置方块
- 区块加载与基础地形生成
- 基础方块材质与光照
- 世界保存与读取

## 技术栈

| 分层 | 技术 | 来源 |
| --- | --- | --- |
| 语言 | Rust | 自研 |
| 图形接口 | wgpu | 第三方 |
| 窗口与输入 | winit | 第三方 |
| 数学 | glam | 第三方 |
| GPU 数据转换 | bytemuck | 第三方 |
| 异步初始化 | pollster | 第三方 |
| 纹理读取 | image | 第三方 |
| 地形噪声 | noise | 第三方 |
| 序列化 | serde | 第三方 |
| 错误处理 | anyhow | 第三方 |
| 日志 | tracing | 第三方 |

### 自研模块

| 模块 | 职责 |
| --- | --- |
| World | 世界管理系统 |
| Block | 方块数据系统 |
| Chunk | 区块管理系统 |
| Mesher | 体素网格生成算法 |
| Renderer | 自定义渲染器 |
| Camera | 第一人称摄像机 |
| Physics | 基础碰撞与重力 |
| Raycast | 方块射线检测 |
| Terrain | 程序化地形生成 |

## 目录结构

```
neverland-rhapsody/
├── Cargo.toml
├── README.md
├── progress.md              # 跨会话进度状态
├── codemap.md               # 代码地图入口
├── assets/
│   ├── textures/blocks/     # 方块纹理
│   └── shaders/             # WGSL 着色器
├── src/
│   ├── main.rs
│   ├── core/                # 主循环、时间、配置
│   ├── renderer/            # GPU、着色器、渲染管线
│   ├── world/               # 方块、区块、世界数据
│   ├── mesh/                # 表面提取与网格优化
│   ├── player/              # 移动、碰撞、物理
│   ├── interaction/         # 射线检测、破坏与放置
│   └── save/                # 世界存档
└── tests/                   # 集成测试
```

模块职责与文件级索引随开发逐层登记在各级 `codemap.md`，本文件不重复维护明细。

## 快速开始

前置要求：Rust stable 工具链（`cargo`、`rustc`）。

```bash
git clone <repo-url>
cd neverland-rhapsody
cargo run
```

> `cargo run` 会打开一个 1280×720 的窗口。WASD 前后左右移动，空格上升、Shift 下降；左键点击窗口捕获鼠标以转动视角，捕获后左键破坏选中方块、右键在命中面外侧放置方块，按 Esc 释放，再次按 Esc 退出。
> Windows + MinGW 环境若为 mcfgthread 变体，链接参数已在 `.cargo/config.toml` 中补全。

构建与质量检查：

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## 里程碑

| 版本 | 开发内容 | 验收结果 | 状态 |
| --- | --- | --- | --- |
| 0.1 | 窗口与程序入口 | 能启动游戏 | 已完成 |
| 0.2 | wgpu 渲染器 | 能显示 3D 场景 | 已完成 |
| 0.3 | 摄像机与控制 | 能自由观察场景 | 已完成 |
| 0.4 | Block + Chunk | 能渲染方块地面 | 已完成 |
| 0.5 | 地形生成 | 能生成山地 | 已完成 |
| 0.6 | 射线检测 | 能选中方块 | 已完成 |
| 0.7 | 方块交互 | 能挖掘、放置 | 已完成 |
| 0.8 | 世界存档 | 能保存和读取 | 未开始 |

一次只推进一个里程碑，验收通过后再进入下一项。

## 开发约定

- **模块化优先**：即使只是 Demo，也不把逻辑塞进 `main.rs`。
- **自研边界**：GPU 驱动与图形 API 不自研；引擎核心逻辑不调用现成游戏引擎。
- **定位与进度分离**：`codemap.md` 记录“代码在哪”，`progress.md` 记录“做到哪、下一步做什么”，两者不混写。
- **完成定义**：构建通过、静态检查无警告、测试通过、codemap 与 progress 同步，才算一项任务完成。
- **变更历史交给 Git**：文档中不保留“已废弃”“旧路径”类历史条目。

## 后续扩展

架构为多人联机与模组系统预留了扩展空间，第一版不实现。

## License

待定。
