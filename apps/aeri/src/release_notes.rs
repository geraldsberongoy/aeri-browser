//! What's New: `aeri://release-notes`, the notes of every version, newest
//! first (Search `ReleaseNotesPanel`, Settings › About › What's new). The
//! text is `CHANGELOG.md`, built into the app, read as the small part of
//! Markdown it uses: `##` a version, `###` a list's title, `-` an item and
//! plain lines. A page of Aeri's own is a GPUI view in a window of its own
//! that the frame places over the tab's page, as Settings is
//! (`shell::internal`).

use gpui::{Context, FontWeight, Window, div, prelude::*, px};

use crate::theme;

/// The notes as written in the repository.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// Search's plate for the full notes is 560 wide.
const WIDTH: f32 = 560.;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Block {
    /// A version: "0.1.0 (in development)".
    Version(String),
    /// A list's title inside a version: New, Better, Fixed, or a date.
    Heading(String),
    Item(String),
    Text(String),
}

/// The blocks of `markdown`, without its `#` title and blank lines. Code
/// marks and bold marks are dropped: the page has one weight for body text.
fn blocks(markdown: &str) -> Vec<Block> {
    let plain = |text: &str| text.replace(['`', '*'], "");
    markdown
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            if line.is_empty() || (line.starts_with("# ") && !line.starts_with("## ")) {
                None
            } else if let Some(text) = line.strip_prefix("### ") {
                Some(Block::Heading(plain(text)))
            } else if let Some(text) = line.strip_prefix("## ") {
                Some(Block::Version(plain(text)))
            } else if let Some(text) = line.strip_prefix("- ") {
                Some(Block::Item(plain(text)))
            } else {
                Some(Block::Text(plain(line)))
            }
        })
        .collect()
}

pub struct ReleaseNotes {
    blocks: Vec<Block>,
}

impl ReleaseNotes {
    pub fn new() -> Self {
        Self {
            blocks: blocks(CHANGELOG),
        }
    }
}

impl Render for ReleaseNotes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let column = self.blocks.iter().enumerate().fold(
            div().w_full().flex().flex_col().gap(px(6.)),
            |column, (ix, block)| match block {
                Block::Version(text) => column.child(
                    div()
                        .when(ix > 0, |d| d.mt(px(18.)))
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(text.clone()),
                ),
                Block::Heading(text) => column.child(
                    div()
                        .mt(px(6.))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::c(t.text_secondary))
                        .child(text.clone()),
                ),
                Block::Item(text) => column.child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .text_size(px(12.5))
                        .child(
                            div()
                                .text_color(theme::c(t.text_disabled))
                                .child("\u{2022}"),
                        )
                        .child(div().flex_1().min_w_0().child(text.clone())),
                ),
                Block::Text(text) => column.child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme::c(t.text_secondary))
                        .child(text.clone()),
                ),
            },
        );
        div()
            .id("release-notes")
            .size_full()
            .overflow_y_scroll()
            .bg(theme::c(t.panel))
            .text_color(theme::c(t.text))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .flex()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(WIDTH + 48.))
                    .px(px(24.))
                    .pt(px(44.))
                    .pb(px(32.))
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("What's New"),
                    )
                    .child(column),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_blocks() {
        let notes = "# Title\n\nIntro with `code`.\n\n## 0.2.0\n\n### New\n\n- **Bold** item\n";
        assert_eq!(
            blocks(notes),
            [
                Block::Text("Intro with code.".into()),
                Block::Version("0.2.0".into()),
                Block::Heading("New".into()),
                Block::Item("Bold item".into()),
            ]
        );
    }

    #[test]
    fn the_changelog_has_a_version_and_items() {
        let blocks = blocks(CHANGELOG);
        assert!(blocks.iter().any(|b| matches!(b, Block::Version(_))));
        assert!(blocks.iter().any(|b| matches!(b, Block::Item(_))));
    }
}
