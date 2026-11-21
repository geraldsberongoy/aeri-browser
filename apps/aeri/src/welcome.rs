//! First run: a full-screen film, three hands-on feature previews, then setup.
//! Only Start Browsing submits explicitly changed preferences.

mod tour;

use std::sync::Arc;

use aeri_core::appearance::AppAppearance;
use aeri_core::layout::TabPlace;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, Hsla, ImageSource, IntoElement, KeyDownEvent,
    ObjectFit, ParentElement, Render, RenderImage, Stateful, Styled, Window, div, img,
    linear_color_stop, linear_gradient, prelude::*, px, rgb,
};

use self::tour::Feature;
use crate::mac::intro::IntroPlayer;
use crate::shell::AppMsg;

#[derive(Clone, Copy, Default)]
pub(crate) struct Choices {
    pub(crate) tabs: Option<TabPlace>,
    pub(crate) appearance: Option<AppAppearance>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Film,
    Tour(Feature),
    Setup,
}

const INK: u32 = 0x0B0B0B;
const INK_RAISED: u32 = 0x161616;
const TEXT: u32 = 0xF5F5F5;
const MUTED: u32 = 0xAAAAAA;
const FILM_FOOTER: f32 = 72.;

fn white(alpha: f32) -> Hsla {
    let mut c: Hsla = rgb(0xFFFFFF).into();
    c.a = alpha;
    c
}

pub(crate) struct Welcome {
    step: Step,
    initial: Choices,
    choices: Choices,
    demo: bool,
    player: Option<IntroPlayer>,
    film_run: u64,
    film_available: bool,
    ribbon: Option<Arc<RenderImage>>,
    tx: UnboundedSender<AppMsg>,
    side: FocusHandle,
    top: FocusHandle,
    skip: FocusHandle,
    start: FocusHandle,
    back: FocusHandle,
    try_it: FocusHandle,
    appearance: [FocusHandle; 3],
}

impl Welcome {
    pub(crate) fn new(
        initial: Choices,
        reduce_motion: bool,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let handle = |cx: &mut Context<Self>| cx.focus_handle().tab_stop(true);
        let this = Self {
            step: if reduce_motion {
                Step::Tour(Feature::Spaces)
            } else {
                Step::Film
            },
            initial,
            choices: Choices::default(),
            demo: false,
            player: None,
            film_run: 0,
            film_available: true,
            ribbon: crate::brand::ribbon(true),
            tx,
            side: handle(cx),
            top: handle(cx),
            skip: handle(cx),
            start: handle(cx),
            back: handle(cx),
            try_it: handle(cx),
            appearance: std::array::from_fn(|_| handle(cx)),
        };
        // Enter always takes the forward action, including skipping the film.
        window.focus(&this.start, cx);
        this
    }

    fn finish(&self, choices: Option<Choices>) {
        let _ = self.tx.unbounded_send(AppMsg::FinishWelcome(choices));
    }

    fn choose(&mut self, place: TabPlace, cx: &mut Context<Self>) {
        self.choices.tabs = Some(place);
        cx.notify();
    }

