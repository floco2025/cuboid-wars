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
