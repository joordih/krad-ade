use krad_domain::{KraddieColor, KraddieMood, PaintTarget};

pub(super) const SWATCHES: [KraddieColor; 24] = [
    KraddieColor::rgb(0x001d_1b2e),
    KraddieColor::rgb(0x0046_4366),
    KraddieColor::rgb(0x0089_86a8),
    KraddieColor::rgb(0x00f4_f4f8),
    KraddieColor::rgb(0x00ff_8fab),
    KraddieColor::rgb(0x00ef_476f),
    KraddieColor::rgb(0x00c1_121f),
    KraddieColor::rgb(0x00ff_9f1c),
    KraddieColor::rgb(0x00ff_d166),
    KraddieColor::rgb(0x00ff_f3b0),
    KraddieColor::rgb(0x0006_d6a0),
    KraddieColor::rgb(0x0031_a37c),
    KraddieColor::rgb(0x0055_d6a8),
    KraddieColor::rgb(0x0011_8ab2),
    KraddieColor::rgb(0x004c_c9f0),
    KraddieColor::rgb(0x0079_b8ff),
    KraddieColor::rgb(0x004d_86d1),
    KraddieColor::rgb(0x0097_6bff),
    KraddieColor::rgb(0x006a_44d6),
    KraddieColor::rgb(0x00e6_e1f5),
    KraddieColor::rgb(0x00b8_b0d4),
    KraddieColor::rgb(0x008d_5524),
    KraddieColor::rgb(0x00c6_8642),
    KraddieColor::rgb(0x00ff_ffff),
];

const LAYER_TARGET: &str = "Layer";
const FACE_TARGET: &str = "Face";

pub(super) fn target_label(target: PaintTarget, layer_name: Option<&str>) -> String {
    match target {
        PaintTarget::Layer(index) => layer_name.map_or_else(
            || format!("{LAYER_TARGET} {}", index + 1),
            ToOwned::to_owned,
        ),
        PaintTarget::Expression(mood) => format!("{FACE_TARGET}: {}", mood.label()),
    }
}

pub(super) const fn face_targets() -> [PaintTarget; 5] {
    [
        PaintTarget::Expression(KraddieMood::Idle),
        PaintTarget::Expression(KraddieMood::Working),
        PaintTarget::Expression(KraddieMood::Waiting),
        PaintTarget::Expression(KraddieMood::Done),
        PaintTarget::Expression(KraddieMood::Error),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_name_their_layer_or_mood() {
        assert_eq!(target_label(PaintTarget::Layer(0), Some("Body")), "Body");
        assert_eq!(target_label(PaintTarget::Layer(2), None), "Layer 3");
        assert_eq!(target_label(face_targets()[4], None), "Face: Error");
        let unique: std::collections::HashSet<KraddieColor> = SWATCHES.into_iter().collect();
        assert_eq!(unique.len(), SWATCHES.len());
    }
}
