//! 轻量破坏粒子：方块被破坏时生成一小簇短生命周期碎块，并按时清理。
//!
//! 粒子不做世界碰撞，只按重力自由下落，避免与本版本的方块交互逻辑耦合。

use glam::{IVec3, Vec3};

use crate::mesh::Vertex;
use crate::world::{Block, Face};

/// 单个碎块渲染为小立方体，共 6 个面 × 2 个三角形 × 3 个顶点。
const VERTICES_PER_PARTICLE: usize = 36;
/// 同时存在的粒子上限，防止连续破坏导致顶点缓冲无限增长。
const MAX_PARTICLES: usize = 64;
/// 每次破坏生成的碎块数量。
const PARTICLES_PER_BREAK: usize = 14;
/// 粒子重力加速度（格/秒²）。
const PARTICLE_GRAVITY: f32 = 9.0;
/// 单个粒子的基础寿命（秒）。
const MIN_LIFETIME: f32 = 0.45;
const MAX_LIFETIME: f32 = 0.9;

/// 一个碎块。
#[derive(Clone, Copy, Debug)]
struct Particle {
    position: Vec3,
    velocity: Vec3,
    life: f32,
    size: f32,
    color: [f32; 3],
}

/// 破坏粒子系统。
pub struct ParticleSystem {
    particles: Vec<Particle>,
    rng: u32,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
            rng: 0x9E37_79B9,
        }
    }

    /// 当前存活粒子数量。
    #[cfg(test)]
    fn particle_count(&self) -> usize {
        self.particles.len()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// 在方块中心生成一簇碎块。
    pub fn spawn_block(&mut self, block: Block, block_pos: IVec3) {
        let base_color = block.face_color(Face::PosY);
        let center = block_pos.as_vec3() + Vec3::splat(0.5);

        for _ in 0..PARTICLES_PER_BREAK {
            let direction = self.random_direction();
            let speed = 1.2 + self.random() * 2.6;
            let offset = direction * (self.random() * 0.35);
            let brightness = 0.85 + self.random() * 0.3;
            let life = MIN_LIFETIME + self.random() * (MAX_LIFETIME - MIN_LIFETIME);
            let size = 0.06 + self.random() * 0.07;

            self.particles.push(Particle {
                position: center + offset,
                velocity: direction * speed + Vec3::Y * 1.6,
                life,
                size,
                color: (base_color * brightness).to_array(),
            });
        }

        if self.particles.len() > MAX_PARTICLES {
            let excess = self.particles.len() - MAX_PARTICLES;
            self.particles.drain(..excess);
        }
    }

    /// 推进粒子生命周期并清理过期粒子。
    pub fn update(&mut self, delta_secs: f32) {
        let delta = if delta_secs.is_finite() {
            delta_secs.clamp(0.0, 0.25)
        } else {
            0.0
        };
        if delta == 0.0 {
            return;
        }

        for particle in &mut self.particles {
            particle.velocity.y -= PARTICLE_GRAVITY * delta;
            particle.position += particle.velocity * delta;
            particle.life -= delta;
        }
        self.particles
            .retain(|particle| particle.life > 0.0 && particle.position.is_finite());
    }

    /// 生成粒子立方体的非索引顶点，写入 `out`。
    pub fn build_vertices(&self, out: &mut Vec<Vertex>) {
        out.clear();
        out.reserve(self.particles.len() * VERTICES_PER_PARTICLE);
        for particle in &self.particles {
            let min = particle.position - Vec3::splat(particle.size * 0.5);
            for face in Face::ALL {
                let corners = face_corners(face, min, particle.size);
                let color = (Vec3::from_array(particle.color) * face.shade()).to_array();
                for index in [0, 1, 2, 0, 2, 3] {
                    out.push(Vertex {
                        position: corners[index].to_array(),
                        color,
                    });
                }
            }
        }
    }

    fn random(&mut self) -> f32 {
        let mut value = self.rng;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.rng = value;
        value as f32 / u32::MAX as f32
    }

    fn random_direction(&mut self) -> Vec3 {
        let vector = Vec3::new(
            self.random() * 2.0 - 1.0,
            self.random() * 2.0 - 1.0,
            self.random() * 2.0 - 1.0,
        );
        let normalized = vector.normalize_or_zero();
        if normalized == Vec3::ZERO {
            Vec3::Y
        } else {
            normalized
        }
    }
}

/// 小立方体某个面的四个角点，顺序与区块网格一致。
fn face_corners(face: Face, min: Vec3, size: f32) -> [Vec3; 4] {
    let unit = |x: f32, y: f32, z: f32| min + Vec3::new(x, y, z) * size;
    match face {
        Face::PosX => [
            unit(1.0, 0.0, 1.0),
            unit(1.0, 0.0, 0.0),
            unit(1.0, 1.0, 0.0),
            unit(1.0, 1.0, 1.0),
        ],
        Face::NegX => [
            unit(0.0, 0.0, 0.0),
            unit(0.0, 0.0, 1.0),
            unit(0.0, 1.0, 1.0),
            unit(0.0, 1.0, 0.0),
        ],
        Face::PosY => [
            unit(0.0, 1.0, 1.0),
            unit(1.0, 1.0, 1.0),
            unit(1.0, 1.0, 0.0),
            unit(0.0, 1.0, 0.0),
        ],
        Face::NegY => [
            unit(0.0, 0.0, 0.0),
            unit(1.0, 0.0, 0.0),
            unit(1.0, 0.0, 1.0),
            unit(0.0, 0.0, 1.0),
        ],
        Face::PosZ => [
            unit(0.0, 0.0, 1.0),
            unit(1.0, 0.0, 1.0),
            unit(1.0, 1.0, 1.0),
            unit(0.0, 1.0, 1.0),
        ],
        Face::NegZ => [
            unit(1.0, 0.0, 0.0),
            unit(0.0, 0.0, 0.0),
            unit(0.0, 1.0, 0.0),
            unit(1.0, 1.0, 0.0),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawning_creates_particles_and_geometry() {
        let mut particles = ParticleSystem::new();
        particles.spawn_block(Block::Stone, IVec3::new(3, 4, 5));

        assert_eq!(particles.particle_count(), PARTICLES_PER_BREAK);
        let mut vertices = Vec::new();
        particles.build_vertices(&mut vertices);
        assert_eq!(vertices.len(), PARTICLES_PER_BREAK * VERTICES_PER_PARTICLE);
    }

    #[test]
    fn particles_expire_and_are_cleaned_up() {
        let mut particles = ParticleSystem::new();
        particles.spawn_block(Block::Dirt, IVec3::ZERO);
        assert!(!particles.is_empty());

        // 每次最多推进 0.25 秒，多推几轮直到超过最大寿命。
        for _ in 0..8 {
            particles.update(0.25);
        }

        assert!(particles.is_empty(), "过期粒子未被清理");
        let mut vertices = Vec::new();
        particles.build_vertices(&mut vertices);
        assert!(vertices.is_empty());
    }

    #[test]
    fn particle_count_is_capped() {
        let mut particles = ParticleSystem::new();
        for x in 0..10 {
            particles.spawn_block(Block::Grass, IVec3::new(x, 0, 0));
        }
        assert!(particles.particle_count() <= MAX_PARTICLES);
    }

    #[test]
    fn invalid_delta_does_not_advance_lifetime() {
        let mut particles = ParticleSystem::new();
        particles.spawn_block(Block::Grass, IVec3::ZERO);
        let before = particles.particle_count();

        particles.update(f32::NAN);
        particles.update(-1.0);

        assert_eq!(particles.particle_count(), before);
    }
}