    pub(crate) fn schedule_intro(&self, window: &Window, cx: &mut Context<Self>) {
        if self.step != Step::Film {
            return;
        }
        let view = cx.entity().downgrade();
        let run = self.film_run;
        window.defer(cx, move |window, cx| {
            let _ = view.update(cx, |this, cx| {
                if this.step != Step::Film || this.film_run != run {
                    return;
                }
                if !window.is_simple_fullscreen() {
                    window.toggle_simple_fullscreen();
                }
                let view = cx.entity().downgrade();
                window.on_next_frame(move |window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if this.step != Step::Film || this.film_run != run || this.player.is_some()
                        {
                            return;
                        }
                        let tx = this.tx.clone();
                        let id = window.window_handle().window_id();
                        this.player = crate::shell::content_view(window).and_then(|parent| {
                            // SAFETY: the live GPUI window exposes its AppKit content view.
                            unsafe {
                                IntroPlayer::play(parent, f64::from(FILM_FOOTER), move || {
                                    let _ = tx.unbounded_send(AppMsg::WelcomeFilmFinished(id, run));
                                })
                            }
                        });
                        if this.player.is_none() {
                            this.film_available = false;
                            this.film_finished(run, window, cx);
                        }
                    });
                });
            });
        });
    }

    pub(crate) fn stop_intro(&mut self) {
        self.player = None;
    }

    pub(crate) fn film_finished(&mut self, run: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.step == Step::Film && self.film_run == run {
            self.show(Step::Tour(Feature::Spaces), window, cx);
        }
    }

    fn show(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        if step != Step::Film {
            self.stop_intro();
            if self.step == Step::Film && window.is_simple_fullscreen() {
                window.toggle_simple_fullscreen();
            }
        } else {
            self.film_run += 1;
        }
        self.step = step;
        self.demo = false;
        window.focus(&self.start, cx);
        cx.notify();
        self.schedule_intro(window, cx);
    }

    fn forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            Step::Film => self.film_finished(self.film_run, window, cx),
            Step::Tour(feature) => {
                self.show(feature.next().map_or(Step::Setup, Step::Tour), window, cx);
            }
            Step::Setup => self.finish(Some(self.choices)),
        }
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            Step::Film => {}
            Step::Tour(feature) => {
                self.show(
                    feature.previous().map_or(Step::Film, Step::Tour),
                    window,
                    cx,
                );
            }
            Step::Setup => self.show(Step::Tour(Feature::Split), window, cx),
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let m = &event.keystroke.modifiers;
        if m.platform || m.control || m.alt {
            return;
        }
        match key {
            "escape" => {
                cx.stop_propagation();
                if self.step == Step::Film {
                    self.film_finished(self.film_run, window, cx);
                } else {
                    self.finish(None);
                }
            }
            "tab" => {
                cx.stop_propagation();
                if m.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
            }
            "left" | "right" if self.step == Step::Setup => {
                let on_side = self.side.is_focused(window);
                if on_side || self.top.is_focused(window) {
                    cx.stop_propagation();
                    let (place, handle) = if (key == "right") == on_side {
                        (TabPlace::Top, &self.top)
                    } else {
                        (TabPlace::Side, &self.side)
                    };
                    window.focus(handle, cx);
                    self.choose(place, cx);
                }
            }
            _ => {}
        }
    }

    fn hero(&self) -> impl IntoElement {
        let ribbon = div().w(px(236.)).h(px(236.)).flex_none();
        let ribbon = match &self.ribbon {
            Some(image) => ribbon.child(
                img(ImageSource::from(image.clone()))
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            ),
            None => ribbon,
        };
        // The film owns the entrance motion; no second reveal in setup.
        div()
            .flex_none()
            .w(px(280.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(linear_gradient(
                200.,
                linear_color_stop(white(0.09), 0.),
                linear_color_stop(white(0.), 0.7),
            ))
            .border_r_1()
            .border_color(white(0.06))
            .child(ribbon)
    }

    fn card(
        &self,
        place: TabPlace,
        title: &'static str,
        caption: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (id, handle) = match place {
            TabPlace::Side => ("welcome-side", &self.side),
            TabPlace::Top => ("welcome-top", &self.top),
        };
        let on = self.choices.tabs.or(self.initial.tabs) == Some(place);
        div()
            .id(id)
            .track_focus(handle)
            .cursor_pointer()
            .flex_1()
            .p(px(3.))
            .rounded(px(16.))
            .border_2()
            .border_color(Hsla::transparent_black())
            .focus_visible(|s| s.border_color(white(0.85)))
            .on_click(cx.listener(move |this, _, _, cx| this.choose(place, cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(if on { white(0.65) } else { white(0.15) })
                    .bg(if on {
                        white(0.09)
                    } else {
                        rgb(INK_RAISED).into()
                    })
                    .child(diagram(place, on))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT))
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(rgb(MUTED))
                                            .child(caption),
                                    ),
                            )
                            .child(radio(on)),
                    ),
            )
    }

    fn appearance_choices(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.choices.appearance.or(self.initial.appearance);
        div().flex().gap(px(6.)).children(
            [
                ("Match macOS", AppAppearance::System),
                ("Light", AppAppearance::Light),
                ("Dark", AppAppearance::Dark),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (label, mode))| {
                let on = selected == Some(mode)
                    || (mode == AppAppearance::System
                        && selected == Some(AppAppearance::FollowTheme));
                button(
                    mode.key(),
                    &self.appearance[i],
                    label,
                    if on {
                        ButtonStyle::Primary
                    } else {
                        ButtonStyle::Secondary
                    },
                )
                .flex_1()
                .justify_center()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.choices.appearance = Some(mode);
                    cx.notify();
                }))
            }),
        )
    }

    fn buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let previous = match self.step {
            Step::Film => None,
            Step::Tour(Feature::Spaces) => self.film_available.then_some("Watch Intro"),
            _ => Some("Back"),
        };
        let next = match self.step {
            Step::Film => "Skip Intro",
            Step::Tour(Feature::Split) => "Customize Aeri",
            Step::Tour(_) => "Next",
            Step::Setup => "Start Browsing",
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.))
            .child(div().flex().gap(px(4.)).children(previous.map(|label| {
                button("welcome-back", &self.back, label, ButtonStyle::Quiet)
                    .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
            })))
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(
                        button("welcome-skip", &self.skip, "Skip Setup", ButtonStyle::Quiet)
                            .on_click(cx.listener(|this, _, _, _| this.finish(None))),
                    )
                    .child(
                        button("welcome-start", &self.start, next, ButtonStyle::Primary)
                            .on_click(cx.listener(|this, _, window, cx| this.forward(window, cx))),
                    ),
            )
    }

    fn film(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x000000))
            .flex()
            .flex_col()
            .child(div().flex_1())
            .child(
                div()
                    .h(px(FILM_FOOTER))
                    .flex_none()
                    .px(px(28.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                button(
                                    "film-skip-setup",
                                    &self.skip,
                                    "Skip Setup",
                                    ButtonStyle::Quiet,
                                )
                                .on_click(cx.listener(|this, _, _, _| this.finish(None))),
                            )
                            .child(
                                button(
                                    "intro-skip",
                                    &self.start,
                                    "Skip Intro",
                                    ButtonStyle::Primary,
                                )
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.forward(window, cx)),
                                ),
                            ),
                    ),
            )
    }

    fn tour(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .pt(px(48.))
            .px(px(36.))
            .pb(px(28.))
            .flex()
            .flex_col()
            .justify_between()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(30.))
                    .child(
                        div()
                            .w(px(254.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .pt(px(18.))
                            .child(heading(feature.title()))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .text_color(rgb(MUTED))
                                    .child(feature.description()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(TEXT))
                                    .border_t_1()
                                    .border_color(white(0.16))
                                    .pt(px(14.))
                                    .child(feature.tip()),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(feature.preview(self.demo))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(MUTED))
                                            .child("Interactive preview"),
                                    )
                                    .child(
                                        button(
                                            "welcome-demo",
                                            &self.try_it,
                                            feature.action(self.demo),
                                            ButtonStyle::Secondary,
                                        )
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.demo = !this.demo;
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(feature.progress()),
                    )
                    .child(self.buttons(cx)),
            )
    }

    fn setup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().child(self.hero()).child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .flex_col()
                .justify_between()
                .px(px(26.))
                .pt(px(44.))
                .pb(px(28.))
                .child(heading("Make Aeri yours"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(caption("Tab layout"))
                        .child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .child(self.card(
                                    TabPlace::Side,
                                    "Sidebar",
                                    "Tabs down the side",
                                    cx,
                                ))
                                .child(self.card(
                                    TabPlace::Top,
                                    "Top Tabs",
                                    "Tabs across the top",
                                    cx,
                                )),
                        )
                        .child(caption("Appearance"))
                        .child(self.appearance_choices(cx))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(MUTED))
                                .child("You can change both in Settings."),
                        ),
                )
                .child(self.buttons(cx)),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonStyle {
    Primary,
    Secondary,
    Quiet,
}

