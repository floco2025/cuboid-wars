use std::{collections::HashMap, sync::LazyLock};

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use common::protocol::ItemType;
use serde::Deserialize;

#[derive(Deserialize)]
struct ItemSymbol {
    #[serde(default)]
    polygons: Vec<Vec<[f32; 2]>>,
    #[serde(default)]
    circles: Vec<SymbolCircle>,
}

#[derive(Deserialize)]
struct SymbolCircle {
    center: [f32; 2],
    radius: f32,
}

// Counter-clockwise convex pieces in a unit square, Y-up; the editor reads the same outlines.
static SYMBOLS: LazyLock<HashMap<String, ItemSymbol>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../assets/symbols/items.json"))
        .expect("item symbol outlines are invalid JSON")
});

fn symbol(item: ItemType) -> &'static ItemSymbol {
    SYMBOLS
        .get(item.config_id())
        .expect("item silhouette missing from symbol outlines")
}

pub fn item_symbol_mesh(item: ItemType, size: f32, depth: f32) -> Mesh {
    let symbol = symbol(item);
    let polygons = symbol.polygons.iter().map(|points| {
        let polygon = ConvexPolygon::new(points.iter().map(|&point| Vec2::from_array(point) * size))
            .expect("item symbol piece is not convex");
        Extrusion::new(polygon, depth).mesh().build()
    });
    let spheres = symbol.circles.iter().map(|circle| {
        let [x, y] = circle.center;
        Sphere::new(circle.radius * size)
            .mesh()
            .uv(24, 16)
            .translated_by(Vec3::new(x * size, y * size, 0.0))
    });
    let mut pieces = polygons.chain(spheres);
    let mut mesh = pieces.next().expect("item symbol has no pieces");
    for piece in pieces {
        mesh.merge(&piece)
            .expect("item symbol mesh attributes are incompatible");
    }
    mesh
}

const IMAGE_SIZE: u32 = 32;

// The silhouette on a square image, for square HUD slots.
pub fn item_symbol_image(item: ItemType) -> Image {
    symbol_image(symbol(item), IMAGE_SIZE)
}

// The silhouette cropped to its outline's width, for HUD slots that take
// their width from the image's aspect.
pub fn item_symbol_image_cropped(item: ItemType) -> Image {
    let symbol = symbol(item);
    symbol_image(symbol, outline_width_px(symbol))
}

// Pixel width of the outline's reach either side of the centre, rounded outward.
fn outline_width_px(symbol: &ItemSymbol) -> u32 {
    let half_width = symbol
        .polygons
        .iter()
        .flatten()
        .map(|[x, _]| x.abs())
        .chain(
            symbol
                .circles
                .iter()
                .map(|circle| circle.center[0].abs() + circle.radius),
        )
        .fold(0.0_f32, f32::max);
    ((2.0 * half_width * IMAGE_SIZE as f32).ceil() as u32).min(IMAGE_SIZE)
}

fn symbol_image(symbol: &ItemSymbol, width: u32) -> Image {
    const SAMPLES: u32 = 4;
    let mut data = Vec::with_capacity((width * IMAGE_SIZE * 4) as usize);
    for y in 0..IMAGE_SIZE {
        for x in (IMAGE_SIZE - width) / 2..(IMAGE_SIZE + width) / 2 {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let point = Vec2::new((x * SAMPLES + sx) as f32 + 0.5, (y * SAMPLES + sy) as f32 + 0.5)
                        / (IMAGE_SIZE * SAMPLES) as f32;
                    let point = Vec2::new(point.x - 0.5, 0.5 - point.y);
                    if symbol.polygons.iter().any(|polygon| contains(polygon, point))
                        || symbol.circles.iter().any(|circle| {
                            point.distance_squared(Vec2::from_array(circle.center)) <= circle.radius * circle.radius
                        })
                    {
                        hits += 1;
                    }
                }
            }
            data.extend([255, 255, 255, (hits * 255 / (SAMPLES * SAMPLES)) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width,
            height: IMAGE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn contains(polygon: &[[f32; 2]], point: Vec2) -> bool {
    polygon.iter().zip(polygon.iter().cycle().skip(1)).all(|(&a, &b)| {
        let a = Vec2::from_array(a);
        let b = Vec2::from_array(b);
        (b - a).perp_dot(point - a) >= 0.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cropped_width_covers_the_widest_polygon_or_circle() {
        let symbol = ItemSymbol {
            polygons: vec![vec![[-0.1, 0.0], [0.2, 0.0], [0.0, 0.3]]],
            circles: vec![SymbolCircle {
                center: [0.1, 0.0],
                radius: 0.15,
            }],
        };
        assert_eq!(outline_width_px(&symbol), 16);
        let wide = ItemSymbol {
            polygons: vec![vec![[-0.5, 0.0], [0.5, 0.0], [0.0, 0.5]]],
            circles: Vec::new(),
        };
        assert_eq!(outline_width_px(&wide), IMAGE_SIZE);
    }
}
