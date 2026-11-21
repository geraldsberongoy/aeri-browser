//! A short fade toward the pointer's current target. Share one clock across
//! the tab's highlight, title and controls so they can reverse together.

use std::time::{Duration, Instant};

use aeri_core::appearance::Rgba;
use gpui::{Context, Div, Stateful, WeakEntity, Window, prelude::*};

use super::WindowFrame;
use crate::motion;

const FADE: Duration = Duration::from_millis(140);
type Key = (&'static str, u64);

#[derive(Default)]
pub(super) struct TabHovers {
    entries: Vec<(Key, Fade)>,
    scheduled: bool,
}

struct Fade {
    since: Instant,
    from: f32,
    over: bool,
}

impl Fade {
    fn target(&self) -> f32 {
        u8::from(self.over) as f32
    }

    fn sample(&self, now: Instant, still: bool) -> f32 {
        if still || now.duration_since(self.since) >= FADE {
            return self.target();
        }
        let elapsed = now.duration_since(self.since).as_secs_f32() / FADE.as_secs_f32();
        self.from + (self.target() - self.from) * motion::ease_out(elapsed)
    }

    fn retarget(&mut self, now: Instant, over: bool, still: bool) {
        *self = Self {
            since: now,
            from: self.sample(now, still),
            over,
        };
    }
}

impl WindowFrame {
    /// Attach to the existing hit area; never animate hit testing or selection.
    pub(super) fn tab_hover(
        &self,
        el: Stateful<Div>,
        key: Key,
        still: bool,
        cx: &Context<Self>,
    ) -> (Stateful<Div>, f32) {
        let amount = self
            .tab_hovers
            .entries
            .iter()
            .find(|(id, _)| *id == key)
            .map_or(0., |(_, fade)| fade.sample(Instant::now(), still));
        let el = el
            .on_mouse_move(cx.listener(move |this, _, window, cx| {
                if !cx.has_active_drag() {
                    this.retarget_tab_hover(key, true, window, cx);
                }
            }))
            .on_hover(cx.listener(move |this, &over: &bool, window, cx| {
                // Only real moves enter a tab. GPUI can still think a
                // stationary pointer is over its last row after a child
                // Chromium window takes over the mouse.
                if !over {
                    this.retarget_tab_hover(key, false, window, cx);
                }
            }));
        (el, amount)
    }

    pub(crate) fn clear_tab_hover(&mut self, window: &Window, cx: &mut Context<Self>) {
        if let Some((key, _)) = self.tab_hovers.entries.iter().find(|(_, fade)| fade.over) {
            self.retarget_tab_hover(*key, false, window, cx);
        }
    }

    fn retarget_tab_hover(
        &mut self,
        key: Key,
        over: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let hovers = &mut self.tab_hovers;
        let index = hovers.entries.iter().position(|(id, _)| *id == key);
        if index.is_some_and(|i| hovers.entries[i].1.over == over) || index.is_none() && !over {
            return;
        }
        let now = Instant::now();
        let still = motion::still(self.window_id, cx);
        if over {
            // A row removed under the pointer might never send an exit.
            for (id, fade) in &mut hovers.entries {
                if *id != key && fade.over {
                    fade.retarget(now, false, still);
                }
            }
        }
        if let Some(index) = index {
            hovers.entries[index].1.retarget(now, over, still);
        } else {
            hovers.entries.push((
                key,
                Fade {
                    since: now,
                    from: 0.,
                    over,
                },
            ));
        }
        if still {
            hovers.entries.retain(|(_, fade)| fade.over);
        } else if !hovers.scheduled {
            hovers.scheduled = true;
            tick(window, cx.entity().downgrade());
        }
        cx.notify();
    }
}

fn tick(window: &Window, view: WeakEntity<WindowFrame>) {
    window.on_next_frame(move |window, cx| {
        let again = view.update(cx, |frame, cx| {
            let still = motion::still(frame.window_id, cx);
            let hovers = &mut frame.tab_hovers;
            let now = Instant::now();
            let moving = |fade: &Fade| !still && now.duration_since(fade.since) < FADE;
            hovers.entries.retain(|(_, fade)| fade.over || moving(fade));
            hovers.scheduled = hovers.entries.iter().any(|(_, fade)| moving(fade));
            cx.notify();
            hovers.scheduled
        });
        if again.unwrap_or(false) {
            tick(window, view);
        }
    });
}

pub(super) fn blend(from: Rgba, to: Rgba, amount: f32) -> Rgba {
    let lerp = |a, b| a + (b - a) * amount;
    Rgba {
        r: lerp(from.r, to.r),
        g: lerp(from.g, to.g),
        b: lerp(from.b, to.b),
        a: lerp(from.a, to.a),
    }
}