fn button(
    id: &'static str,
    focus: &FocusHandle,
    label: &'static str,
    style: ButtonStyle,
) -> Stateful<gpui::Div> {
    let primary = style == ButtonStyle::Primary;
    div()
        .id(id)
        .track_focus(focus)
        .cursor_pointer()
        .h(px(40.))
        .px(px(if primary { 17. } else { 12. }))
        .flex()
        .items_center()
        .rounded(px(20.))
        .border_2()
        .border_color(if style == ButtonStyle::Secondary {
            white(0.20)
        } else {
            Hsla::transparent_black()
        })
        .bg(if primary {
            rgb(TEXT).into()
        } else {
            Hsla::transparent_black()
        })
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(if primary { INK } else { TEXT }))
        .hover(move |s| {
            s.bg(if primary {
                rgb(0xDADADA).into()
            } else {
                white(0.10)
            })
        })
        .focus_visible(move |s| {
            s.border_color(if primary {
                rgb(0x707070).into()
            } else {
                white(0.85)
            })
        })
        .child(label)
}

fn heading(text: &'static str) -> impl IntoElement {
    div()
        .text_size(px(31.))
        .font_weight(FontWeight::BOLD)
        .child(text)
}

fn caption(text: &'static str) -> impl IntoElement {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .child(text)
}

