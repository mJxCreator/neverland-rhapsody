//! 应用主体：窗口创建与主循环。

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};
use tracing::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use super::config::AppConfig;
use super::time::FrameTimer;
use crate::combat::{
    damage::DamageKind, hit_detection, knockback, AttackController, CombatConfig, WeaponStats,
};
use crate::entity::{
    entity_collision, entity_registry, entity_render, AiConfig, DamageResult, EntityId,
    EntityManager, EntityModelConfig, EntityModelSet, EntityPhysicsConfig, EntityPhysicsSystem,
    EntityRenderConfig, SpawnConfig, Spawner,
};
use crate::equipment::Equipment;
use crate::equipment_visual;
use crate::interaction::{
    drops_for_harvest, placement_cell, primary_use, raycast, secondary_use, HarvestTool, Hotbar,
    ItemUse, MiningOutcome, MiningState, MiningTarget, RayHit,
};
use crate::item::{
    collect_pickups, crafting, recipe_registry, tool, DroppedItemConfig, DroppedItemStore,
    Inventory, ItemId, ItemStack, RecipeCategory, RecipeId, Wear, HOTBAR_OFFSET,
};
use crate::model::{builtin, ModelDraw, ModelRegistry};
use crate::player::{
    damage, find_respawn_position, jump_speed_for_height, Damage, DamageOutcome, DamageSource,
    PhysicsConfig, PhysicsSystem, PhysicsWorld, Player, PlayerController, Survival, FIXED_TIMESTEP,
};
use crate::player_visual::third_person::ThirdPersonConfig;
use crate::player_visual::{
    player_model, third_person, CameraMode, PlayerVisual, PlayerVisualState,
};
use crate::renderer::{spawn_pose, RenderOutcome, Renderer};
use crate::save::{
    BlockEdit, CharacterSave, SaveEntry, SaveLibrary, WorldSave, CHARACTER_SAVE_VERSION,
};
use crate::survival::cooking::CookingRegistry;
use crate::survival::food::{EatingState, EatingUpdate};
use crate::survival::respawn;
use crate::survival::rng::GameRng;
use crate::survival::sleep;
use crate::survival::stamina::ATTACK_COST;
use crate::survival::{Vitals, VitalsInput, WorldTime};
use crate::texture::types::TextureImage;
use crate::texture::{TextureAssets, TextureWatcher, DEFAULT_ASSET_ROOT};
use crate::ui::menu::config::MAX_ROWS;
use crate::ui::{ClickButton, MenuAction, MenuEntry, MenuUi, UiIntent, UiManager, UiPointerAction, UiState};
use crate::world::{Block, ChunkConfig, ChunkManager, ChunkPos, World, DEFAULT_SEED};

/// 目标帧间隔，约 60 FPS。
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// 准星射线长度：超出该距离的方块不会被选中。
const SELECTION_REACH: f32 = 8.0;
/// 生物交互距离：准星在该距离内命中生物时可造成伤害。
const ENTITY_REACH: f32 = 4.0;
/// 存档写入失败后的重试间隔，避免每帧都重复写盘。
const SAVE_RETRY_INTERVAL: Duration = Duration::from_secs(2);
/// 单帧区块网格构建的时间预算，避免跨区块边界时一帧内集中重建多个网格。
const CHUNK_MESH_BUDGET: Duration = Duration::from_millis(2);
/// 纹理热重载轮询间隔；开发期修改 PNG 后约 1 秒内生效。
const TEXTURE_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// 纹理资源根目录环境变量；未设置时使用 `assets`。
const ASSET_ROOT_ENV: &str = "NEVERLAND_ASSET_ROOT";
/// 中文字体相对资源根的路径；缺失时回退内置 ASCII 字形。
const FONT_FILE: &str = "fonts/fusion-pixel-12px-monospaced-zh_hans.ttf";

/// 应用级界面：菜单与游戏世界二选一，暂停菜单覆盖在冻结的游戏画面上。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppScreen {
    /// 主菜单、角色选择或世界选择。
    Menu,
    /// 正在某个世界中游玩。
    InGame,
    /// 游戏内暂停菜单；世界与玩家时间冻结。
    Paused,
}

/// 持有窗口、渲染器与主循环状态。
pub struct App {
    config: AppConfig,
    /// 当前应用界面；决定输入交给菜单还是世界。
    screen: AppScreen,
    /// 菜单状态与选择结果展示。
    menu: MenuUi,
    /// 多角色/多世界存档库。
    library: SaveLibrary,
    /// 正在游玩的角色；菜单中为 `None`。
    active_character: Option<SaveEntry>,
    /// 正在游玩的世界；菜单中为 `None`。
    active_world: Option<SaveEntry>,
    world: World,
    save: WorldSave,
    chunk_manager: ChunkManager,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    controller: PlayerController,
    player: Player,
    physics: PhysicsSystem,
    /// 持续挖掘状态：绑定当前目标方块与累计时间。
    mining: MiningState,
    /// 左键是否仍按住。
    mining_held: bool,
    /// 快捷栏仅保存选中下标，物品内容来自背包。
    hotbar: Hotbar,
    /// 玩家背包：27 格主背包 + 9 格快捷栏。
    inventory: Inventory,
    /// 统一 UI 状态机：背包、合成、死亡互斥，并集中鼠标/键盘路由。
    ui: UiManager,
    /// 生命值、死亡状态与无敌帧。
    survival: Survival,
    /// 饥饿、体力、自然恢复与床复活点。
    vitals: Vitals,
    /// 世界昼夜时间。
    world_time: WorldTime,
    /// 全部营火的烹饪状态。
    cooking: CookingRegistry,
    /// 当前进食状态机。
    eating: EatingState,
    /// 生存系统使用的确定性随机数。
    game_rng: GameRng,
    /// 窗口失焦时暂停世界时间与生存消耗。
    paused: bool,
    /// 下一次周期性写入生存状态与营火的时刻。
    next_survival_save: Instant,
    /// 四个护甲槽。
    equipment: Equipment,
    /// 玩家攻击状态机：前摇、判定窗口与冷却。
    combat: AttackController,
    /// 初始出生点；用于重生时的安全落点搜索。
    spawn_anchor: Vec3,
    /// 世界掉落物。
    dropped: DroppedItemStore,
    /// 掉落物行为参数。
    drop_config: DroppedItemConfig,
    /// 生物实体管理器。
    entities: EntityManager,
    /// 实体固定步长物理。
    entity_physics: EntityPhysicsSystem,
    /// 生物自然生成器。
    spawner: Spawner,
    /// 生物 AI 参数。
    ai_config: AiConfig,
    /// 生物渲染参数。
    entity_render_config: EntityRenderConfig,
    /// 模型资源注册表：磁盘 GLB 优先，缺失时使用内置模型。
    models: ModelRegistry,
    /// 生物与模型实例的桥接状态。
    entity_models: EntityModelSet,
    /// 玩家角色视觉：模型、动画、装备挂载与第一/第三人称表现。
    player_visual: PlayerVisual,
    /// 当前相机视角；仅影响相机与绘制，不写入存档。
    camera_mode: CameraMode,
    /// 上一帧实际执行的物理步数，用于调试日志。
    physics_steps: u32,
    cursor_grabbed: bool,
    timer: FrameTimer,
    next_frame: Instant,
    /// 上一帧区块生成耗时（毫秒），用于调试日志。
    generation_ms: f64,
    /// 上一帧网格构建与上传耗时（毫秒）。
    mesh_ms: f64,
    /// 下一次允许重试写入存档的时刻。
    next_save_retry: Instant,
    /// 当前纹理资源；HUD 与后续背包 UI 可通过它查询物品图标。
    textures: Arc<TextureAssets>,
    /// 纹理资源根目录，热重载时重新装配使用。
    asset_root: String,
    /// 开发期纹理文件监听器。
    texture_watcher: TextureWatcher,
    /// 下一次纹理轮询时刻。
    next_texture_poll: Instant,
}

