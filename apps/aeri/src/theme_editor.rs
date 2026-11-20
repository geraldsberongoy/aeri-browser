//! The theme editor: a small native GPUI window in which the Person designs
//! a theme (Phase 12). Every valid change is previewed live through the
//! shell; nothing is kept until Save. Like the space editor it is never a
//! web page, and it is a window of its own because the frame window hands
//! the keyboard to the page.

mod view;
mod widgets;

use std::time::{SystemTime, UNIX_EPOCH};

use aeri_core::appearance::{
    Accessibility, Background, Harmony, Hex, Mode, Recipe, Rgba, Stop, Texture, harmony, resolve,
    shuffle,
};
use futures::channel::mpsc::UnboundedSender;
use gpui::{AppContext, Context, Entity, FocusHandle, Focusable, Window};

use crate::shell::{AppMsg, ThemeEditorMsg};
use crate::text_input::{InputEvent, TextInput};

/// Preset colours for stops and the accent: distinct hues, then neutrals.
const PALETTE: [u32; 18] = [
    0xE5484D, 0xF76B15, 0xF5B800, 0x8DB63C, 0x30A46C, 0x12A594, 0x00A2C7, 0x3E63DD, 0x6769C1,
    0x8E4EC6, 0xD6409F, 0xE93D82, 0x784B32, 0x66788D, 0x292B36, 0xE0CAA8, 0xF2F0EB, 0x9A9AA3,
];
const DEFAULT_COLOUR: u32 = 0x6769C1;
const DEFAULT_ANGLE: u16 = 135;
const MAX_STOPS: usize = 5;
const TEXTURES: [(&str, Texture); 4] = [
    ("Grain", Texture::Grain),
    ("Sand", Texture::Sand),
    ("Denim", Texture::Denim),
    ("Tweed", Texture::Tweed),
];

pub struct ThemeEditor {
    scope: String,
    parent: Option<String>,
    recipe: Recipe,
    tx: UnboundedSender<AppMsg>,
    name: Entity<TextInput>,
    stop_hex: Entity<TextInput>,
    accent_hex: Entity<TextInput>,
    /// The stop the hex field and palette edit.
    selected: usize,
    focus: FocusHandle,
}

/// `#RRGGBB` (the `#` optional) as an opaque colour.
fn parse_hex(text: &str) -> Option<Rgba> {
    let text = text.trim();
    let text = if text.starts_with('#') {
        text.to_owned()
    } else {
        format!("#{text}")
    };
    Rgba::from_hex(&text).map(|colour| colour.with_alpha(1.))
}

fn spaced(colours: &[Rgba]) -> Vec<Stop> {
    let last = colours.len().saturating_sub(1).max(1);
    colours
        .iter()
        .enumerate()
        .map(|(ix, colour)| Stop {
            at: (ix * 100).div_ceil(last).min(100) as u8,
            color: Hex(*colour),
        })
        .collect()
}

