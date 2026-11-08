//! 应用主体：窗口创建与主循环。

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};
use tracing::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use super::config::AppConfig;
use super::time::FrameTimer;
use crate::entity::{
    entity_collision, entity_registry, entity_render, AiConfig, DamageResult, EntityId,
    EntityManager, EntityPhysicsConfig, EntityPhysicsSystem, EntityRenderConfig, SpawnConfig,
    Spawner,
};
use crate::interaction::{
    drops_for_harvest, placement_cell, primary_use, raycast, secondary_use, HarvestTool, Hotbar,
    ItemUse, MiningOutcome, MiningState, MiningTarget, RayHit,
};
use crate::item::{
    collect_pickups, crafting, recipe_registry, tool, DroppedItemConfig, DroppedItemStore,
    Inventory, ItemId, ItemStack, RecipeCategory, RecipeId, Wear, HOTBAR_OFFSET,
};
use crate::player::{
    damage, find_respawn_position, Damage, DamageOutcome, DamageSource, PhysicsConfig,
    PhysicsSystem, PhysicsWorld, Player, PlayerController, Survival,
};
use crate::renderer::{spawn_pose, RenderOutcome, Renderer};
use crate::save::{BlockEdit, WorldSave};
use crate::texture::{TextureAssets, TextureWatcher, DEFAULT_ASSET_ROOT};
use crate::ui::{ClickButton, UiIntent, UiManager, UiPointerAction, UiState};
use crate::world::{Block, ChunkConfig, ChunkManager, ChunkPos, World};

/// 目标帧间隔，约 60 FPS。
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// 准星射线长度：超出该距离的方块不会被选中。
const SELECTION_REACH: f32 = 8.0;
/// 生物交互距离：准星在该距离内命中生物时可造成伤害。
const ENTITY_REACH: f32 = 4.0;
/// 单次测试攻击的基础伤害；只用于验证伤害与掉落闭环。
const CREATURE_TEST_DAMAGE: u32 = 3;
/// 存档写入失败后的重试间隔，避免每帧都重复写盘。
const SAVE_RETRY_INTERVAL: Duration = Duration::from_secs(2);
/// 纹理热重载轮询间隔；开发期修改 PNG 后约 1 秒内生效。
const TEXTURE_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// 纹理资源根目录环境变量；未设置时使用 `assets`。
const ASSET_ROOT_ENV: &str = "NEVERLAND_ASSET_ROOT";

/// 持有窗口、渲染器与主循环状态。
pub struct App {
    config: AppConfig,
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

impl App {
    pub fn new(config: AppConfig) -> Self {
        let world = World::default();
        let save = WorldSave::load(WorldSave::default_path(), world.seed());
        // 物理参数集中管理，玩家碰撞体尺寸也由同一处提供。
        let physics = PhysicsSystem::new(PhysicsConfig::default());
        let (spawn, yaw, pitch) = spawn_pose(&world);
        let mut player = Player::new(spawn, physics.config().dimensions);
        player.rotate_look(yaw, pitch);

        // 旧档沿用已有背包；新世界只发放无法通过采集获得的铁锭测试材料。
        let (inventory, selected_hotbar) = save
            .player()
            .map(Inventory::from_saved)
            .unwrap_or_else(|| (Self::starter_inventory(), 0));
        let survival = save
            .survival()
            .map(Survival::from_saved)
            .unwrap_or_default();
        let spawn_seed = world.seed();
        let asset_root =
            std::env::var(ASSET_ROOT_ENV).unwrap_or_else(|_| DEFAULT_ASSET_ROOT.to_string());
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
        let mut ui = UiManager::new();
        ui.set_ui_textures(&textures.ui_textures);
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
            spawn_anchor: spawn,
            dropped: DroppedItemStore::new(),
            drop_config: DroppedItemConfig::default(),
            entities: EntityManager::new(),
            entity_physics: EntityPhysicsSystem::new(EntityPhysicsConfig::default()),
            spawner: Spawner::new(SpawnConfig::default(), spawn_seed),
            ai_config: AiConfig::default(),
            entity_render_config: EntityRenderConfig::default(),
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
        inventory
    }

