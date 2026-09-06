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

pub fn item_symbol_image(item: ItemType) -> Image {
    const SIZE: u32 = 32;
    const SAMPLES: u32 = 4;
    let symbol = symbol(item);
    // Crop transparent sides so HUD slots fit the upright silhouettes.
    let width = match item {
        ItemType::Key(_) => SIZE / 2,
        ItemType::MissilePack => 20,
        _ => SIZE,
    };
    let mut data = Vec::with_capacity((width * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in (SIZE - width) / 2..(SIZE + width) / 2 {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let point = Vec2::new((x * SAMPLES + sx) as f32 + 0.5, (y * SAMPLES + sy) as f32 + 0.5)
                        / (SIZE * SAMPLES) as f32;
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
            height: SIZE,
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
