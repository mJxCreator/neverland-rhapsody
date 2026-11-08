//! 骨骼层级：关节父子关系、绑定局部变换、逆绑定矩阵与姿态计算。
//!
//! 关节按导入顺序存储，父子关系通过下标表达；全局矩阵计算会沿父链递归，
//! 不要求关节在数组中按拓扑序排列。

use glam::{Mat4, Vec3};

use super::types::Transform;

/// 单个关节的静态定义。
#[derive(Clone, Debug, PartialEq)]
pub struct Joint {
    pub name: String,
    /// 父关节下标；根关节为 `None`。
    pub parent: Option<usize>,
    /// 绑定姿态下的局部变换。
    pub local: Transform,
}

/// 骨骼定义；同一模型的所有实例共享该定义。
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    pub joints: Vec<Joint>,
    /// 每个关节的逆绑定矩阵，长度与 `joints` 相同。
    pub inverse_bind: Vec<Mat4>,
}

impl Skeleton {
    /// 校验父子下标与矩阵数量；返回可读错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.inverse_bind.len() != self.joints.len() {
            return Err(format!(
                "逆绑定矩阵数量 {} 与关节数量 {} 不一致",
                self.inverse_bind.len(),
                self.joints.len()
            ));
        }
        for (index, joint) in self.joints.iter().enumerate() {
            if let Some(parent) = joint.parent {
                if parent >= self.joints.len() || parent == index {
                    return Err(format!("关节 {index} 的父关节下标 {parent} 非法"));
                }
            }
        }
        // 检测环，避免姿态计算无限递归。
        for index in 0..self.joints.len() {
            let mut cursor = Some(index);
            let mut steps = 0;
            while let Some(current) = cursor {
                if steps > self.joints.len() {
                    return Err(format!("关节 {index} 的父链存在环"));
                }
                cursor = self.joints[current].parent;
                steps += 1;
            }
        }
        Ok(())
    }

    /// 绑定姿态的局部变换列表。
    pub fn bind_locals(&self) -> Vec<Transform> {
        self.joints.iter().map(|joint| joint.local).collect()
    }

    /// 按名称查找关节下标（大小写不敏感）。
    pub fn joint_index(&self, name: &str) -> Option<usize> {
        self.joints
            .iter()
            .position(|joint| joint.name.eq_ignore_ascii_case(name))
    }

    /// 由局部变换计算每个关节的全局矩阵。
    pub fn global_matrices(&self, locals: &[Transform]) -> Vec<Mat4> {
        let mut globals = vec![Mat4::IDENTITY; self.joints.len()];
        let mut resolved = vec![false; self.joints.len()];
        for index in 0..self.joints.len() {
            resolve_global(index, self, locals, &mut globals, &mut resolved);
        }
        globals
    }

    /// 由局部姿态计算蒙皮矩阵：全局变换 × 逆绑定矩阵。
    pub fn skin_matrices(&self, locals: &[Transform]) -> Vec<Mat4> {
        let globals = self.global_matrices(locals);
        globals
            .iter()
            .zip(&self.inverse_bind)
            .map(|(global, inverse)| *global * *inverse)
            .collect()
    }

    /// 绑定姿态下的蒙皮矩阵，应当全部接近单位矩阵，用于自检。
    pub fn bind_skin_matrices(&self) -> Vec<Mat4> {
        self.skin_matrices(&self.bind_locals())
    }
}

/// 递归解析关节全局矩阵，并缓存已完成节点。
fn resolve_global(
    index: usize,
    skeleton: &Skeleton,
    locals: &[Transform],
    globals: &mut [Mat4],
    resolved: &mut [bool],
) {
    if resolved[index] {
        return;
    }
    let local = locals
        .get(index)
        .copied()
        .unwrap_or(Transform::IDENTITY)
        .to_matrix();
    let matrix = match skeleton.joints[index].parent {
        Some(parent) => {
            resolve_global(parent, skeleton, locals, globals, resolved);
            globals[parent] * local
        }
        None => local,
    };
    globals[index] = matrix;
    resolved[index] = true;
}

/// 一次求得所有关节姿态：局部变换、全局矩阵与蒙皮矩阵。
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub locals: Vec<Transform>,
    pub globals: Vec<Mat4>,
    pub skin: Vec<Mat4>,
}

