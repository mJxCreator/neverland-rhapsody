//! UI 绘制基础：统一的顶点格式与四边形/图标/边框图元。
//!
//! 所有界面共享同一种顶点格式与同一条动态顶点缓冲，由渲染器一次性提交，
//! 因此不同 UI 之间不会切换管线或创建新的 GPU 资源。
//! `uv` 为哨兵值 [`SOLID_UV`] 时按纯色绘制；否则由 `atlas` 选择物品图集或 UI 图集。

use bytemuck::{Pod, Zeroable};

use crate::item::ItemId;
use crate::texture::types::UVRect;
use crate::texture::ui_texture::{UiDrawMode, UiTextureView};
use crate::texture::ItemTextureMap;

use super::layout::{valid_viewport, UiRect};

/// UV 哨兵：负值表示不采样纹理，只使用颜色。
pub const SOLID_UV: [f32; 2] = [-1.0, -1.0];
/// 顶点使用物品图标图集。
pub const ATLAS_ITEM: f32 = 0.0;
/// 顶点使用 UI 控件图集。
pub const ATLAS_UI: f32 = 1.0;

/// 一个 UI 顶点：裁剪空间坐标、图集 UV、图集来源与带透明度的颜色。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    /// 0 = 物品图集，1 = UI 图集；纯色顶点忽略该值。
    pub atlas: f32,
}

/// 纯色绘制矩形。
pub fn push_quad(out: &mut Vec<UiVertex>, viewport: (f32, f32), rect: UiRect, color: [f32; 4]) {
    push_quad_uv(out, viewport, rect, [SOLID_UV; 4], color, ATLAS_ITEM);
}

/// 绘制一张物品图集中的图标；没有图标时不产生几何。
pub fn push_item_icon(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    item: ItemId,
    icons: &ItemTextureMap,
) {
    let Some(icon) = icons.icon(item) else {
        return;
    };
    push_textured_quad(out, viewport, rect, icon.uv, [1.0, 1.0, 1.0, 1.0]);
}

/// 按物品图集 UV 子区域采样绘制矩形；`uv` 为 `min` 左上、`max` 右下。
pub fn push_textured_quad(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    uv: UVRect,
    color: [f32; 4],
) {
    push_atlas_quad(out, viewport, rect, uv, color, ATLAS_ITEM);
}

/// 绘制 UI 图集控件：整图拉伸或按九宫格切分。
pub fn push_ui_view(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    view: UiTextureView,
    color: [f32; 4],
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    match view.draw {
        UiDrawMode::Stretch => push_atlas_quad(out, viewport, rect, view.uv, color, ATLAS_UI),
        UiDrawMode::NineSlice => push_nine_slice(out, viewport, rect, view, color),
    }
}

/// 把九宫格源图按切边映射到目标矩形，四角保持像素尺寸，边与中心拉伸。
fn push_nine_slice(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    view: UiTextureView,
    color: [f32; 4],
) {
    let [left, top, right, bottom] = view.slice;
    let source_width = view.source_size[0];
    let source_height = view.source_size[1];
    if source_width == 0
        || source_height == 0
        || left + right >= source_width
        || top + bottom >= source_height
    {
        push_atlas_quad(out, viewport, rect, view.uv, color, ATLAS_UI);
        return;
    }

    let scale_x = if (left + right) as f32 >= rect.width {
        rect.width / (left + right) as f32
    } else {
        1.0
    };
    let scale_y = if (top + bottom) as f32 >= rect.height {
        rect.height / (top + bottom) as f32
    } else {
        1.0
    };
    let left_dest = left as f32 * scale_x;
    let right_dest = right as f32 * scale_x;
    let top_dest = top as f32 * scale_y;
    let bottom_dest = bottom as f32 * scale_y;
    let mid_width = (rect.width - left_dest - right_dest).max(0.0);
    let mid_height = (rect.height - top_dest - bottom_dest).max(0.0);

    let left_u = view.uv.min[0] + view.uv.width() * (left as f32 / source_width as f32);
    let right_u = view.uv.max[0] - view.uv.width() * (right as f32 / source_width as f32);
    let top_v = view.uv.min[1] + view.uv.height() * (top as f32 / source_height as f32);
    let bottom_v = view.uv.max[1] - view.uv.height() * (bottom as f32 / source_height as f32);

    let columns = [
        (rect.x, left_dest, view.uv.min[0], left_u),
        (rect.x + left_dest, mid_width, left_u, right_u),
        (
            rect.x + left_dest + mid_width,
            right_dest,
            right_u,
            view.uv.max[0],
        ),
    ];
    let rows = [
        (rect.y, top_dest, view.uv.min[1], top_v),
        (rect.y + top_dest, mid_height, top_v, bottom_v),
        (
            rect.y + top_dest + mid_height,
            bottom_dest,
            bottom_v,
            view.uv.max[1],
        ),
    ];
    for (y, height, v0, v1) in rows {
        for (x, width, u0, u1) in columns {
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            push_atlas_quad(
                out,
                viewport,
                UiRect::new(x, y, width, height),
                UVRect {
                    min: [u0, v0],
                    max: [u1, v1],
                },
                color,
                ATLAS_UI,
            );
        }
    }
}