/// A small window drawn with its tabs: down the side or across the top.
fn diagram(place: TabPlace, on: bool) -> impl IntoElement {
    let tab = |on_tab: bool| -> Hsla {
        if on_tab {
            white(if on { 0.85 } else { 0.5 })
        } else {
            white(0.16)
        }
    };
    let page = div()
        .flex_1()
        .rounded(px(5.))
        .bg(white(0.05))
        .p(px(8.))
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(div().h(px(6.)).w(px(54.)).rounded_full().bg(white(0.14)))
        .child(div().h(px(4.)).w_full().rounded_full().bg(white(0.07)))
        .child(div().h(px(4.)).w(px(86.)).rounded_full().bg(white(0.07)));
    let frame = div()
        .h(px(96.))
        .rounded(px(8.))
        .bg(rgb(INK))
        .border_1()
        .border_color(white(0.08))
        .p(px(6.))
        .gap(px(6.))
        .flex();
    match place {
        TabPlace::Side => frame
            .child(
                div()
                    .w(px(38.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().h(px(9.)).rounded_full().bg(tab(true)))
                    .child(div().h(px(9.)).rounded_full().bg(tab(false)))
                    .child(div().h(px(9.)).rounded_full().bg(tab(false))),
            )
            .child(page),
        TabPlace::Top => frame
            .flex_col()
            .child(
                div()
                    .h(px(11.))
                    .flex_none()
                    .flex()
                    .gap(px(4.))
                    .child(div().w(px(42.)).rounded_full().bg(tab(true)))
                    .child(div().w(px(32.)).rounded_full().bg(tab(false)))
                    .child(div().w(px(32.)).rounded_full().bg(tab(false))),
            )
            .child(page),
    }
}

fn radio(on: bool) -> impl IntoElement {
    let ring = div()
        .size(px(18.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(if on { white(0.90) } else { white(0.45) });
    if on {
        ring.child(div().size(px(10.)).rounded_full().bg(rgb(TEXT)))
    } else {
        ring
    }
}

impl Focusable for Welcome {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.start.clone()
    }
}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.step {
            Step::Film => self.film(cx).into_any_element(),
            Step::Tour(feature) => self.tour(feature, cx).into_any_element(),
            Step::Setup => self.setup(cx).into_any_element(),
        };
        div()
            .id("welcome")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.on_key_down(event, window, cx)
            }))
            .size_full()
            .bg(rgb(INK))
            .text_color(rgb(TEXT))
            .line_height(gpui::relative(crate::theme::LINE_HEIGHT))
            .child(body)
    }
}
