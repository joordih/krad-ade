use gpui_kit::App;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSFontDescriptorSymbolicTraits, NSFontManager, NSFontTraitMask};

use super::FamilyKind;

const PROBE_WEIGHT: isize = 5;
const PROBE_SIZE: f64 = 13.0;

fn kind_of(traits: NSFontDescriptorSymbolicTraits) -> FamilyKind {
    let class = traits & NSFontDescriptorSymbolicTraits::ClassMask;
    let decorative = [
        NSFontDescriptorSymbolicTraits::ClassScripts,
        NSFontDescriptorSymbolicTraits::ClassOrnamentals,
        NSFontDescriptorSymbolicTraits::ClassSymbolic,
    ];
    if decorative.contains(&class) {
        FamilyKind::Decorative
    } else if traits.contains(NSFontDescriptorSymbolicTraits::TraitMonoSpace) {
        FamilyKind::Monospace
    } else {
        FamilyKind::Text
    }
}

pub(super) fn system_families(_: &App) -> Vec<(String, FamilyKind)> {
    let Some(main_thread) = MainThreadMarker::new() else {
        return Vec::new();
    };
    let manager = NSFontManager::sharedFontManager(main_thread);
    manager
        .availableFontFamilies()
        .iter()
        .filter_map(|family| {
            let font = manager.fontWithFamily_traits_weight_size(
                &family,
                NSFontTraitMask::empty(),
                PROBE_WEIGHT,
                PROBE_SIZE,
            )?;
            let traits = font.fontDescriptor().symbolicTraits();
            Some((family.to_string(), kind_of(traits)))
        })
        .collect()
}
