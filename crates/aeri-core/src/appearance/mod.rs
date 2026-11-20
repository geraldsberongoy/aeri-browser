//! Appearance and customisation (Phase 12; design in
//! `docs/design/phase-12-appearance.md`): colour maths, theme recipes and
//! their files, and the tokens the chrome paints with.

pub mod color;
pub mod harmony;
pub mod recipe;
pub mod tokens;

pub use color::{Oklch, Rgba};
pub use harmony::{Harmony, harmony, shuffle};
pub use recipe::{
    Background, Base, Density, Headings, Hex, Material, Motion, Recipe, Stop, Texture, ThemeError,
    UiSize,
};
pub use tokens::{Accessibility, AppAppearance, Backdrop, Mode, Resolved, Tokens, resolve};

/// The bundled themes (design §3.6), Aeri first. Each is an ordinary
/// recipe, readable from the same file format.
pub fn presets() -> Vec<Recipe> {
    const FILES: [&str; 10] = [
        r##"{"version":1,"name":"Aeri","base":"system","background":{"kind":"default"}}"##,
        r##"{"version":1,"name":"Paper","base":"light","background":{"kind":"solid","color":"#E0CAA8"},"accent":"#784B32","grain":3,"pageRadius":8}"##,
        r##"{"version":1,"name":"Slate","base":"dark","background":{"kind":"solid","color":"#66788D"},"accent":"#6F9AE8","density":"compact","pageRadius":8}"##,
        r##"{"version":1,"name":"Tide","base":"system","background":{"kind":"gradient","angle":120,"stops":[{"at":0,"color":"#2866C7"},{"at":100,"color":"#30AEA2"}]},"accent":"#197F9E","grain":2}"##,
        r##"{"version":1,"name":"Orchard","base":"system","background":{"kind":"gradient","angle":145,"stops":[{"at":0,"color":"#6769C1"},{"at":50,"color":"#8F649E"},{"at":100,"color":"#E4A577"}]},"accent":"#7C55AA","headings":"rounded"}"##,
        r##"{"version":1,"name":"Ember","base":"dark","background":{"kind":"gradient","angle":45,"stops":[{"at":0,"color":"#854F6B"},{"at":100,"color":"#CC693E"}]},"accent":"#D48856","grain":4,"motion":"subtle"}"##,
        r##"{"version":1,"name":"Lichen","base":"system","background":{"kind":"gradient","angle":110,"stops":[{"at":0,"color":"#648870"},{"at":100,"color":"#B1A563"}]},"accent":"#4E803E","pageRadius":12}"##,
        r##"{"version":1,"name":"Frost","base":"light","background":{"kind":"gradient","angle":155,"stops":[{"at":0,"color":"#A9C7E6"},{"at":100,"color":"#D2CAEF"}]},"accent":"#4169BE","density":"spacious","material":"sidebar","opacity":94}"##,
        r##"{"version":1,"name":"Ink","base":"dark","background":{"kind":"solid","color":"#292B36"},"accent":"#A6A9C0","pageRadius":0,"motion":"off"}"##,
        r##"{"version":1,"name":"Dusk","base":"system","background":{"kind":"gradient","angle":70,"stops":[{"at":0,"color":"#404D89"},{"at":50,"color":"#91679E"},{"at":100,"color":"#D18A94"}]},"accent":"#825AB0","grain":5,"pageRadius":12}"##,
    ];
    FILES
        .iter()
        .filter_map(|file| Recipe::from_file(file.as_bytes()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_a_valid_file_and_reads_in_both_appearances() {
        let presets = presets();
        assert_eq!(presets.len(), 10);
        assert_eq!(presets[0], Recipe::aeri());
        for recipe in &presets {
            let again = Recipe::from_file(recipe.to_file().as_bytes()).unwrap();
            assert_eq!(&again, recipe);
            for mode in [Mode::Light, Mode::Dark] {
                let t = resolve(recipe, mode, None, Accessibility::default()).tokens;
                assert!(
                    t.text.contrast(t.sidebar) >= 4.5,
                    "{} {mode:?}",
                    recipe.name
                );
            }
        }
    }
}
