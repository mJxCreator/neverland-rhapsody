//! 装备挂载系统：挂载点定义、骨骼变换求解与运行时挂载列表。
//!
//! 挂载点只描述“挂在哪个骨骼、相对骨骼的局部变换”，不保存任何装备属性。
//! 装备是否有效、当前穿什么，全部由 `equipment` 与 `inventory` 决定。

pub mod attachment_manager;
pub mod attachment_point;
pub mod attachment_transform;

pub use self::attachment_manager::AttachmentManager;
pub use self::attachment_point::{AttachmentId, PLAYER_BONES};