    /// 创建事件循环并进入主循环，窗口关闭或按下 Esc 时返回。
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
                // 先按出生点填满初始加载区域，渲染器初始化时即可一次性上传网格。
                let spawn = self.player.position();
                let player_chunk =
                    ChunkPos::from_world(spawn.x.floor() as i32, 0, spawn.z.floor() as i32);
                let loaded =
                    self.chunk_manager
                        .preload(player_chunk, &mut self.world, &mut |pos, world| {
                            self.save.apply_chunk(pos, world)
                        });
                info!(chunks = self.world.chunk_count(), loaded, "初始区块已加载");

                // 存档中属于已加载区块的掉落物与生物恢复为运行时实体。
                let loaded_chunks: Vec<ChunkPos> = self.world.chunk_positions().collect();
                for pos in loaded_chunks {
                    let records = self.save.chunk_drops(pos).to_vec();
                    self.dropped.restore_chunk(pos, &records);
                    let entities = self.save.chunk_entities(pos).to_vec();
                    self.entities.restore_chunk(pos, &entities);
                }

                match Renderer::new(
                    window.clone(),
                    window.inner_size(),
                    &self.world,
                    self.player.eye_position(),
                    self.player.yaw(),
                    self.player.pitch(),
                    self.textures.clone(),
                ) {
                    Ok(renderer) => {
                        info!("渲染器已初始化");
                        self.renderer = Some(renderer);
                    }
                    Err(err) => {
                        error!(error = %format!("{err:#}"), "渲染器初始化失败");
                        event_loop.exit();
                    }
                }

                self.window = Some(window);
                self.timer = FrameTimer::new();
                self.next_frame = Instant::now();
            }
            Err(err) => {
                error!(error = %err, "窗口创建失败");
                event_loop.exit();
            }
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
    fn handle_ui_intent(&mut self, intent: UiIntent, event_loop: &ActiveEventLoop) {
        match intent {
            UiIntent::OpenInventory => self.open_inventory(),
            UiIntent::CloseInventory => self.close_inventory(),
            UiIntent::OpenCrafting => self.open_crafting(),
            UiIntent::CloseCrafting => self.close_crafting(),
            UiIntent::ReleaseCursor => self.release_cursor(),
            UiIntent::Quit => {
                info!("按下 Esc，退出");
                self.flush_save();
                event_loop.exit();
            }
        }
    }