impl Pose {
    /// 绑定姿态。
    pub fn bind(skeleton: &Skeleton) -> Self {
        Self::from_locals(skeleton, &skeleton.bind_locals())
    }

    /// 由局部变换生成完整姿态。
    pub fn from_locals(skeleton: &Skeleton, locals: &[Transform]) -> Self {
        let globals = skeleton.global_matrices(locals);
        let skin = globals
            .iter()
            .zip(&skeleton.inverse_bind)
            .map(|(global, inverse)| *global * *inverse)
            .collect();
        Self {
            locals: locals.to_vec(),
            globals,
            skin,
        }
    }

    /// 关节数量。
    pub fn len(&self) -> usize {
        self.locals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.locals.is_empty()
    }

    /// 混合两个姿态的局部变换后重新计算全局与蒙皮矩阵。
    pub fn blend(from: &Pose, to: &Pose, skeleton: &Skeleton, weight: f32) -> Self {
        let locals: Vec<Transform> = from
            .locals
            .iter()
            .zip(&to.locals)
            .map(|(a, b)| Transform::lerp(*a, *b, weight))
            .collect();
        let _ = skeleton;
        Self::from_locals(skeleton, &locals)
    }
}

/// 关节局部变换对应的位移（调试与测试使用）。
pub fn joint_position(pose: &Pose, index: usize) -> Vec3 {
    pose.globals
        .get(index)
        .map(|matrix| matrix.w_axis.truncate())
        .unwrap_or(Vec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    fn skeleton() -> Skeleton {
        let joints = vec![
            Joint {
                name: "root".to_string(),
                parent: None,
                local: Transform::IDENTITY,
            },
            Joint {
                name: "body".to_string(),
                parent: Some(0),
                local: Transform::new(Vec3::new(0.0, 0.5, 0.0), Quat::IDENTITY, Vec3::ONE),
            },
            Joint {
                name: "head".to_string(),
                parent: Some(1),
                local: Transform::new(Vec3::new(0.0, 0.0, 0.5), Quat::IDENTITY, Vec3::ONE),
            },
        ];
        let globals = {
            let bind = Skeleton {
                joints: joints.clone(),
                inverse_bind: Vec::new(),
            };
            bind.global_matrices(&bind.bind_locals())
        };
        Skeleton {
            joints,
            inverse_bind: globals.iter().map(|matrix| matrix.inverse()).collect(),
        }
    }

    #[test]
    fn hierarchy_accumulates_parent_transforms() {
        let skeleton = skeleton();
        assert!(skeleton.validate().is_ok());
        let pose = Pose::bind(&skeleton);
        assert!((joint_position(&pose, 1) - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-5);
        assert!((joint_position(&pose, 2) - Vec3::new(0.0, 0.5, 0.5)).length() < 1e-5);
    }

    #[test]
    fn bind_pose_skinning_is_identity() {
        let skeleton = skeleton();
        for matrix in skeleton.bind_skin_matrices() {
            let expected = Mat4::IDENTITY.to_cols_array();
            let actual = matrix.to_cols_array();
            for (a, b) in actual.iter().zip(expected.iter()) {
                assert!((a - b).abs() < 1e-4, "绑定姿态蒙皮矩阵应接近单位矩阵");
            }
        }
    }

    #[test]
    fn invalid_parents_are_rejected() {
        let mut skeleton = skeleton();
        skeleton.joints[1].parent = Some(9);
        assert!(skeleton.validate().is_err());
        skeleton.joints[1].parent = Some(1);
        assert!(skeleton.validate().is_err());
    }

    #[test]
    fn pose_blend_moves_between_endpoints() {
        let skeleton = skeleton();
        let bind = Pose::bind(&skeleton);
        let mut locals = skeleton.bind_locals();
        locals[2].translation = Vec3::new(0.0, 0.0, 2.0);
        let moved = Pose::from_locals(&skeleton, &locals);
        let blended = Pose::blend(&bind, &moved, &skeleton, 0.5);
        let head = joint_position(&blended, 2);
        assert!(head.z > 0.5 && head.z < 2.0);
    }
}