fn push_atlas_quad(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    uv: UVRect,
    color: [f32; 4],
    atlas: f32,
) {
    if uv.max[0] <= uv.min[0] || uv.max[1] <= uv.min[1] {
        return;
    }
    push_quad_uv(
        out,
        viewport,
        rect,
        [
            [uv.min[0], uv.min[1]],
            [uv.min[0], uv.max[1]],
            [uv.max[0], uv.max[1]],
            [uv.max[0], uv.min[1]],
        ],
        color,
        atlas,
    );
}

/// 绘制四条边组成的边框，中心保持透明。
pub fn push_border(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    thickness: f32,
    color: [f32; 4],
) {
    if thickness <= 0.0 || rect.width < thickness * 2.0 || rect.height < thickness * 2.0 {
        return;
    }
    push_quad(
        out,
        viewport,
        UiRect::new(rect.x, rect.y, rect.width, thickness),
        color,
    );
    push_quad(
        out,
        viewport,
        UiRect::new(
            rect.x,
            rect.y + rect.height - thickness,
            rect.width,
            thickness,
        ),
        color,
    );
    push_quad(
        out,
        viewport,
        UiRect::new(
            rect.x,
            rect.y + thickness,
            thickness,
            rect.height - thickness * 2.0,
        ),
        color,
    );
    push_quad(
        out,
        viewport,
        UiRect::new(
            rect.x + rect.width - thickness,
            rect.y + thickness,
            thickness,
            rect.height - thickness * 2.0,
        ),
        color,
    );
}

/// 顶点顺序：左上、左下、右下、左上、右下、右上。
fn push_quad_uv(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    uvs: [[f32; 2]; 4],
    color: [f32; 4],
    atlas: f32,
) {
    let (width, height) = viewport;
    if !valid_viewport(width, height) || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    let x0 = rect.x / width * 2.0 - 1.0;
    let x1 = (rect.x + rect.width) / width * 2.0 - 1.0;
    let y0 = 1.0 - rect.y / height * 2.0;
    let y1 = 1.0 - (rect.y + rect.height) / height * 2.0;

    for (px, py, uv) in [
        (x0, y0, uvs[0]),
        (x0, y1, uvs[1]),
        (x1, y1, uvs[2]),
        (x0, y0, uvs[0]),
        (x1, y1, uvs[2]),
        (x1, y0, uvs[3]),
    ] {
        out.push(UiVertex {
            position: [px, py],
            uv,
            color,
            atlas,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_quad_produces_two_triangles() {
        let mut vertices = Vec::new();
        push_quad(
            &mut vertices,
            (1280.0, 720.0),
            UiRect::new(10.0, 20.0, 30.0, 40.0),
            [1.0, 0.0, 0.0, 1.0],
        );
        assert_eq!(vertices.len(), 6);
        assert!(vertices.iter().all(|vertex| vertex.uv == SOLID_UV));
        for vertex in &vertices {
            assert!((-1.0..=1.0).contains(&vertex.position[0]));
            assert!((-1.0..=1.0).contains(&vertex.position[1]));
        }
    }

    #[test]
    fn ui_texture_uses_ui_atlas_and_nine_slice_keeps_corners() {
        let view = UiTextureView {
            texture: crate::texture::types::TextureId::new(0),
            uv: UVRect {
                min: [0.0, 0.0],
                max: [1.0, 1.0],
            },
            source_size: [64, 64],
            draw: UiDrawMode::NineSlice,
            slice: [8, 8, 8, 8],
            transparency: crate::texture::types::Transparency::Alpha,
            placeholder: false,
        };

        let mut vertices = Vec::new();
        push_ui_view(
            &mut vertices,
            (1280.0, 720.0),
            UiRect::new(100.0, 100.0, 200.0, 120.0),
            view,
            [1.0; 4],
        );
        assert_eq!(vertices.len(), 9 * 6, "九宫格应产生九个四边形");
        assert!(vertices.iter().all(|vertex| vertex.atlas == ATLAS_UI));
        // 左上角的 UV 应落在源图左上切边内。
        assert!(vertices[0].uv[0] < 0.25 && vertices[0].uv[1] < 0.25);

        let stretched = UiTextureView {
            draw: UiDrawMode::Stretch,
            ..view
        };
        let mut stretched_vertices = Vec::new();
        push_ui_view(
            &mut stretched_vertices,
            (1280.0, 720.0),
            UiRect::new(0.0, 0.0, 100.0, 50.0),
            stretched,
            [1.0; 4],
        );
        assert_eq!(stretched_vertices.len(), 6);
        assert_eq!(stretched_vertices[0].uv, [0.0, 0.0]);
    }

    #[test]
    fn invalid_rect_and_viewport_produce_no_geometry() {
        let mut vertices = Vec::new();
        push_quad(
            &mut vertices,
            (1280.0, 720.0),
            UiRect::new(0.0, 0.0, 0.0, 10.0),
            [1.0; 4],
        );
        push_quad(
            &mut vertices,
            (0.0, 720.0),
            UiRect::new(0.0, 0.0, 10.0, 10.0),
            [1.0; 4],
        );
        assert!(vertices.is_empty());
    }

    #[test]
    fn border_draws_four_sides_without_center() {
        let mut vertices = Vec::new();
        push_border(
            &mut vertices,
            (1280.0, 720.0),
            UiRect::new(10.0, 10.0, 100.0, 80.0),
            2.0,
            [1.0; 4],
        );
        assert_eq!(vertices.len(), 24);
    }
}