/// 尝试读取 `assets/textures/entities/<name>.png` 覆盖内置模型图集。
///
/// 文件缺失时静默使用内置生成纹理；读取或解码失败只告警，不影响启动。
fn load_entity_texture(root: &str, name: &str) -> Option<TextureImage> {
    let path = Path::new(root)
        .join("textures")
        .join("entities")
        .join(format!("{name}.png"));
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            debug!(path = %path.display(), "未提供实体贴图，使用内置生成图集");
            return None;
        }
        Err(error) => {
            warn!(path = %path.display(), error = %error, "实体贴图读取失败，使用内置生成图集");
            return None;
        }
    };
    match crate::texture::texture_loader::decode_png_bytes(&bytes, &path.display().to_string()) {
        Ok(image) => {
            if image.width != 64 || image.height != 64 {
                warn!(
                    path = %path.display(),
                    width = image.width,
                    height = image.height,
                    "实体贴图建议为 64×64，其他尺寸会按归一化 UV 缩放"
                );
            }
            info!(
                path = %path.display(),
                width = image.width,
                height = image.height,
                "实体贴图覆盖已启用"
            );
            Some(image)
        }
        Err(error) => {
            warn!(
                path = %path.display(),
                error = %error,
                "实体贴图解码失败，使用内置生成图集"
            );
            None
        }
    }
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        // 启动先进入主菜单：世界在玩家选定角色与世界后才创建与加载。
        let default_world = World::default();
        let save = WorldSave::empty(default_world.seed());
        let world = World::with_generation(default_world.seed(), save.worldgen_version());
        // 物理参数集中管理，玩家碰撞体尺寸也由同一处提供。
        let physics = PhysicsSystem::new(PhysicsConfig::default());
        let (spawn, yaw, pitch) = spawn_pose(&world);
        let mut player = Player::new(spawn, physics.config().dimensions);
        player.rotate_look(yaw, pitch);

        // 菜单阶段使用新角色的默认状态；进入世界后再从角色快照恢复。
        let (inventory, selected_hotbar) = (Self::starter_inventory(), 0);
        let survival = crate::player::Survival::default();
        let equipment = Equipment::default();
        let vitals = Vitals::default();
        let world_time = WorldTime::default();
        let cooking = CookingRegistry::new();
        let spawn_seed = world.seed();
        // 旧版单档在首次启动时迁移为 WORLD 1 + CHARACTER 1。
        let library = SaveLibrary::new();
        if let Err(err) = library.migrate_legacy() {
            warn!(error = %format!("{err:#}"), "旧存档迁移失败，保留原文件");
        }
        let asset_root =
            std::env::var(ASSET_ROOT_ENV).unwrap_or_else(|_| DEFAULT_ASSET_ROOT.to_string());
        // 字体在启动时加载一次；失败时绘制层自动回退到内置 ASCII 字形。
        let font_path = Path::new(&asset_root).join(FONT_FILE);
        match std::fs::read(&font_path) {
            Ok(bytes) => match crate::ui::core::font::load_font(bytes) {
                Ok(()) => info!(path = %font_path.display(), "字体已加载"),
                Err(error) => crate::ui::core::font::warn_fallback(&error),
            },
            Err(error) => {
                warn!(path = %font_path.display(), error = %error, "字体文件读取失败，回退内置字形");
            }
        }
        let textures = Arc::new(TextureAssets::load(&asset_root));
        let texture_summary = textures.summary();
        info!(
            block_textures = texture_summary.block_textures,
            item_textures = texture_summary.item_textures,
            ui_textures = texture_summary.ui_textures,
            placeholders = texture_summary.placeholders,
            warnings = texture_summary.warnings,
            errors = texture_summary.errors,
            root = %asset_root,
            "纹理资源已装配"
        );
        let texture_watcher = TextureWatcher::new(&asset_root);
        // 模型目录中的 GLB 优先；缺失或损坏时由内置猪/牛保证正式 3D 外观。
        let models_dir = Path::new(&asset_root).join("models");
        let mut models = ModelRegistry::load_from_dir(&models_dir);
        // 允许用同目录规范的 64×64 PNG 覆盖内置猪/牛图集；缺失时保留生成纹理。
        let mut pig = builtin::pig();
        let mut cow = builtin::cow();
        let mut zombie = builtin::zombie();
        if let Some(image) = load_entity_texture(&asset_root, "pig") {
            builtin::override_texture(&mut pig, image);
        }
        if let Some(image) = load_entity_texture(&asset_root, "cow") {
            builtin::override_texture(&mut cow, image);
        }
        if let Some(image) = load_entity_texture(&asset_root, "zombie") {
            builtin::override_texture(&mut zombie, image);
        }
        models.register_builtin("pig", pig);
        models.register_builtin("cow", cow);
        models.register_builtin("zombie", zombie);
        // 玩家与装备同样遵循“磁盘同名 GLB 优先”的注册规则。
        models.register_builtin(player_model::PLAYER_MODEL, player_model::player());
        models.register_builtin(
            player_model::FIRST_PERSON_ARMS_MODEL,
            player_model::first_person_arms(),
        );
        equipment_visual::builtin::register(&mut models);
        for error in models.errors() {
            warn!(error = %error, "模型文件加载失败");
        }
        info!(
            dir = %models_dir.display(),
            models = %models.summary(),
            "模型资源已装配"
        );
        let entity_models = EntityModelSet::new(&models, EntityModelConfig::default());
        let player_visual = PlayerVisual::new(&models);
        let mut ui = UiManager::new();
        ui.set_ui_textures(&textures.ui_textures);
        let mut menu = MenuUi::new();
        menu.set_ui_textures(&textures.ui_textures);
        menu.open_main();
        let mut hotbar = Hotbar::new();
        hotbar.set_selected(selected_hotbar);
        debug!(
            recipes = recipe_registry::enabled().count(),
            tools = recipe_registry::by_category(RecipeCategory::Tools).count(),
            materials = recipe_registry::by_category(RecipeCategory::Materials).count(),
            "配方注册表已加载"
        );

        Self {
            config,
            screen: AppScreen::Menu,
            menu,
            library,
            active_character: None,
            active_world: None,
            world,
            save,
            chunk_manager: ChunkManager::new(ChunkConfig::default()),
            window: None,
            renderer: None,
            controller: PlayerController::new(),
            player,
            physics,
            mining: MiningState::new(),
            mining_held: false,
            hotbar,
            inventory,
            ui,
            survival,
            vitals,
            world_time,
            cooking,
            eating: EatingState::new(),
            game_rng: GameRng::new(u64::from(spawn_seed) ^ 0x5EED_2020),
            paused: false,
            next_survival_save: Instant::now() + Duration::from_secs(15),
            equipment,
            combat: AttackController::new(CombatConfig::default()),
            spawn_anchor: spawn,
            dropped: DroppedItemStore::new(),
            drop_config: DroppedItemConfig::default(),
            entities: EntityManager::new(),
            entity_physics: EntityPhysicsSystem::new(EntityPhysicsConfig::default()),
            spawner: Spawner::new(SpawnConfig::default(), spawn_seed),
            ai_config: AiConfig::default(),
            entity_render_config: EntityRenderConfig::default(),
            models,
            entity_models,
            player_visual,
            camera_mode: CameraMode::default(),
            physics_steps: 0,
            cursor_grabbed: false,
            timer: FrameTimer::new(),
            next_frame: Instant::now(),
            generation_ms: 0.0,
            mesh_ms: 0.0,
            next_save_retry: Instant::now(),
            textures,
            asset_root,
            texture_watcher,
            next_texture_poll: Instant::now(),
        }
    }

    /// 新世界初始物品：铁锭没有冶炼来源，作为明确的测试材料发放；
    /// 木材与石头由玩家按“采集 → 木棍 → 木镐 → 石镐”的合成闭环获得。
    fn starter_inventory() -> Inventory {
        let mut inventory = Inventory::new();
        if let Some(stack) = ItemStack::new(ItemId::IRON_INGOT, 4) {
            inventory.set_slot(HOTBAR_OFFSET, Some(stack));
        }
        // 钻石暂无自然来源，先作为装备测试材料发放。
        if let Some(stack) = ItemStack::new(ItemId::DIAMOND, 6) {
            inventory.set_slot(HOTBAR_OFFSET + 1, Some(stack));
        }
        // 生存系统测试物资：营火、床与基础食物暂无完整生产链，先直接发放。
        if let Some(stack) = ItemStack::new(ItemId::CAMPFIRE, 2) {
            inventory.set_slot(HOTBAR_OFFSET + 2, Some(stack));
        }
        if let Some(stack) = ItemStack::new(ItemId::BED, 1) {
            inventory.set_slot(HOTBAR_OFFSET + 3, Some(stack));
        }
        if let Some(stack) = ItemStack::new(ItemId::APPLE, 3) {
            inventory.set_slot(HOTBAR_OFFSET + 4, Some(stack));
        }
        if let Some(stack) = ItemStack::new(ItemId::BREAD, 3) {
            inventory.set_slot(HOTBAR_OFFSET + 5, Some(stack));
        }
        inventory
    }

    /// 创建事件循环并进入主循环，窗口关闭或主菜单选择退出时返回。
    pub fn run(config: AppConfig) -> anyhow::Result<()> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Wait);

        let mut app = Self::new(config);
        event_loop.run_app(&mut app)?;
        info!("事件循环已退出");
        Ok(())
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title(self.config.window_title.clone())
            .with_inner_size(PhysicalSize::new(
                self.config.window_width,
                self.config.window_height,
            ));

        match event_loop.create_window(attributes) {
            Ok(window) => {
                info!(
                    width = self.config.window_width,
                    height = self.config.window_height,
                    "窗口已创建"
                );

                let window = Arc::new(window);
                // 启动时世界为空，渲染器先建好管线；选定世界后再上传区块。
                info!("初始区块待选择世界后加载");

                match Renderer::new(
                    window.clone(),
                    window.inner_size(),
                    &self.world,
                    self.player.eye_position(),
                    self.player.yaw(),
                    self.player.pitch(),
                    self.textures.clone(),
                ) {
                    Ok(mut renderer) => {
                        renderer.upload_models(&self.models);
                        info!("渲染器已初始化");
                        self.renderer = Some(renderer);
                    }
                    Err(err) => {
                        error!(error = %format!("{err:#}"), "渲染器初始化失败");
                        event_loop.exit();
                    }
                }

                self.window = Some(window);
                self.sync_ime();
                self.timer = FrameTimer::new();
                self.next_frame = Instant::now();
            }
            Err(err) => {
                error!(error = %err, "窗口创建失败");
                event_loop.exit();
            }
        }
    }

    /// 把存档库条目转换为菜单条目。
    fn menu_entries(entries: Vec<SaveEntry>) -> Vec<MenuEntry> {
        entries
            .into_iter()
            .map(|entry| MenuEntry::new(entry.id, entry.name))
            .collect()
    }

    /// 打开角色选择并刷新列表。
    fn open_character_select(&mut self) {
        let characters = Self::menu_entries(self.library.characters());
        self.menu.open_characters(characters);
    }

    /// 打开世界选择；角色必须来自当前角色列表。
    fn open_world_select(&mut self, character_id: &str) {
        let Some(character) = self
            .library
            .characters()
            .into_iter()
            .find(|entry| entry.id == character_id)
        else {
            self.menu
                .show_notice("未找到角色");
            return;
        };
        let worlds = Self::menu_entries(self.library.worlds());
        self.menu
            .open_worlds(MenuEntry::new(character.id, character.name), worlds);
    }

    /// 进入指定世界的指定角色；角色快照与世界存档分别加载。
    fn start_session(&mut self, character_id: &str, world_id: &str) {
        let Some(seed) = self.library.world_seed(world_id) else {
            warn!(world = world_id, "世界缺少种子，无法进入");
            self.menu.show_notice("世界数据缺失");
            return;
        };
        let Some(character_save) = self.library.character(character_id) else {
            warn!(character = character_id, "角色存档缺失，无法进入");
            self.menu.show_notice("角色数据缺失");
            return;
        };

        let save = WorldSave::load(self.library.world_path(world_id), seed);
        let world = World::with_generation(seed, save.worldgen_version());
        let (inventory, selected_hotbar) = character_save
            .inventory
            .as_ref()
            .map(Inventory::from_saved)
            .unwrap_or_else(|| (Self::starter_inventory(), 0));
        let survival = character_save
            .survival
            .map(Survival::from_saved)
            .unwrap_or_default();
        let equipment = character_save
            .equipment
            .as_ref()
            .map(Equipment::from_saved)
            .unwrap_or_default();
        let vitals = character_save
            .vitals
            .map(Vitals::from_saved)
            .unwrap_or_default();
        let world_time = WorldTime::from_saved(save.world_time());
        let mut cooking = CookingRegistry::new();
        cooking.restore(save.campfires());

        let physics = PhysicsSystem::new(PhysicsConfig::default());
        let (spawn, yaw, pitch) = spawn_pose(&world);
        let mut player = Player::new(spawn, physics.config().dimensions);
        player.rotate_look(yaw, pitch);
        let mut hotbar = Hotbar::new();
        hotbar.set_selected(selected_hotbar);
        let mut ui = UiManager::new();
        ui.set_ui_textures(&self.textures.ui_textures);

        // 整体替换会话状态，确保上一个世界的实体、掉落物与界面不会残留。
        self.save = save;
        self.world = world;
        self.chunk_manager = ChunkManager::new(ChunkConfig::default());
        self.inventory = inventory;
        self.hotbar = hotbar;
        self.survival = survival;
        self.vitals = vitals;
        self.world_time = world_time;
        self.cooking = cooking;
        self.equipment = equipment;
        self.player = player;
        self.physics = physics;
        self.mining = MiningState::new();
        self.mining_held = false;
        self.eating = EatingState::new();
        self.combat = AttackController::new(CombatConfig::default());
        self.spawn_anchor = spawn;
        self.dropped = DroppedItemStore::new();
        self.entities = EntityManager::new();
        self.entity_physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        self.spawner = Spawner::new(SpawnConfig::default(), seed);
        self.ai_config = AiConfig::default();
        self.entity_models = EntityModelSet::new(&self.models, EntityModelConfig::default());
        self.player_visual = PlayerVisual::new(&self.models);
        self.game_rng = GameRng::new(u64::from(seed) ^ 0x5EED_2020);
        self.ui = ui;
        self.physics_steps = 0;
        self.generation_ms = 0.0;
        self.mesh_ms = 0.0;
        self.next_survival_save = Instant::now() + Duration::from_secs(15);
        self.next_save_retry = Instant::now();
        self.paused = false;
        self.active_character = Some(SaveEntry {
            id: character_id.to_owned(),
            name: character_save.name.clone(),
            seed: None,
        });
        let world_name = self
            .library
            .worlds()
            .into_iter()
            .find(|entry| entry.id == world_id)
            .map(|entry| entry.name)
            .unwrap_or_else(|| world_id.to_owned());
        self.active_world = Some(SaveEntry {
            id: world_id.to_owned(),
            name: world_name,
            seed: Some(seed),
        });

        self.load_session_chunks();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.load_world(&self.world);
            renderer.set_model_draws(Vec::new());
            renderer.set_overlay_model_draws(Vec::new());
            renderer.set_camera_pose(
                self.player.eye_position(),
                self.player.yaw(),
                self.player.pitch(),
            );
        }
        self.ui.force_close(&mut self.inventory);
        self.screen = AppScreen::InGame;
        self.grab_cursor();
        info!(
            character = character_id,
            world = world_id,
            seed,
            chunks = self.world.chunk_count(),
            "已进入世界"
        );
    }

    /// 同步加载玩家周围区块，并恢复所属区块的掉落物与生物。
    fn load_session_chunks(&mut self) {
        let spawn = self.player.position();
        let player_chunk =
            ChunkPos::from_world(spawn.x.floor() as i32, 0, spawn.z.floor() as i32);
        let loaded = self
            .chunk_manager
            .preload(player_chunk, &mut self.world, &mut |pos, world| {
                self.save.apply_chunk(pos, world)
            });
        let loaded_chunks: Vec<ChunkPos> = self.world.chunk_positions().collect();
        for pos in &loaded_chunks {
            let records = self.save.chunk_drops(*pos).to_vec();
            self.dropped.restore_chunk(*pos, &records);
            let entities = self.save.chunk_entities(*pos).to_vec();
            self.entities.restore_chunk(*pos, &entities);
        }
        let stats = self.world.generation_stats();
        info!(
            chunks = self.world.chunk_count(),
            loaded,
            generation_version = self.world.generation_version(),
            avg_generation_ms = stats.average_ms(),
            terrain_ms = stats.stage_average_ms(stats.terrain_ns),
            cave_ms = stats.stage_average_ms(stats.cave_ns),
            ore_ms = stats.stage_average_ms(stats.ore_ns),
            decoration_ms = stats.stage_average_ms(stats.decoration_ns),
            biome = self
                .world
                .biome_at(spawn.x.floor() as i32, spawn.z.floor() as i32)
                .name(),
            "世界初始区块已加载"
        );
    }

    /// 打开暂停菜单：世界与玩家时间冻结，鼠标释放用于点击按钮。
    fn open_pause(&mut self) {
        if self.screen != AppScreen::InGame {
            return;
        }
        self.release_cursor();
        self.eating.cancel();
        self.menu.open_pause();
        self.screen = AppScreen::Paused;
        info!("暂停菜单已打开");
    }

    /// 关闭暂停菜单并恢复游戏。
    fn resume_game(&mut self) {
        if self.screen != AppScreen::Paused || !self.menu.close_pause() {
            return;
        }
        self.screen = AppScreen::InGame;
        self.grab_cursor();
        info!("暂停菜单已关闭");
    }

    /// 保存并返回开始界面；角色快照与世界存档都已写入。
    fn leave_to_main_menu(&mut self) {
        self.flush_save();
        self.ui.force_close(&mut self.inventory);
        self.release_cursor();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_world();
            renderer.set_model_draws(Vec::new());
            renderer.set_overlay_model_draws(Vec::new());
        }
        self.world = World::with_generation(DEFAULT_SEED, self.world.generation_version());
        self.save = WorldSave::empty(DEFAULT_SEED);
        self.chunk_manager = ChunkManager::new(ChunkConfig::default());
        self.dropped = DroppedItemStore::new();
        self.entities = EntityManager::new();
        self.active_character = None;
        self.active_world = None;
        self.screen = AppScreen::Menu;
        self.menu.open_main();
        info!("已保存并返回开始界面");
    }

    /// 执行菜单动作；所有磁盘操作都在这里落地。
    fn handle_menu_action(&mut self, action: MenuAction, event_loop: &ActiveEventLoop) {
        match action {
            MenuAction::None => {}
            MenuAction::SinglePlayer => self.open_character_select(),
            MenuAction::Quit => {
                info!("主菜单选择退出游戏");
                event_loop.exit();
            }
            MenuAction::Unavailable(text) => self.menu.show_notice(text),
            MenuAction::NewCharacter => {
                if self.library.characters().len() >= MAX_ROWS {
                    self.menu.show_notice("角色数量已达上限");
                    return;
                }
                if let Err(err) = self.library.create_character() {
                    warn!(error = %format!("{err:#}"), "新建角色失败");
                    self.menu.show_notice("新建角色失败");
                    return;
                }
                self.open_character_select();
                self.menu.show_notice("角色已创建");
            }
            MenuAction::SelectCharacter(id) => self.open_world_select(&id),
            MenuAction::EditCharacter(id) => {
                let name = self
                    .library
                    .character(&id)
                    .map(|save| save.name)
                    .unwrap_or_default();
                self.menu.begin_rename(id, &name);
            }
            MenuAction::DeleteCharacter(id) => {
                if let Err(err) = self.library.delete_character(&id) {
                    warn!(error = %format!("{err:#}"), "删除角色失败");
                    self.menu.show_notice("删除角色失败");
                    return;
                }
                self.open_character_select();
                self.menu.show_notice("角色已删除");
            }
            MenuAction::ConfirmRename => {
                let id = self.menu.rename_character_id().map(str::to_owned);
                let text = self.menu.rename_text().map(str::to_owned).unwrap_or_default();
                let Some(id) = id else {
                    self.menu.cancel_rename();
                    return;
                };
                match self.library.rename_character(&id, &text) {
                    Ok(_) => {
                        self.menu.cancel_rename();
                        self.open_character_select();
                        self.menu.show_notice("角色已重命名");
                    }
                    Err(err) => {
                        warn!(error = %format!("{err:#}"), "重命名角色失败");
                        self.menu.show_notice("名称不合法");
                    }
                }
            }
            MenuAction::CancelRename => self.menu.cancel_rename(),
            MenuAction::NewWorld => {
                if self.library.worlds().len() >= MAX_ROWS {
                    self.menu.show_notice("世界数量已达上限");
                    return;
                }
                if let Err(err) = self.library.create_world() {
                    warn!(error = %format!("{err:#}"), "新建世界失败");
                    self.menu.show_notice("新建世界失败");
                    return;
                }
                self.refresh_world_list();
                self.menu.show_notice("世界已创建");
            }
            MenuAction::DeleteWorld(id) => {
                if let Err(err) = self.library.delete_world(&id) {
                    warn!(error = %format!("{err:#}"), "删除世界失败");
                    self.menu.show_notice("删除世界失败");
                    return;
                }
                self.refresh_world_list();
                self.menu.show_notice("世界已删除");
            }
            MenuAction::SelectWorld(id) => {
                let Some(character) = self.menu.selected_character().cloned() else {
                    self.menu.show_notice("请先选择角色");
                    return;
                };
                self.start_session(&character.id, &id);
            }
            MenuAction::BackToMain => self.menu.open_main(),
            MenuAction::BackToCharacters => self.open_character_select(),
            MenuAction::Resume => self.resume_game(),
            MenuAction::SaveAndQuit => self.leave_to_main_menu(),
        }
    }

    /// 原地刷新世界列表，保留当前角色标题。
    fn refresh_world_list(&mut self) {
        self.menu.set_worlds(Self::menu_entries(self.library.worlds()));
    }

    /// 把当前角色快照写入独立角色文件，并从世界存档得到当前生存状态。
    fn write_character_snapshot(&mut self) {
        let Some(character) = self.active_character.as_ref() else {
            return;
        };
        let save = CharacterSave {
            version: CHARACTER_SAVE_VERSION,
            name: character.name.clone(),
            inventory: Some(self.inventory.to_saved(self.hotbar.selected_index())),
            survival: Some(self.survival.to_saved()),
            equipment: Some(self.equipment.to_saved()),
            vitals: Some(self.vitals.to_saved()),
        };
        if let Err(err) = self.library.save_character(&character.id, &save) {
            warn!(error = %format!("{err:#}"), "角色快照写入失败");
        }
    }

    /// 捕获鼠标用于第一人称视角；优先锁定光标，失败时退回窗口内约束。
    fn grab_cursor(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let result = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        match result {
            Ok(()) => {
                window.set_cursor_visible(false);
                self.cursor_grabbed = true;
                info!("鼠标已捕获，按 Esc 释放");
            }
            Err(err) => warn!(error = %err, "鼠标捕获失败"),
        }
    }

    /// 重命名面板开关时同步系统输入法状态。
    fn sync_ime(&self) {
        if let Some(window) = self.window.as_ref() {
            let allowed =
                self.screen != AppScreen::InGame && self.menu.is_renaming();
            window.set_ime_allowed(allowed);
        }
    }

    /// 释放鼠标并清空输入状态，避免按键与视角残留。
    fn release_cursor(&mut self) {
        if let Some(window) = self.window.as_ref() {
            if let Err(err) = window.set_cursor_grab(CursorGrabMode::None) {
                warn!(error = %err, "释放鼠标捕获失败");
            }
            window.set_cursor_visible(true);
        }
        self.cursor_grabbed = false;
        self.mining_held = false;
        self.mining.cancel();
        self.controller.reset();
    }

    /// 打开背包界面：暂停视角与挖掘输入，但世界物理与区块流送继续运行。
    fn open_inventory(&mut self) {
        if self.renderer.is_none() || self.survival.is_dead() {
            return;
        }
        self.release_cursor();
        if !self.ui.open_inventory() {
            return;
        }
        info!("背包已打开，按 E 或 Esc 关闭");
    }

    /// 关闭背包界面：拖动中的物品先归位，再恢复鼠标捕获。
    fn close_inventory(&mut self) {
        if !self.ui.close_inventory(&mut self.inventory) {
            warn!("背包已满，无法放下拖动中的物品，界面保持打开");
            return;
        }
        self.persist_player();
        self.grab_cursor();
        info!("背包已关闭");
    }

    /// 打开合成界面：玩家输入暂停，但世界继续运行。
    fn open_crafting(&mut self) {
        if self.renderer.is_none() || self.survival.is_dead() {
            return;
        }
        self.release_cursor();
        if !self.ui.open_crafting() {
            return;
        }
        info!("合成界面已打开，按 C 或 Esc 关闭");
    }

    /// 关闭合成界面并恢复鼠标捕获；合成结果已在点击时写盘。
    fn close_crafting(&mut self) {
        self.ui.close_crafting();
        self.persist_player();
        self.grab_cursor();
        info!("合成界面已关闭");
    }

    /// 打开营火烹饪界面；烹饪过程本身持续运行，不受界面开关影响。
    fn open_cooking(&mut self, campfire: IVec3) {
        if self.renderer.is_none() || self.survival.is_dead() {
            return;
        }
        self.eating.cancel();
        self.release_cursor();
        if !self.ui.open_cooking(campfire) {
            return;
        }
        self.cooking.ensure(campfire);
        info!("烹饪界面已打开，按 E 或 Esc 关闭");
    }

    /// 关闭烹饪界面并把营火状态写入存档。
    fn close_cooking(&mut self) {
        if !self.ui.close_cooking() {
            return;
        }
        self.persist_survival();
        self.grab_cursor();
        info!("烹饪界面已关闭");
    }

    /// 右键方块：营火打开烹饪、床尝试睡眠，其余情况按食物或放置方块处理。
    fn secondary_use(&mut self) {
        if self.survival.is_dead() || self.ui.blocks_world_input() {
            return;
        }
        if let Some(hit) = self.targeted_block() {
            match self.world.block_at(hit.block.x, hit.block.y, hit.block.z) {
                Block::Campfire => {
                    self.open_cooking(hit.block);
                    return;
                }
                Block::Bed => {
                    self.interact_bed(hit.block);
                    return;
                }
                _ => {}
            }
        }
        let selected = self.hotbar.selected_index();
        if let Some(stack) = self.inventory.hotbar_stack(selected) {
            if crate::survival::food_registry::definition(stack.item()).is_some() {
                if self.vitals.hunger().is_full() {
                    self.show_message("还不饿");
                    return;
                }
                if self.eating.start(stack.item(), HOTBAR_OFFSET + selected) {
                    return;
                }
            }
        }
        self.place_block();
    }

    /// 尝试睡眠；成功时推进到清晨、恢复体力并记录床坐标。
    fn interact_bed(&mut self, bed: IVec3) {
        let hostile_nearby = self.hostile_nearby(8.0);
        match sleep::try_sleep(
            &mut self.world_time,
            self.vitals.stamina_mut(),
            !self.survival.is_dead(),
            hostile_nearby,
        ) {
            Ok(()) => {
                self.eating.cancel();
                self.vitals.set_respawn(bed);
                self.persist_survival();
                self.show_message("睡到了早晨");
                info!(x = bed.x, y = bed.y, z = bed.z, "玩家已入睡并设置床复活点");
            }
            Err(error) => {
                self.show_message(error.message());
                info!(reason = error.message(), "无法睡眠");
            }
        }
    }

    /// 玩家附近是否仍有存活的敌对生物。
    fn hostile_nearby(&self, radius: f32) -> bool {
        let center = self.player.position();
        self.entities.creatures().iter().any(|creature| {
            creature.entity.health() > 0
                && EntityManager::is_hostile(creature.kind())
                && creature.entity.position().distance(center) <= radius
        })
    }

    /// 在 HUD 上显示一条短状态提示。
    fn show_message(&mut self, text: &'static str) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.hud_mut().show_message(text);
        }
    }

    /// 执行一次合成；失败原因回显到界面，不修改背包。
    fn craft_recipe(&mut self, recipe_id: RecipeId) {
        match crafting::craft_by_id(recipe_id, &mut self.inventory) {
            Ok(output) => {
                self.ui.crafting_mut().show_success(output);
                self.persist_player();
                info!(recipe = ?recipe_id, item = ?output.item(), "合成成功");
            }
            Err(error) => {
                self.ui.crafting_mut().show_failure(error);
                info!(recipe = ?recipe_id, error = ?error, "合成失败");
            }
        }
    }

    /// 执行键盘路由得到的界面意图；每个意图只对应一个界面动作。
    fn handle_ui_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::OpenInventory => self.open_inventory(),
            UiIntent::CloseInventory => self.close_inventory(),
            UiIntent::OpenCrafting => self.open_crafting(),
            UiIntent::CloseCrafting => self.close_crafting(),
            UiIntent::CloseCooking => self.close_cooking(),
            UiIntent::OpenPause => self.open_pause(),
        }
    }

    /// 统一伤害结算：刷新 HUD 反馈，并在致死时进入死亡流程。
    fn apply_damage(&mut self, damage: Damage) -> DamageOutcome {
        let source = damage.source();
        self.eating.cancel();
        let outcome = damage::apply(&mut self.survival, damage);
        match outcome {
            DamageOutcome::Ignored => {}
            DamageOutcome::Applied { .. } => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.hud_mut().show_damage();
                }
                self.persist_player();
                info!(
                    source = source.label(),
                    health = self.survival.health(),
                    "玩家受到伤害"
                );
            }
            DamageOutcome::Killed => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.hud_mut().show_damage();
                }
                info!(source = source.label(), "玩家死亡");
                self.enter_death(source);
            }
        }
        outcome
    }

    /// 敌对生物近战命中玩家：护甲减伤、击退、耐久消耗与 HUD 反馈。
    fn apply_entity_attack(&mut self, attacker: EntityId, raw_damage: u32) {
        if self.survival.is_dead() {
            return;
        }
        self.eating.cancel();
        let Some(attacker_position) = self
            .entities
            .get(attacker)
            .map(|creature| creature.entity.position())
        else {
            return;
        };
        let defense = self.equipment.defense();
        let direction = self.damage_direction_angle(attacker_position);
        let outcome = damage::apply_mitigated(
            &mut self.survival,
            raw_damage,
            DamageSource::Entity,
            DamageKind::Melee,
            defense,
        );
        match outcome {
            DamageOutcome::Ignored => {}
            DamageOutcome::Applied { .. } => {
                if defense > 0 {
                    self.equipment.wear_all();
                }
                let impulse = knockback::player_impulse(attacker_position, self.player.position());
                self.player.apply_impulse(impulse);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.hud_mut().show_damage();
                    renderer.hud_mut().show_damage_direction(direction);
                }
                self.persist_player();
                info!(
                    attacker = attacker.raw(),
                    raw = raw_damage,
                    defense,
                    health = self.survival.health(),
                    "玩家受到敌对生物攻击"
                );
            }
            DamageOutcome::Killed => {
                if defense > 0 {
                    self.equipment.wear_all();
                }
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.hud_mut().show_damage();
                    renderer.hud_mut().show_damage_direction(direction);
                }
                self.persist_player();
                info!(attacker = attacker.raw(), "玩家被敌对生物击杀");
                self.enter_death(DamageSource::Entity);
            }
        }
    }

    /// 攻击来源相对镜头朝向的水平方向角；0 为正前方，正值在右。
    fn damage_direction_angle(&self, attacker_position: Vec3) -> f32 {
        let forward = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.camera().forward())
            .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
        let flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        if flat == Vec3::ZERO {
            return 0.0;
        }
        let right = Vec3::new(-flat.z, 0.0, flat.x);
        let delta = attacker_position - self.player.position();
        let to_attacker = Vec3::new(delta.x, 0.0, delta.z);
        to_attacker.dot(right).atan2(to_attacker.dot(flat))
    }

    /// 进入死亡状态：关闭其他界面、释放输入并显示死亡界面。
    fn enter_death(&mut self, source: DamageSource) {
        if self.ui.state() == UiState::Death {
            return;
        }
        self.release_cursor();
        self.ui.enter_death(&mut self.inventory, source);
        self.persist_player();
        info!(source = source.label(), "死亡界面已打开");
    }

    /// 重生：优先使用仍然有效的床复活点，否则回退到世界出生点并恢复满血。
    fn respawn(&mut self) {
        self.ensure_spawn_chunks();
        self.eating.cancel();
        let dimensions = self.physics.config().dimensions;
        let mut bed_position = None;
        if let Some(bed) = self.vitals.respawn() {
            self.ensure_chunks_around(Vec3::new(
                bed.x as f32 + 0.5,
                bed.y as f32,
                bed.z as f32 + 0.5,
            ));
            bed_position = respawn::bed_respawn_position(&self.world, bed, dimensions);
            if bed_position.is_none() {
                self.vitals.clear_respawn();
                info!("床复活点已失效，回退到世界出生点");
            }
        }
        let position = bed_position
            .unwrap_or_else(|| find_respawn_position(&self.world, self.spawn_anchor, dimensions));
        self.player.teleport(position);
        self.physics.reset();
        self.survival.respawn();
        self.combat.reset();
        self.player_visual.reset();
        self.ui.leave_death();
        self.controller.reset();
        self.persist_player();
        self.persist_survival();
        info!(x = position.x, y = position.y, z = position.z, "玩家已重生");
    }

    /// 重生前同步加载出生点周围区块，保证安全落点搜索看到真实地形。
    fn ensure_spawn_chunks(&mut self) {
        self.ensure_chunks_around(self.spawn_anchor);
    }

    /// 同步加载指定位置周围区块；返回时相关区块已可读取。
    ///
    /// 地形生成统一经 `ChunkManager`，网格进入脏集合后由常规流式预算在后续帧补建，
    /// 重生时不再一次性同步重建几十个区块网格。
    fn ensure_chunks_around(&mut self, anchor: Vec3) {
        let center = ChunkPos::from_world(anchor.x.floor() as i32, 0, anchor.z.floor() as i32);
        let loaded = self
            .chunk_manager
            .load_now(center, 1, &mut self.world, &mut |pos, world| {
                self.save.apply_chunk(pos, world)
            });

        // 新加载区块的掉落物与生物从存档恢复，与流式加载路径保持一致。
        for pos in &loaded {
            let records = self.save.chunk_drops(*pos).to_vec();
            self.dropped.restore_chunk(*pos, &records);
            let entities = self.save.chunk_entities(*pos).to_vec();
            self.entities.restore_chunk(*pos, &entities);
        }
    }

    /// 当前准星命中的生物；若方块比生物更近则不返回，避免隔墙攻击。
    ///
    /// 判定始终从玩家眼睛出发，与相机视角无关：第三人称仅改变观察位置，
    /// 不会扩大或缩短攻击距离。
    fn targeted_creature(&self) -> Option<EntityId> {
        let renderer = self.renderer.as_ref()?;
        let camera = renderer.camera();
        let origin = self.player.eye_position();
        let direction = camera.forward();
        let hit = entity_collision::raycast_entities(
            self.entities.creatures(),
            origin,
            direction,
            ENTITY_REACH,
        )?;
        if let Some(block_hit) = raycast(&self.world, origin, direction, ENTITY_REACH) {
            if let Some(block_distance) = entity_collision::ray_block_distance(
                origin,
                direction,
                block_hit.block,
                ENTITY_REACH,
            ) {
                if block_distance <= hit.1 {
                    return None;
                }
            }
        }
        Some(hit.0)
    }

    /// 当前手持物品的攻击参数；空手或损坏武器回退为空手，
    /// 空手伤害由饥饿派生的基础伤害决定。
    fn current_weapon(&self) -> WeaponStats {
        let index = HOTBAR_OFFSET + self.hotbar.selected_index();
        WeaponStats::for_stack(self.inventory.slot(index))
            .with_base_damage(self.vitals.hunger().base_damage())
    }

    /// 发起一次攻击；冷却中或玩家已死亡时忽略。
    fn start_attack(&mut self) {
        if self.survival.is_dead() || self.ui.state() != UiState::Gameplay {
            return;
        }
        self.eating.cancel();
        if self.combat.try_start(self.current_weapon()).is_ok() {
            debug!(
                state = self.combat.state().label(),
                damage = self.combat.stats().damage,
                "玩家发起攻击"
            );
        }
    }

    /// 推进攻击状态机；判定窗口打开时结算一次锥形命中。
    fn update_combat(&mut self, delta: f32) {
        if !self.combat.tick(delta) {
            return;
        }
        let stats = self.combat.stats();
        let Some(camera_forward) = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.camera().forward())
        else {
            return;
        };
        // 命中锥始终以玩家眼睛为原点，第三人称相机位置不参与攻击判定。
        let query = hit_detection::HitQuery::new(
            self.player.eye_position(),
            camera_forward,
            stats.range,
            stats.angle_deg,
        );
        let targets = {
            let world = &self.world;
            let is_occluded = |origin: Vec3, direction: Vec3, distance: f32| -> bool {
                let Some(hit) = raycast(world, origin, direction, distance) else {
                    return false;
                };
                entity_collision::ray_block_distance(origin, direction, hit.block, distance)
                    .is_some_and(|block_distance| block_distance < distance - 0.05)
            };
            hit_detection::find_targets(self.entities.creatures(), query, &is_occluded)
        };
        if targets.is_empty() {
            return;
        }
        let mut hit_count = 0;
        for id in targets {
            if self.hit_creature(id, stats.damage, stats.knockback) {
                hit_count += 1;
            }
        }
        if hit_count > 0 {
            self.wear_weapon(stats);
            // 攻击造成伤害后消耗体力；体力耗尽时由 Vitals 扣除饥饿。
            self.vitals.spend_stamina(ATTACK_COST);
            info!(
                targets = hit_count,
                damage = stats.damage,
                armed = stats.is_armed(),
                "近战攻击命中"
            );
        }
    }

    /// 对单个生物结算一次伤害与击退；返回是否实际命中。
    fn hit_creature(&mut self, id: EntityId, damage: u32, knockback_strength: f32) -> bool {
        let Some((kind, position, attacker)) = self.entities.get(id).map(|creature| {
            (
                creature.kind(),
                creature.entity.position(),
                self.player.eye_position(),
            )
        }) else {
            return false;
        };
        let result = self
            .entities
            .apply_damage(id, damage, attacker, self.ai_config.flee_seconds);
        if matches!(result, DamageResult::Ignored) {
            return false;
        }
        if let Some(creature) = self.entities.get_mut(id) {
            knockback::apply_to_entity(&mut creature.entity, attacker, knockback_strength);
        }
        if let Some(definition) = entity_registry::definition(kind) {
            let remaining = match result {
                DamageResult::Applied { remaining } => remaining,
                DamageResult::Killed | DamageResult::Ignored => 0,
            };
            if let Some(renderer) = self.renderer.as_mut() {
                renderer
                    .hud_mut()
                    .show_target(definition.name, remaining, definition.max_health);
            }
        }
        if let Some(renderer) = self.renderer.as_mut() {
            let color = entity_registry::definition(kind)
                .map(|definition| definition.accent_color)
                .unwrap_or(Vec3::new(0.7, 0.15, 0.15));
            renderer.spawn_hit_particles(position + Vec3::Y * 1.0, color);
        }
        match result {
            DamageResult::Killed => {
                info!(entity = id.raw(), ?kind, "生物已被击杀");
                self.spawn_creature_drops(kind, position, id);
            }
            DamageResult::Applied { remaining } => {
                debug!(entity = id.raw(), remaining, "生物受到伤害");
            }
            DamageResult::Ignored => {}
        }
        true
    }

    /// 命中后消耗 1 点武器耐久；损坏的武器留在快捷栏中。
    fn wear_weapon(&mut self, stats: WeaponStats) {
        if !stats.is_armed() {
            return;
        }
        let index = HOTBAR_OFFSET + self.hotbar.selected_index();
        let Some(stack) = self.inventory.slot(index) else {
            return;
        };
        let worn = stats.wear(stack);
        if worn.is_broken() && !stack.is_broken() {
            info!(item = ?worn.item(), "武器已损坏");
        }
        self.inventory.set_slot(index, Some(worn));
    }

    /// 按实体定义生成生物掉落物；数量由定义固定配置。
    fn spawn_creature_drops(
        &mut self,
        kind: crate::entity::EntityTypeId,
        position: Vec3,
        id: EntityId,
    ) {
        let Some(definition) = entity_registry::definition(kind) else {
            return;
        };
        for (index, drop) in definition.drops.iter().enumerate() {
            let count = drop.count_for(id.raw());
            if count == 0 {
                continue;
            }
            let Some(stack) = ItemStack::new(drop.item, count) else {
                continue;
            };
            let direction = if index % 2 == 0 { 1.0 } else { -1.0 };
            let velocity = Vec3::new(0.2 * direction, 1.3, -0.1 * direction);
            self.dropped
                .spawn(stack, position + Vec3::Y * 0.5, velocity, &self.drop_config);
        }
    }

    /// 当前准星命中的方块；与视角模式无关，始终以玩家眼睛为射线起点。
    fn targeted_block(&self) -> Option<RayHit> {
        let renderer = self.renderer.as_ref()?;
        let camera = renderer.camera();
        raycast(
            &self.world,
            self.player.eye_position(),
            camera.forward(),
            SELECTION_REACH,
        )
    }

    /// 当前命中方块对应的挖掘目标；空气不可挖掘，因此无需进入状态机。
    ///
    /// 工具速度倍率在这里根据快捷栏当前物品解析：徒手、普通物品、损坏工具
    /// 或类别不适用时都回退为徒手基准。
    fn mining_target(&self, hit: Option<RayHit>) -> Option<MiningTarget> {
        let hit = hit?;
        let block_type = self.world.block_at(hit.block.x, hit.block.y, hit.block.z);
        if block_type == Block::Air {
            return None;
        }
        let stack = self.inventory.hotbar_stack(self.hotbar.selected_index());
        let tool_speed = tool::mining_speed(stack, block_type);
        Some(MiningTarget {
            block: hit.block,
            block_type,
            tool_speed,
        })
    }

    /// 破坏指定方块：写世界、存存档、重建网格并生成碎块。
    ///
    /// `harvest_tool` 决定掉落是否满足采集条件；耐久消耗由调用方在挖掘完成时
    /// 统一结算，本函数不会重复扣减。
    fn break_block(&mut self, block: IVec3, block_type: Block, harvest_tool: HarvestTool) {
        let affected = self.world.set_block(block.x, block.y, block.z, Block::Air);
        if affected.is_empty() {
            return;
        }

        info!(block = ?block, block_type = ?block_type, "方块已破坏");
        self.remember_edit(BlockEdit {
            x: block.x,
            y: block.y,
            z: block.z,
            block: Block::Air,
        });
        if block_type == Block::Campfire {
            if let Some(state) = self.cooking.remove(block) {
                if self.ui.cooking_campfire() == Some(block) {
                    self.ui.close_cooking();
                    self.grab_cursor();
                }
                self.spawn_campfire_contents(block, state);
            }
            self.persist_survival();
        }
        if self.vitals.respawn() == Some(block) {
            self.vitals.clear_respawn();
            self.persist_survival();
        }
        self.spawn_block_drops(block, block_type, harvest_tool);
        self.rebuild_chunks(affected);
        self.mining.cancel();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.spawn_block_particles(block_type, block);
            renderer.set_selection(None);
        }
    }

    /// 营火被破坏时安全掉落输入与产出，保证烹饪物品不丢失。
    fn spawn_campfire_contents(
        &mut self,
        block: IVec3,
        state: crate::survival::cooking::CampfireState,
    ) {
        let center = Vec3::new(
            block.x as f32 + 0.5,
            block.y as f32 + 0.5,
            block.z as f32 + 0.5,
        );
        for stack in [state.input(), state.output()].into_iter().flatten() {
            let velocity = Vec3::new(0.0, 1.0, 0.0);
            self.dropped
                .spawn(stack, center, velocity, &self.drop_config);
        }
    }

    /// 按采集规则生成掉落物；初始速度带轻微抛出效果。
    fn spawn_block_drops(&mut self, block: IVec3, block_type: Block, harvest_tool: HarvestTool) {
        let center = Vec3::new(
            block.x as f32 + 0.5,
            block.y as f32 + 0.5,
            block.z as f32 + 0.5,
        );
        for (index, drop) in drops_for_harvest(block_type, harvest_tool)
            .iter()
            .enumerate()
        {
            let Some(stack) = ItemStack::new(drop.item, drop.count) else {
                continue;
            };
            let direction = if index % 2 == 0 { 1.0 } else { -1.0 };
            let velocity = Vec3::new(0.18 * direction, 1.2, -0.08 * direction);
            self.dropped
                .spawn(stack, center, velocity, &self.drop_config);
        }
    }

    /// 右键放置：通过统一的次要使用入口，把可放置方块放到命中面外侧的空格。
    fn place_block(&mut self) {
        let Some(hit) = self.targeted_block() else {
            return;
        };
        let Some(stack) = self.inventory.hotbar_stack(self.hotbar.selected_index()) else {
            debug!("当前快捷栏槽位为空，无法放置");
            return;
        };
        let block = match secondary_use(Some(stack)) {
            ItemUse::PlaceBlock(block) => block,
            _ => {
                debug!(item = ?stack.item(), "该物品没有放置行为");
                return;
            }
        };
        let target = match placement_cell(&self.world, &hit, &self.player, block) {
            Ok(target) => target,
            Err(reason) => {
                debug!(?reason, hit = ?hit.block, "方块放置被拒绝");
                return;
            }
        };
        // 营火与床属于功能性方块，必须放在实心地面之上。
        if matches!(block, Block::Campfire | Block::Bed)
            && !self
                .world
                .block_at(target.x, target.y - 1, target.z)
                .is_solid()
        {
            debug!(block = ?target, "功能性方块缺少地面支撑");
            self.show_message("需要坚实地面");
            return;
        }

        let affected = self.world.set_block(target.x, target.y, target.z, block);
        if affected.is_empty() {
            return;
        }

        // 世界写入成功后才扣减物品，避免放置失败时损失库存。
        self.inventory.remove_item(stack.item(), 1);
        self.persist_player();

        info!(block = ?target, block_type = ?block, "方块已放置");
        self.remember_edit(BlockEdit {
            x: target.x,
            y: target.y,
            z: target.z,
            block,
        });
        self.rebuild_chunks(affected);
    }

    /// 方块变化后重建受影响区块的网格。
    fn rebuild_chunks(&mut self, affected: Vec<ChunkPos>) {
        if affected.is_empty() {
            return;
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_chunks(&self.world, &affected);
        }
        // 已直接重建，避免调度器重复上传同一批区块。
        self.chunk_manager.skip_dirty(&affected);
    }

    /// 推进动态区块生命周期：加载/卸载区块、应用存档改动、按预算重建网格。
    fn stream_chunks(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        // 以玩家实体位置而非摄像机计算目标区块，摄像机只做渲染跟随。
        let position = self.player.position();
        let player_chunk =
            ChunkPos::from_world(position.x.floor() as i32, 0, position.z.floor() as i32);

        let start = Instant::now();
        // 写盘失败后按固定间隔重试，避免每帧反复读写整个存档文件。
        let now = Instant::now();
        if now >= self.next_save_retry {
            self.next_save_retry = now + SAVE_RETRY_INTERVAL;
            if let Err(err) = self.save.queue_persist() {
                warn!(error = %format!("{err:#}"), "世界存档重试写入失败");
            }
        }
        // 即使写盘失败，改动仍完整保留在 WorldSave 的内存待写入队列中，
        // 卸载区块不会丢失数据；若此时拒绝卸载，持续写盘失败会让已探索区块无界增长。
        //
        // 卸载保护：保留玩家所在区块及其水平一圈邻居。玩家碰撞体最多横跨一格，
        // 该范围覆盖任何可能的碰撞查询，避免依赖的方块数据先于玩家离开被卸载。
        let update = self.chunk_manager.update(
            player_chunk,
            &mut self.world,
            &mut |pos, world| self.save.apply_chunk(pos, world),
            &mut |pos: ChunkPos| {
                (pos.x - player_chunk.x)
                    .abs()
                    .max((pos.z - player_chunk.z).abs())
                    > 1
            },
        );
        self.generation_ms = start.elapsed().as_secs_f64() * 1000.0;
        if !update.loaded.is_empty() || !update.unloaded.is_empty() {
            debug!(
                loaded = update.loaded.len(),
                unloaded = update.unloaded.len(),
                "区块生命周期更新"
            );
        }

        // 区块卸载前保存所属掉落物与生物，重新加载的区块则从存档恢复，二者互不重复。
        for pos in &update.loaded {
            let records = self.save.chunk_drops(*pos).to_vec();
            self.dropped.restore_chunk(*pos, &records);
            let entities = self.save.chunk_entities(*pos).to_vec();
            self.entities.restore_chunk(*pos, &entities);
        }
        if !update.unloaded.is_empty() {
            for pos in &update.unloaded {
                let records = self.dropped.remove_chunk(*pos);
                self.save.record_chunk_drops(*pos, records);
                let entities = self.entities.take_chunk_entities(*pos);
                self.save.record_chunk_entities(*pos, entities);
            }
            // 后台线程合并写盘，主线程不等待 JSON 序列化与磁盘 IO。
            if let Err(err) = self.save.queue_persist() {
                warn!(error = %format!("{err:#}"), "卸载区块时写入存档失败");
            }
        }

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // 先释放已卸载区块的 GPU 资源，再补建本帧允许的网格。
        renderer.remove_chunks(&update.unloaded);

        let start = Instant::now();
        // 建网格同样按时间预算推进：一帧只花约 2ms，剩余区块放回队列下一帧继续。
        let deadline = start + CHUNK_MESH_BUDGET;
        let queue = self.chunk_manager.take_meshes(player_chunk, &self.world);
        let mut deferred = Vec::new();
        for (index, pos) in queue.iter().enumerate() {
            if Instant::now() >= deadline {
                deferred.extend_from_slice(&queue[index..]);
                break;
            }
            renderer.update_chunks(&self.world, &[*pos]);
        }
        self.chunk_manager.requeue_meshes(&deferred);
        self.mesh_ms = start.elapsed().as_secs_f64() * 1000.0;
    }

    /// 退出前确保内存中的方块改动、掉落物、玩家背包与生命值已经落盘。
    fn flush_save(&mut self) {
        // 拖动中的物品先归位，避免关闭界面时丢失。
        self.ui.force_close(&mut self.inventory);
        self.persist_player();
        self.persist_survival();

        for pos in self.world.chunk_positions() {
            let records = self.dropped.snapshot_chunk(pos);
            self.save.record_chunk_drops(pos, records);
            let entities = self.entities.snapshot_chunk(pos);
            self.save.record_chunk_entities(pos, entities);
        }
        if let Err(err) = self.save.ensure_persisted() {
            warn!(error = %format!("{err:#}"), "退出前写入世界存档失败");
        }
    }

    /// 把当前背包、快捷栏选择、生命、装备、饥饿与体力写入角色快照。
    fn persist_player(&mut self) {
        self.write_character_snapshot();
    }

    /// 把角色快照与世界侧状态（昼夜时间、营火）分别写入各自存档。
    fn persist_survival(&mut self) {
        self.write_character_snapshot();
        if self.active_world.is_some() {
            if let Err(err) = self
                .save
                .set_world_state(self.world_time.to_saved(), self.cooking.to_saved())
            {
                warn!(error = %format!("{err:#}"), "世界状态写入存档失败");
            }
        }
    }

    /// 把发生变化的区块掉落物快照写回存档。
    fn persist_dirty_drops(&mut self) {
        let chunks = self.dropped.take_dirty_chunks();
        if chunks.is_empty() {
            return;
        }
        for pos in chunks {
            let records = self.dropped.snapshot_chunk(pos);
            self.save.record_chunk_drops(pos, records);
        }
        if let Err(err) = self.save.queue_persist() {
            warn!(error = %format!("{err:#}"), "掉落物变化写入存档失败");
        }
    }

    /// 把一次改动写入存档；失败时只记录日志，不影响继续游戏。
    fn remember_edit(&mut self, edit: BlockEdit) {
        if let Err(err) = self.save.record(edit) {
            warn!(error = %format!("{err:#}"), "世界存档写入失败");
        }
    }

    /// 开发期纹理热重载：轮询文件变化，成功时替换 GPU 图集并重建区块网格。
    fn poll_texture_reload(&mut self) {
        let now = Instant::now();
        if now < self.next_texture_poll {
            return;
        }
        self.next_texture_poll = now + TEXTURE_POLL_INTERVAL;
        if !self.texture_watcher.poll() {
            return;
        }

        let assets = Arc::new(TextureAssets::load(&self.asset_root));
        if !assets.assets_ok() {
            warn!(
                errors = assets.errors.len(),
                current_placeholders = self.textures.summary().placeholders,
                "纹理热重载失败，保留上一份有效纹理"
            );
            return;
        }

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.reload_textures(assets.clone(), &self.world);
        }
        self.ui.set_ui_textures(&assets.ui_textures);
        self.textures = assets;
        info!("纹理资源已热重载");
    }

    fn draw_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        // 菜单与暂停界面冻结世界：不推进物理、时间、区块与实体，只重绘界面。
        if self.screen != AppScreen::InGame {
            self.draw_menu_frame();
            self.present(event_loop);
            return;
        }
        let delta_secs = delta.as_secs_f32();
        // 周期性保存生存状态，避免异常退出丢失饥饿、体力、时间与烹饪进度。
        let now = Instant::now();
        if !self.paused && now >= self.next_survival_save {
            self.next_survival_save = now + Duration::from_secs(15);
            self.persist_survival();
        }
        // 视角旋转与物理位移解耦：鼠标视角立即生效，位移走固定步长物理。
        let look = self.controller.take_look();
        self.player.rotate_look(look.0, look.1);
        self.controller.tick(delta_secs);

        let mut intent = self.controller.intent();
        let moving = intent.forward != 0.0 || intent.right != 0.0;
        let ui_blocking = self.ui.blocks_world_input();
        // 冲刺由体力与饥饿共同约束；界面阻塞、暂停或死亡时不允许冲刺。
        intent.sprint = self.controller.sprint()
            && self
                .vitals
                .can_sprint(!self.survival.is_dead(), ui_blocking || self.paused, moving);
        let sprint_active = intent.sprint;
        // 行走/奔跑速度与跳跃高度由当前饥饿比例派生。
        let hunger = *self.vitals.hunger();
        let gravity = self.physics.config().gravity;
        self.physics.set_movement_speeds(
            hunger.walk_speed(),
            hunger.run_speed(),
            jump_speed_for_height(hunger.jump_height(), gravity),
        );
        let physics_world = PhysicsWorld::new(&self.world);
        let update = self
            .physics
            .update(&mut self.player, &intent, &physics_world, delta_secs);
        self.physics_steps = update.steps;

        // 生命值计时与真实落地事件结算：下落距离由物理步落地瞬间计算，
        // 不按帧高度差累加，避免插值与积分噪声产生误伤。
        self.survival.update(delta_secs);
        if let Some(distance) = update.landing_fall {
            if let Some(fall_damage) = damage::fall_damage(distance) {
                self.apply_damage(fall_damage);
            }
        }

        // 生存状态只随固定物理步推进，避免掉帧导致饥饿与体力异常消耗。
        let game_dt = update.steps as f32 * FIXED_TIMESTEP;
        if !self.paused && !self.survival.is_dead() && game_dt > 0.0 {
            let healed = self.vitals.update(
                game_dt,
                VitalsInput {
                    moving,
                    sprinting: sprint_active,
                    jumps: update.jumps,
                    blocked: ui_blocking,
                },
                true,
                self.survival.health() < self.survival.max_health(),
            );
            for _ in 0..healed {
                self.survival.heal(1);
            }
            self.world_time.advance(game_dt);
            let cooked = self.cooking.update(game_dt);
            if !cooked.is_empty() {
                self.cooking.take_dirty();
                self.persist_survival();
            }
        }

        // 进食会被移动、跳跃、界面、死亡与暂停打断，且打断不产生恢复。
        if self.eating.is_eating() {
            let interrupted = ui_blocking
                || self.paused
                || self.survival.is_dead()
                || moving
                || sprint_active
                || intent.jump;
            if interrupted {
                self.eating.cancel();
            } else if self.eating.update(game_dt) == EatingUpdate::Completed
                && self
                    .eating
                    .consume(
                        &mut self.inventory,
                        self.vitals.hunger_mut(),
                        &mut self.game_rng,
                    )
                    .is_some()
            {
                self.persist_player();
                self.persist_survival();
            }
        }

        // 生物物理与玩家物理共用固定步长；重叠时把玩家水平推开，避免相互穿透。
        let player_box = self.player.aabb();
        self.entity_physics.update(
            self.entities.creatures_mut(),
            &self.world,
            Some(player_box),
            delta_secs,
        );
        let mut separated = self.player.position();
        let entity_physics_world = PhysicsWorld::new(&self.world);
        let dimensions = self.physics.config().dimensions;
        entity_collision::resolve_player_overlaps(
            &mut separated,
            dimensions.width,
            dimensions.height,
            self.entities.creatures(),
            &|x, y, z| entity_physics_world.is_solid(x, y, z),
        );
        let shift = separated - self.player.position();
        if shift != Vec3::ZERO {
            self.player.shift_position(shift);
        }

        // AI 决策频率由距离控制，移动仍由物理步执行。
        let hostile_attacks = self.entities.update_ai(
            delta_secs,
            self.player.position(),
            &self.world,
            &self.ai_config,
        );
        for attack in hostile_attacks {
            self.apply_entity_attack(attack.attacker, attack.damage);
        }
        self.update_combat(delta_secs);
        self.entities.update_deaths(delta_secs);

        // 摄像机用物理插值系数跟随玩家，避免固定步长与渲染帧率不同步产生抖动。
        let alpha = update.alpha;
        let eye = self.player.interpolated_eye_position(alpha);
        let yaw = self.player.yaw();
        let pitch = self.player.pitch();
        // 第三人称只调整观察位置：相机沿视线反向外推，并在遇到方块时收缩。
        let camera_position = match self.camera_mode {
            CameraMode::FirstPerson => eye,
            CameraMode::ThirdPerson => third_person::camera_position(
                eye,
                yaw,
                pitch,
                ThirdPersonConfig::default(),
                |x, y, z| self.world.block_at(x, y, z).is_solid(),
            ),
        };
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.camera_mut().set_pose(camera_position, yaw, pitch);
        }

        // 开发期纹理修改后重新装配资源；失败时保留上一份有效纹理。
        self.poll_texture_reload();

        // 物理移动后先补齐区块，再让射线检测基于最新的已加载世界。
        self.stream_chunks();

        // 自然生成依赖已加载区块；新区块列一旦生成就写入存档，避免重载后重复生成。
        let spawn_player_box = self.player.aabb();
        let populated = self.spawner.update(
            &mut self.entities,
            &self.world,
            self.player.position(),
            spawn_player_box,
            &|column| self.save.is_chunk_populated(column),
            delta_secs,
        );
        if !populated.is_empty() {
            for column in populated {
                self.save.record_populated_chunk(column);
            }
            if let Err(err) = self.save.queue_persist() {
                warn!(error = %format!("{err:#}"), "自然生成记录写入存档失败");
            }
        }

        // 新生成的生物也要在本帧立即获得模型实例与待机姿态。
        self.entity_models
            .update(self.entities.creatures(), &self.models, delta_secs);

        // 掉落物物理与自动拾取；拾取后立即同步玩家背包与所属区块存档。
        self.dropped
            .update(delta_secs, &self.world, &self.drop_config);
        // 以玩家身体中心作为拾取判定点。
        let center = self.player.position() + Vec3::Y * 0.9;
        let picked = collect_pickups(
            &mut self.dropped,
            &mut self.inventory,
            center,
            &self.drop_config,
        );
        self.persist_dirty_drops();
        if !picked.is_empty() {
            if let Some(renderer) = self.renderer.as_mut() {
                if let Some((item, count)) = picked.headline() {
                    renderer.hud_mut().show_pickup(item, count);
                }
            }
            self.persist_player();
        }

        // 推进 UI 短动画与提示计时；界面状态由 UiManager 统一维护。
        self.ui.update(delta_secs);
        if ui_blocking {
            self.mining.cancel();
        }
        let hit = if ui_blocking {
            None
        } else {
            self.targeted_block()
        };
        let mining_target = self.mining_target(hit);
        let outcome =
            self.mining
                .update(self.mining_held && !ui_blocking, mining_target, delta_secs);
        // 破坏方块按累计挖掘时间消耗体力：每累计 1 秒扣 1 点。
        if !self.paused
            && !self.survival.is_dead()
            && matches!(
                outcome,
                MiningOutcome::Mining { .. } | MiningOutcome::Completed { .. }
            )
        {
            self.vitals.drain_mining(delta_secs);
        }

        // 视觉层只消费本帧状态：真实速度、落地事件、战斗/挖掘/进食与装备数据。
        let feet = eye - Vec3::Y * self.physics.config().dimensions.eye_height;
        let held_stack = self.inventory.hotbar_stack(self.hotbar.selected_index());
        let visual_state = PlayerVisualState {
            feet_position: feet,
            yaw,
            velocity: self.player.velocity(),
            grounded: self.player.is_grounded(),
            health: self.survival.health(),
            dead: self.survival.is_dead(),
            mining: self.mining.is_mining(),
            eating: self.eating.is_eating(),
            attacking: !self.combat.state().is_idle(),
            sprinting: sprint_active,
            landed: update.landing_fall.is_some(),
        };
        self.player_visual.update(
            &self.models,
            &self.equipment,
            held_stack,
            &visual_state,
            delta_secs,
        );

        let mut selection = hit.map(|hit| hit.block);
        if let MiningOutcome::Completed { block } = outcome {
            let block_type = self.world.block_at(block.x, block.y, block.z);
            let selected = self.hotbar.selected_index();
            let tool_stack = self.inventory.hotbar_stack(selected);
            let harvest_tool = HarvestTool::from_stack(tool_stack);

            // 一次成功挖掘只结算一次耐久；普通物品与损坏工具不参与扣减。
            let wear = if tool_stack.is_some_and(|stack| stack.is_tool() && !stack.is_broken()) {
                self.inventory.damage_hotbar_tool(selected, 1)
            } else {
                Wear::NotATool
            };
            self.break_block(block, block_type, harvest_tool);
            if wear != Wear::NotATool {
                self.persist_player();
            }
            if wear == Wear::Broken {
                info!(item = ?tool_stack.map(ItemStack::item), "工具已损坏");
                if let (Some(renderer), Some(stack)) = (self.renderer.as_mut(), tool_stack) {
                    renderer.hud_mut().show_tool_broken(stack.item());
                }
            }
            // 方块已变成空气，本帧不应再高亮旧目标。
            selection = None;
        }

        // 准星指向敌对生物时持续刷新名牌，离开准星后由 HUD 自行淡出。
        let aim_target = if ui_blocking {
            None
        } else {
            self.targeted_creature().and_then(|id| {
                let creature = self.entities.get(id)?;
                if !EntityManager::is_hostile(creature.kind()) {
                    return None;
                }
                let definition = entity_registry::definition(creature.kind())?;
                Some((
                    definition.name,
                    creature.entity.health(),
                    definition.max_health,
                ))
            })
        };
        let slots = self.inventory.hotbar_slots();
        let selected_slot = self.hotbar.selected_index();
        let progress = self.mining.progress();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.hud_mut().update(delta_secs);
            if let Some((name, health, max_health)) = aim_target {
                renderer.hud_mut().show_target(name, health, max_health);
            }
            renderer
                .hud_mut()
                .set_health(self.survival.health(), self.survival.max_health());
            renderer.hud_mut().set_survival(
                self.vitals.hunger().value(),
                self.vitals.hunger().max(),
                self.vitals.stamina().value(),
                self.vitals.stamina().max(),
                self.world_time.phase().label(),
            );
            renderer
                .hud_mut()
                .set_eating_progress(self.eating.progress());
            renderer.set_selection(if ui_blocking { None } else { selection });
            renderer
                .hud_mut()
                .set(selected_slot, slots, progress, ui_blocking);
            renderer.update_particles(delta_secs);
            self.dropped
                .build_vertices(renderer.drop_vertices_mut(), &self.textures.item_textures);
            let view_proj = renderer.camera().view_proj();
            let camera_position = renderer.camera().position();
            let mut model_draws: Vec<ModelDraw> = Vec::new();
            self.entity_models.build_draws(
                self.entities.creatures(),
                &self.models,
                self.entity_physics.alpha(),
                view_proj,
                camera_position,
                &mut model_draws,
            );
            // 第一人称隐藏第三人称身体，仅绘制独立手臂与手持物品；
            // 第三人称绘制完整角色与全部装备挂载。第一人称批次走深度关闭的覆盖层，
            // 保证贴墙时手臂不会被世界几何裁掉。
            let mut overlay_model_draws: Vec<ModelDraw> = Vec::new();
            let target = if self.camera_mode == CameraMode::FirstPerson {
                &mut overlay_model_draws
            } else {
                &mut model_draws
            };
            self.player_visual.build_draws(
                &self.models,
                self.camera_mode,
                camera_position,
                yaw,
                pitch,
                target,
            );
            renderer.set_model_draws(model_draws);
            renderer.set_overlay_model_draws(overlay_model_draws);
            let modeled = self.entity_models.modeled_types().to_vec();
            let entity_vertices = renderer.entity_vertices_mut();
            entity_vertices.clear();
            entity_render::build_vertices(
                self.entities.creatures(),
                self.entity_physics.alpha(),
                view_proj,
                camera_position,
                &self.entity_render_config,
                &modeled,
                entity_vertices,
            );
            let viewport = renderer.viewport();
            let mut ui_vertices = Vec::new();
            let cooking_state = self
                .ui
                .cooking_campfire()
                .and_then(|pos| self.cooking.state(pos));
            self.ui.build_with_cooking(
                &mut ui_vertices,
                &self.inventory,
                &self.equipment,
                selected_slot,
                &self.textures.item_textures,
                viewport,
                recipe_registry::all(),
                cooking_state,
            );
            renderer.set_ui_vertices(ui_vertices);
        }

        self.present(event_loop);
    }

    /// 绘制菜单或暂停界面；暂停时保留上一帧的世界与实体画面作为冻结背景。
    fn draw_menu_frame(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let viewport = renderer.viewport();
        renderer
            .hud_mut()
            .set(0, [None; crate::item::HOTBAR_SLOTS], None, true);
        let mut ui_vertices = Vec::new();
        self.menu.build(&mut ui_vertices, viewport);
        renderer.set_ui_vertices(ui_vertices);
        if self.screen == AppScreen::Menu {
            // 主菜单/选择界面不显示上个世界的模型、生物与掉落物。
            renderer.set_model_draws(Vec::new());
            renderer.set_overlay_model_draws(Vec::new());
            renderer.entity_vertices_mut().clear();
            renderer.drop_vertices_mut().clear();
        }
    }

    /// 提交并呈现当前帧，处理 surface 失效与致命错误。
    fn present(&mut self, event_loop: &ActiveEventLoop) {
        let outcome = match self.renderer.as_mut() {
            Some(renderer) => renderer.render(),
            None => return,
        };

        match outcome {
            RenderOutcome::Presented | RenderOutcome::Skipped => {}
            RenderOutcome::Reconfigure => {
                warn!("surface 已失效，重新配置");
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.reconfigure();
                }
            }
            RenderOutcome::Fatal(err) => {
                error!(error = %format!("{err:#}"), "渲染失败，退出");
                event_loop.exit();
            }
        }
    }
}