impl ThemeEditor {
    pub fn new(
        scope: String,
        parent: Option<String>,
        recipe: Recipe,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let closed = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            let _ = closed.unbounded_send(AppMsg::ThemeEditor(ThemeEditorMsg::Cancel));
            true
        });
        let name = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx);
            input.set_text(recipe.name.clone(), cx);
            input.select_all_text(cx);
            input
        });
        let stop_hex = cx.new(|cx| TextInput::new("#RRGGBB", cx));
        let accent_hex = cx.new(|cx| TextInput::new("#RRGGBB", cx));
        cx.subscribe_in(
            &name,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Edited(text) => {
                    let text = text.trim();
                    let name = if text.is_empty() { "Custom" } else { text }.to_owned();
                    this.edit(cx, |r| r.name = name);
                }
                InputEvent::Cancelled => this.cancel(window),
                _ => {}
            },
        )
        .detach();
        cx.subscribe_in(
            &stop_hex,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Edited(text) | InputEvent::Submitted(text) => {
                    if let Some(colour) = parse_hex(text) {
                        this.set_selected_colour(colour, cx);
                    }
                }
                InputEvent::Cancelled => this.cancel(window),
                _ => {}
            },
        )
        .detach();
        cx.subscribe_in(
            &accent_hex,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Edited(text) | InputEvent::Submitted(text) => {
                    if let Some(colour) = parse_hex(text) {
                        this.edit(cx, |r| r.accent = Some(Hex(colour)));
                    }
                }
                InputEvent::Cancelled => this.cancel(window),
                _ => {}
            },
        )
        .detach();
        window.focus(&name.focus_handle(cx), cx);
        let mut editor = Self {
            scope,
            parent,
            recipe,
            tx,
            name,
            stop_hex,
            accent_hex,
            selected: 0,
            focus: cx.focus_handle(),
        };
        editor.sync_inputs(cx);
        editor
    }

    fn send(&self, msg: ThemeEditorMsg) {
        let _ = self.tx.unbounded_send(AppMsg::ThemeEditor(msg));
    }

    fn cancel(&self, window: &mut Window) {
        self.send(ThemeEditorMsg::Cancel);
        window.remove_window();
    }

    fn save(&self, window: &mut Window) {
        if self.recipe.validate().is_ok() {
            self.send(ThemeEditorMsg::Save(self.recipe.clone()));
            window.remove_window();
        }
    }

    fn inherit(&self, window: &mut Window) {
        self.send(ThemeEditorMsg::Inherit);
        window.remove_window();
    }

    /// Changes the draft; a valid draft is previewed.
    fn edit(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Recipe)) {
        change(&mut self.recipe);
        self.changed(cx);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        if self.recipe.validate().is_ok() {
            self.send(ThemeEditorMsg::Preview(self.recipe.clone()));
        }
        cx.notify();
    }

    /// Shows the selected stop and the accent in their hex fields.
    fn sync_inputs(&mut self, cx: &mut Context<Self>) {
        if let Some(colour) = self.colours().get(self.selected) {
            let hex = colour.to_hex();
            self.stop_hex
                .update(cx, |input, cx| input.set_text(hex, cx));
        }
        if let Some(accent) = self.recipe.accent {
            let hex = accent.0.to_hex();
            self.accent_hex
                .update(cx, |input, cx| input.set_text(hex, cx));
        }
    }

    fn colours(&self) -> Vec<Rgba> {
        match &self.recipe.background {
            Background::Default => Vec::new(),
            Background::Solid { color } => vec![color.0],
            Background::Gradient { stops, .. } => stops.iter().map(|s| s.color.0).collect(),
        }
    }

    fn angle(&self) -> u16 {
        match &self.recipe.background {
            Background::Gradient { angle, .. } => *angle,
            _ => DEFAULT_ANGLE,
        }
    }

    /// One colour makes a solid background; two to five an evenly spaced
    /// gradient (so the stops are always valid).
    fn set_colours(&mut self, mut colours: Vec<Rgba>) {
        colours.truncate(MAX_STOPS);
        self.recipe.background = match colours.as_slice() {
            [] => return,
            [only] => Background::Solid { color: Hex(*only) },
            _ => Background::Gradient {
                angle: self.angle(),
                stops: spaced(&colours),
            },
        };
        self.selected = self.selected.min(colours.len() - 1);
    }

    fn seed(&self) -> Rgba {
        self.colours()
            .first()
            .copied()
            .unwrap_or(Rgba::rgb(DEFAULT_COLOUR))
    }

    fn apply_colours(&mut self, colours: Vec<Rgba>, cx: &mut Context<Self>) {
        self.set_colours(colours);
        self.sync_inputs(cx);
        self.changed(cx);
    }

    fn set_selected_colour(&mut self, colour: Rgba, cx: &mut Context<Self>) {
        let mut colours = self.colours();
        match colours.get_mut(self.selected) {
            Some(slot) => *slot = colour,
            None => colours = vec![colour],
        }
        self.set_colours(colours);
        self.changed(cx);
    }

    fn set_kind(&mut self, kind: usize, cx: &mut Context<Self>) {
        let seed = self.seed();
        let current = match self.recipe.background {
            Background::Default => 0,
            Background::Solid { .. } => 1,
            Background::Gradient { .. } => 2,
        };
        match kind {
            _ if kind == current => return,
            0 => self.recipe.background = Background::Default,
            1 => self.recipe.background = Background::Solid { color: Hex(seed) },
            _ => self.set_colours(harmony(seed, Harmony::SingleAnalogous)),
        }
        self.selected = 0;
        self.sync_inputs(cx);
        self.changed(cx);
    }

    fn add_colour(&mut self, cx: &mut Context<Self>) {
        let mut colours = self.colours();
        let Some(&last) = colours.last() else { return };
        if colours.len() >= MAX_STOPS {
            return;
        }
        let new = harmony(last, Harmony::SingleAnalogous)
            .get(1)
            .copied()
            .unwrap_or(last);
        let at = (self.selected + 1).min(colours.len());
        colours.insert(at, new);
        self.selected = at;
        self.apply_colours(colours, cx);
    }

    fn remove_colour(&mut self, cx: &mut Context<Self>) {
        let mut colours = self.colours();
        if colours.len() <= 2 {
            return;
        }
        colours.remove(self.selected.min(colours.len() - 1));
        self.apply_colours(colours, cx);
    }

    fn turn_angle(&mut self, by: i32, cx: &mut Context<Self>) {
        if let Background::Gradient { angle, .. } = &mut self.recipe.background {
            *angle = (i32::from(*angle) + by).rem_euclid(360) as u16;
            self.changed(cx);
        }
    }

    fn pick_harmony(&mut self, mode: Harmony, cx: &mut Context<Self>) {
        self.selected = 0;
        self.apply_colours(harmony(self.seed(), mode), cx);
    }

    fn shuffle(&mut self, cx: &mut Context<Self>) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        self.selected = 0;
        self.apply_colours(shuffle(nanos), cx);
    }

    fn set_accent_mode(&mut self, custom: bool, cx: &mut Context<Self>) {
        if custom == self.recipe.accent.is_some() {
            return;
        }
        self.recipe.accent = custom.then(|| {
            let shown = resolve(&self.recipe, Mode::Light, None, Accessibility::default());
            Hex(shown.tokens.accent)
        });
        self.sync_inputs(cx);
        self.changed(cx);
    }

    fn pick_texture(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.edit(cx, |r| match ix.checked_sub(1) {
            None => r.grain = 0,
            Some(ix) => {
                r.texture = TEXTURES[ix].1;
                if r.grain == 0 {
                    r.grain = 4;
                }
            }
        });
    }
}
