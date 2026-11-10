use super::*;

fn point(x: u8, y: u8) -> CanvasPoint {
    CanvasPoint::new(x, y).unwrap_or_else(|| unreachable!())
}

fn color(index: u8) -> PaletteIndex {
    PaletteIndex::try_from(index).unwrap_or_else(|_| unreachable!())
}

const BLANK_COLORS: usize = 8;

fn blank() -> KraddieDesign {
    let mut design = KraddieDesign::kraddie(BodyStyle::default());
    design.palette = (0..BLANK_COLORS)
        .map(|index| KraddieColor::rgb(u32::try_from(index).unwrap_or_default()))
        .collect();
    design.layers = vec![KraddieLayer::new("Base"), KraddieLayer::new("Top")];
    for mood in KraddieMood::ALL {
        design.moods.get_mut(mood).expression.clear();
    }
    design
}

#[test]
fn every_style_of_kraddie_is_a_valid_design_with_five_faces() {
    for style in BodyStyle::ALL {
        let design = KraddieDesign::kraddie(style);
        assert_eq!(design.validate(), Ok(()), "{style:?}");
        assert_eq!(design.style, style);
        let faces: Vec<&PixelGrid> = KraddieMood::ALL
            .iter()
            .map(|mood| &design.moods.get(*mood).expression)
            .collect();
        for (number, face) in faces.iter().enumerate() {
            assert!(face.painted().count() > 0, "{style:?} {number}");
            for other in &faces[number + 1..] {
                assert_ne!(face, other, "{style:?} repeats a face");
            }
        }
        let body = design
            .layers
            .iter()
            .map(|layer| layer.pixels.painted().count());
        assert!(body.sum::<usize>() > 0, "{style:?} has no body");
    }
}

#[test]
fn a_design_survives_json_and_rejects_foreign_pixels() {
    let mut design = blank();
    design.paint(
        PaintTarget::Layer(0),
        PaintTool::Brush,
        point(3, 3),
        point(3, 3),
        color(5),
    );
    let line = serde_json::to_string(&design).unwrap_or_else(|_| unreachable!());
    let back: KraddieDesign = serde_json::from_str(&line).unwrap_or_else(|_| unreachable!());
    assert_eq!(back, design);
    let mut shrunk = design;
    shrunk.palette.truncate(5);
    assert_eq!(shrunk.validate(), Err(DesignError::ColorOutsidePalette));
    let short_row = line.replacen("\"........................\"", "\"...\"", 1);
    assert!(serde_json::from_str::<KraddieDesign>(&short_row).is_err());
    let bad_mark = line.replacen(
        "\"........................\"",
        "\".......................z\"",
        1,
    );
    assert!(serde_json::from_str::<KraddieDesign>(&bad_mark).is_err());
}

#[test]
fn faces_paint_over_shown_layers_in_order() {
    let mut design = blank();
    let top = design.add_layer().unwrap_or_else(|| unreachable!());
    assert!(design.paint(
        PaintTarget::Layer(0),
        PaintTool::Brush,
        point(1, 1),
        point(1, 1),
        color(1)
    ));
    assert!(design.paint(
        PaintTarget::Layer(top),
        PaintTool::Brush,
        point(1, 1),
        point(1, 1),
        color(2)
    ));
    assert_eq!(
        design.pixel(point(1, 1), KraddieMood::Idle),
        Some(design.palette[2])
    );
    assert!(design.toggle_layer(top));
    assert_eq!(
        design.pixel(point(1, 1), KraddieMood::Idle),
        Some(design.palette[1])
    );
    let face = PaintTarget::Expression(KraddieMood::Error);
    assert!(design.paint(face, PaintTool::Brush, point(1, 1), point(1, 1), color(6)));
    assert_eq!(
        design.pixel(point(1, 1), KraddieMood::Error),
        Some(design.palette[6])
    );
    assert_eq!(
        design.pixel(point(1, 1), KraddieMood::Idle),
        Some(design.palette[1])
    );
    assert!(!design.paint(face, PaintTool::Brush, point(1, 1), point(1, 1), color(15)));
}

#[test]
fn layers_and_palette_stay_within_their_bounds() {
    let mut design = blank();
    while design.add_layer().is_some() {}
    assert_eq!(design.layers.len(), MAX_LAYERS);
    while design.remove_layer(0) {}
    assert_eq!(design.layers.len(), 1);
    while design.add_color(KraddieColor::rgb(0x0012_3456)).is_some() {}
    assert_eq!(design.palette.len(), MAX_PALETTE);
    assert!(design.set_color(color(0), KraddieColor::rgb(0x00ff_0000)));
    assert_eq!(design.palette[0].to_string(), "#ff0000");
    assert_eq!(design.validate(), Ok(()));
    assert_eq!("#12345".parse::<KraddieColor>(), Err(KraddieColorError));
    assert_eq!(
        "#0a0B0c"
            .parse::<KraddieColor>()
            .map(KraddieColor::channels),
        Ok([10, 11, 12])
    );
}

#[test]
fn undo_and_redo_walk_back_and_forth_through_strokes() {
    let mut design = blank();
    let original = design.clone();
    let mut history = DesignHistory::default();
    assert!(!history.undo(&mut design));
    history.record(&design);
    design.paint(
        PaintTarget::Layer(1),
        PaintTool::Line,
        point(0, 0),
        point(23, 23),
        color(5),
    );
    let stroked = design.clone();
    history.record(&design);
    design.paint(
        PaintTarget::Layer(1),
        PaintTool::Fill,
        point(0, 23),
        point(0, 23),
        color(4),
    );
    assert!(history.undo(&mut design));
    assert_eq!(design, stroked);
    assert!(history.undo(&mut design));
    assert_eq!(design, original);
    assert!(!history.can_undo());
    assert!(history.redo(&mut design));
    assert_eq!(design, stroked);
    history.record(&design);
    assert!(!history.can_redo());
    for _ in 0..MAX_UNDO + 10 {
        let before = design.clone();
        design.add_color(KraddieColor::rgb(0));
        design.palette.truncate(MAX_PALETTE - 1);
        design.layers[0].shown = design.layers[0].shown.toggled();
        history.record(&before);
    }
    let mut undone = 0;
    while history.undo(&mut design) {
        undone += 1;
    }
    assert_eq!(undone, MAX_UNDO);
}