/// 数字键 1-9 对应的快捷栏槽位；其他按键返回 `None`。
fn hotbar_slot_for_key(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::Digit1 => Some(0),
        KeyCode::Digit2 => Some(1),
        KeyCode::Digit3 => Some(2),
        KeyCode::Digit4 => Some(3),
        KeyCode::Digit5 => Some(4),
        KeyCode::Digit6 => Some(5),
        KeyCode::Digit7 => Some(6),
        KeyCode::Digit8 => Some(7),
        KeyCode::Digit9 => Some(8),
        _ => None,
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.create_window(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                info!("收到关闭请求，退出");
                self.flush_save();
                event_loop.exit();
            }
            WindowEvent::Ime(ime) => {
                if self.screen == AppScreen::InGame {
                    return;
                }
                match ime {
                    Ime::Commit(text) => self.menu.ime_commit(&text),
                    Ime::Preedit(text, _) => {
                        self.menu.ime_preedit((!text.is_empty()).then_some(text.as_str()));
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                // 菜单与暂停界面独占键盘：重命名输入、Esc 返回/继续均在此处理。
                if self.screen != AppScreen::InGame {
                    let action =
                        self.menu
                            .key_with_text(code, pressed, event.text.as_deref());
                    if action != MenuAction::None {
                        self.handle_menu_action(action, event_loop);
                    }
                    self.sync_ime();
                    return;
                }
                // 统一 UI 快捷键路由：一次按键最多产生一个界面意图。
                // 死亡界面不响应任何 UI 快捷键，避免误操作绕过死亡流程。
                if let Some(intent) =
                    crate::ui::input::route_key(self.ui.state(), code, pressed, self.cursor_grabbed)
                {
                    self.handle_ui_intent(intent);
                    return;
                }
                // 界面打开时只响应开关界面，暂停移动与视角输入。
                if self.ui.blocks_world_input() {
                    return;
                }
                // 未捕获鼠标时键盘不驱动相机，避免释放鼠标后误移动。
                if !self.cursor_grabbed {
                    return;
                }
                // F5 在第一/第三人称之间切换；仅影响相机与绘制，不写入存档。
                if pressed && code == KeyCode::F5 {
                    self.camera_mode = self.camera_mode.toggle();
                    self.show_message(self.camera_mode.label());
                    return;
                }
                if let Some(slot) = hotbar_slot_for_key(code) {
                    if pressed && self.hotbar.select_slot(slot) {
                        self.eating.cancel();
                        self.persist_player();
                    }
                    return;
                }
                self.controller.handle_key(code, pressed);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let x = position.x as f32;
                let y = position.y as f32;
                self.ui.set_cursor(x, y);
                self.menu.set_cursor(x, y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // 菜单与暂停界面独占鼠标；只在按下时解析一次点击。
                if self.screen != AppScreen::InGame {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let viewport = self
                            .renderer
                            .as_ref()
                            .map_or((1280.0, 720.0), Renderer::viewport);
                        let action = self.menu.click_at(viewport, self.menu.cursor());
                        self.handle_menu_action(action, event_loop);
                        self.sync_ime();
                    }
                    return;
                }
                // 任意全屏界面打开时，鼠标只交给当前界面处理；
                // 死亡界面优先级由 UiManager 的状态机保证。
                if self.ui.blocks_world_input() {
                    let viewport = self
                        .renderer
                        .as_ref()
                        .map_or((1280.0, 720.0), Renderer::viewport);
                    let click = match button {
                        MouseButton::Left => Some(ClickButton::Left),
                        MouseButton::Right => Some(ClickButton::Right),
                        _ => None,
                    };
                    if let Some(click) = click {
                        let cooking_state = self
                            .ui
                            .cooking_campfire()
                            .and_then(|pos| self.cooking.state_mut(pos));
                        match self.ui.click_with_cooking(
                            click,
                            state == ElementState::Pressed,
                            &mut self.inventory,
                            &mut self.equipment,
                            viewport,
                            recipe_registry::all(),
                            cooking_state,
                        ) {
                            UiPointerAction::None => {}
                            UiPointerAction::InventoryChanged => self.persist_player(),
                            UiPointerAction::CookingChanged => {
                                self.persist_player();
                                self.persist_survival();
                            }
                            UiPointerAction::Craft(recipe_id) => self.craft_recipe(recipe_id),
                            UiPointerAction::Respawn => self.respawn(),
                            UiPointerAction::MainMenu => {
                                info!("死亡界面选择返回主菜单");
                                self.leave_to_main_menu();
                            }
                        }
                    }
                    return;
                }
                if !self.cursor_grabbed {
                    // 首次点击只用于捕获鼠标，避免误触挖掘。
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.grab_cursor();
                    }
                    return;
                }
                match (button, state) {
                    // 左键按住持续挖掘，松开立即中断并清空进度。
                    // 主要使用入口只用于识别物品语义；徒手与方块物品仍可徒手挖掘，
                    // 实际效率由 Mining 依据当前工具倍率计算。
                    (MouseButton::Left, ElementState::Pressed) => {
                        if self.targeted_creature().is_some() {
                            self.start_attack();
                            self.mining_held = false;
                            self.mining.cancel();
                        } else {
                            let stack = self.inventory.hotbar_stack(self.hotbar.selected_index());
                            debug!(primary = ?primary_use(stack), "主要使用");
                            self.mining_held = true;
                        }
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        self.mining_held = false;
                        self.mining.cancel();
                    }
                    // 右键只在按下瞬间触发一次物品使用，避免长按连续放置。
                    (MouseButton::Right, ElementState::Pressed) => self.secondary_use(),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.screen != AppScreen::InGame {
                    return;
                }
                // 死亡或合成界面打开时滚轮不切换快捷栏；背包内仍可预览选中格。
                match self.ui.state() {
                    UiState::Death | UiState::Crafting | UiState::Cooking => return,
                    UiState::Gameplay if !self.cursor_grabbed => return,
                    _ => {}
                };
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                let before = self.hotbar.selected_index();
                self.hotbar.scroll(amount);
                if before != self.hotbar.selected_index() {
                    self.eating.cancel();
                    self.persist_player();
                }
            }
            WindowEvent::Focused(false) => {
                self.paused = true;
                self.eating.cancel();
                if self.cursor_grabbed {
                    warn!("窗口失去焦点，释放鼠标捕获");
                    self.release_cursor();
                } else {
                    self.controller.reset();
                }
            }
            WindowEvent::Focused(true) => {
                self.paused = false;
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                self.next_frame = Instant::now();
            }
            WindowEvent::RedrawRequested => {
                let stats = self.timer.tick();
                let frame_start = Instant::now();
                self.draw_frame(stats.delta, event_loop);

                if let Some(fps) = stats.fps {
                    let player = self.player.position();
                    let player_chunk =
                        ChunkPos::from_world(player.x.floor() as i32, 0, player.z.floor() as i32);
                    let gpu_chunks = self
                        .renderer
                        .as_ref()
                        .map_or(0, crate::renderer::Renderer::chunk_count);
                    info!(
                        fps = f64::from(fps),
                        delta_ms = stats.delta.as_secs_f64() * 1000.0,
                        player_x = player.x,
                        player_y = player.y,
                        player_z = player.z,
                        player_vy = self.player.velocity().y,
                        player_ay = self.player.acceleration().y,
                        player_grounded = self.player.is_grounded(),
                        player_state = ?self.player.movement_state(),
                        physics_steps = self.physics_steps,
                        chunk_x = player_chunk.x,
                        chunk_z = player_chunk.z,
                        loaded_chunks = self.world.chunk_count(),
                        gpu_chunks,
                        pending_generation = self.chunk_manager.pending_generation(),
                        pending_meshes = self.chunk_manager.pending_mesh(),
                        generation_ms = self.generation_ms,
                        mesh_ms = self.mesh_ms,
                        selected_hotbar = self.hotbar.selected_index(),
                        selected_tool_durability = ?self
                            .inventory
                            .hotbar_stack(self.hotbar.selected_index())
                            .and_then(ItemStack::durability),
                        inventory_open = matches!(self.ui.state(), UiState::Inventory),
                        crafting_open = matches!(self.ui.state(), UiState::Crafting),
                        ui_state = self.ui.state().label(),
                        dead = self.survival.is_dead(),
                        health = self.survival.health(),
                        max_health = self.survival.max_health(),
                        invulnerability = self.survival.invulnerability_remaining(),
                        dropped_items = self.dropped.len(),
                        entities = self.entities.len(),
                        alive_entities = self.entities.alive_count(),
                        mining_active = self.mining.is_mining(),
                        mining_target = ?self.mining.target(),
                        mining_progress = self.mining.progress(),
                        tool_speed = ?self.mining.target().map(|block| {
                            let block_type = self.world.block_at(block.x, block.y, block.z);
                            tool::mining_speed(
                                self.inventory.hotbar_stack(self.hotbar.selected_index()),
                                block_type,
                            )
                        }),
                        mining_elapsed = self.mining.elapsed_time(),
                        mining_required = self.mining.required_time(),
                        "帧率统计"
                    );
                }
                // 以帧起点为基准排下一帧，避免与 vsync 叠加成双重限帧。
                self.next_frame = frame_start + FRAME_INTERVAL;
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if !self.cursor_grabbed {
            return;
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            self.controller.add_look(delta.0 as f32, delta.1 as f32);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
    }
}