    /// 统一伤害结算：刷新 HUD 反馈，并在致死时进入死亡流程。
    fn apply_damage(&mut self, damage: Damage) -> DamageOutcome {
        let source = damage.source();
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

    /// 重生：确保出生区块已加载，在出生点附近寻找安全落点并恢复满血。
    fn respawn(&mut self) {
        self.ensure_spawn_chunks();
        let dimensions = self.physics.config().dimensions;
        let position = find_respawn_position(&self.world, self.spawn_anchor, dimensions);
        self.player.teleport(position);
        self.physics.reset();
        self.survival.respawn();
        self.ui.leave_death();
        self.controller.reset();
        self.persist_player();
        info!(x = position.x, y = position.y, z = position.z, "玩家已重生");
    }

    /// 重生前同步加载出生点周围区块，保证安全落点搜索看到真实地形。
    fn ensure_spawn_chunks(&mut self) {
        let spawn = self.spawn_anchor;
        let spawn_chunk = ChunkPos::from_world(spawn.x.floor() as i32, 0, spawn.z.floor() as i32);
        let desired = ChunkManager::desired_positions(spawn_chunk, 1);
        let mut loaded = Vec::new();
        let mut edited = Vec::new();
        for pos in desired {
            if self.world.contains_chunk(pos) {
                continue;
            }
            let chunk = self.world.generate_chunk(pos);
            if !self.world.insert_chunk(chunk) {
                continue;
            }
            edited.extend(self.save.apply_chunk(pos, &mut self.world));
            let records = self.save.chunk_drops(pos).to_vec();
            self.dropped.restore_chunk(pos, &records);
            let entities = self.save.chunk_entities(pos).to_vec();
            self.entities.restore_chunk(pos, &entities);
            loaded.push(pos);
        }
        if loaded.is_empty() {
            return;
        }

        // 新加载区块、其水平邻居以及存档改动的相邻区块都要重建，避免边界残面。
        let mut affected = loaded.clone();
        for pos in &loaded {
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let neighbor = ChunkPos::new(pos.x + dx, pos.y, pos.z + dz);
                    if neighbor != *pos
                        && self.world.contains_chunk(neighbor)
                        && !affected.contains(&neighbor)
                    {
                        affected.push(neighbor);
                    }
                }
            }
        }
        for pos in edited {
            if self.world.contains_chunk(pos) && !affected.contains(&pos) {
                affected.push(pos);
            }
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_chunks(&self.world, &affected);
        }
        self.chunk_manager.skip_dirty(&affected);
    }

    /// 当前准星命中的生物；若方块比生物更近则不返回，避免隔墙攻击。
    fn targeted_creature(&self) -> Option<EntityId> {
        let renderer = self.renderer.as_ref()?;
        let camera = renderer.camera();
        let origin = camera.position();
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

    /// 对生物造成一次基础测试伤害；击杀时只结算一次掉落。
    fn attack_creature(&mut self, id: EntityId) {
        let Some((kind, position)) = self
            .entities
            .get(id)
            .map(|creature| (creature.kind(), creature.entity.position()))
        else {
            return;
        };
        let source = self.player.eye_position();
        match self.entities.apply_damage(
            id,
            CREATURE_TEST_DAMAGE,
            source,
            self.ai_config.flee_seconds,
        ) {
            DamageResult::Killed => {
                info!(entity = id.raw(), ?kind, "生物已被击杀");
                self.spawn_creature_drops(kind, position, id);
            }
            DamageResult::Applied { remaining } => {
                debug!(entity = id.raw(), remaining, "生物受到伤害");
            }
            DamageResult::Ignored => {}
        }
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

    /// 当前准星命中的方块。
    fn targeted_block(&self) -> Option<RayHit> {
        let renderer = self.renderer.as_ref()?;
        let camera = renderer.camera();
        raycast(
            &self.world,
            camera.position(),
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
        self.spawn_block_drops(block, block_type, harvest_tool);
        self.rebuild_chunks(affected);
        self.mining.cancel();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.spawn_block_particles(block_type, block);
            renderer.set_selection(None);
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
            if let Err(err) = self.save.ensure_persisted() {
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
            if let Err(err) = self.save.ensure_persisted() {
                warn!(error = %format!("{err:#}"), "卸载区块时写入存档失败");
            }
        }

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // 先释放已卸载区块的 GPU 资源，再补建本帧允许的网格。
        renderer.remove_chunks(&update.unloaded);

        let start = Instant::now();
        let meshes = self.chunk_manager.take_meshes(player_chunk, &self.world);
        renderer.update_chunks(&self.world, &meshes);
        self.mesh_ms = start.elapsed().as_secs_f64() * 1000.0;
    }

    /// 退出前确保内存中的方块改动、掉落物、玩家背包与生命值已经落盘。
    fn flush_save(&mut self) {
        // 拖动中的物品先归位，避免关闭界面时丢失。
        self.ui.force_close(&mut self.inventory);
        self.persist_player();

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

    /// 把当前背包、快捷栏选择与生存状态写入存档；失败只记录日志，不中断游戏。
    fn persist_player(&mut self) {
        let saved = self.inventory.to_saved(self.hotbar.selected_index());
        if let Err(err) = self.save.set_player_state(saved, self.survival.to_saved()) {
            warn!(error = %format!("{err:#}"), "玩家状态写入存档失败");
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
        if let Err(err) = self.save.ensure_persisted() {
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
        let delta_secs = delta.as_secs_f32();
        // 视角旋转与物理位移解耦：鼠标视角立即生效，位移走固定步长物理。
        let look = self.controller.take_look();
        self.player.rotate_look(look.0, look.1);

        let intent = self.controller.intent();
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
        self.entities.update_ai(
            delta_secs,
            self.player.position(),
            &self.world,
            &self.ai_config,
        );
        self.entities.update_deaths(delta_secs);

        // 摄像机用物理插值系数跟随玩家，避免固定步长与渲染帧率不同步产生抖动。
        let alpha = update.alpha;
        let eye = self.player.interpolated_eye_position(alpha);
        let yaw = self.player.yaw();
        let pitch = self.player.pitch();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.camera_mut().set_pose(eye, yaw, pitch);
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
            if let Err(err) = self.save.ensure_persisted() {
                warn!(error = %format!("{err:#}"), "自然生成记录写入存档失败");
            }
        }

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

        let ui_blocking = self.ui.blocks_world_input();
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

        let slots = self.inventory.hotbar_slots();
        let selected_slot = self.hotbar.selected_index();
        let progress = self.mining.progress();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.hud_mut().update(delta_secs);
            renderer
                .hud_mut()
                .set_health(self.survival.health(), self.survival.max_health());
            renderer.set_selection(if ui_blocking { None } else { selection });
            renderer
                .hud_mut()
                .set(selected_slot, slots, progress, ui_blocking);
            renderer.update_particles(delta_secs);
            self.dropped
                .build_vertices(renderer.drop_vertices_mut(), &self.textures.item_textures);
            let view_proj = renderer.camera().view_proj();
            let camera_position = renderer.camera().position();
            let entity_vertices = renderer.entity_vertices_mut();
            entity_vertices.clear();
            entity_render::build_vertices(
                self.entities.creatures(),
                self.entity_physics.alpha(),
                view_proj,
                camera_position,
                &self.entity_render_config,
                entity_vertices,
            );
            let viewport = renderer.viewport();
            let mut ui_vertices = Vec::new();
            self.ui.build(
                &mut ui_vertices,
                &self.inventory,
                selected_slot,
                &self.textures.item_textures,
                viewport,
                recipe_registry::all(),
            );
            renderer.set_ui_vertices(ui_vertices);
        }

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
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                // 统一 UI 快捷键路由：一次按键最多产生一个界面意图。
                // 死亡界面不响应任何 UI 快捷键，避免误操作绕过死亡流程。
                if let Some(intent) =
                    crate::ui::input::route_key(self.ui.state(), code, pressed, self.cursor_grabbed)
                {
                    self.handle_ui_intent(intent, event_loop);
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
                if let Some(slot) = hotbar_slot_for_key(code) {
                    if pressed && self.hotbar.select_slot(slot) {
                        self.persist_player();
                    }
                    return;
                }
                self.controller.handle_key(code, pressed);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.set_cursor(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
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
                        match self.ui.click(
                            click,
                            state == ElementState::Pressed,
                            &mut self.inventory,
                            viewport,
                            recipe_registry::all(),
                        ) {
                            UiPointerAction::None => {}
                            UiPointerAction::InventoryChanged => self.persist_player(),
                            UiPointerAction::Craft(recipe_id) => self.craft_recipe(recipe_id),
                            UiPointerAction::Respawn => self.respawn(),
                            UiPointerAction::MainMenu => {
                                info!("死亡界面选择返回主菜单");
                                self.flush_save();
                                event_loop.exit();
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
                        if let Some(target) = self.targeted_creature() {
                            self.attack_creature(target);
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
                    (MouseButton::Right, ElementState::Pressed) => self.place_block(),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // 死亡或合成界面打开时滚轮不切换快捷栏；背包内仍可预览选中格。
                match self.ui.state() {
                    UiState::Death | UiState::Crafting => return,
                    UiState::Gameplay if !self.cursor_grabbed => return,
                    _ => {}
                }
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                let before = self.hotbar.selected_index();
                self.hotbar.scroll(amount);
                if before != self.hotbar.selected_index() {
                    self.persist_player();
                }
            }
            WindowEvent::Focused(false) => {
                if self.cursor_grabbed {
                    warn!("窗口失去焦点，释放鼠标捕获");
                    self.release_cursor();
                } else {
                    self.controller.reset();
                }
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
