// Markdown's behaviour, by example (Search Markdown.swift).

use super::{address, page, slug};

fn html(text: &str) -> String {
    page(text, None).html
}

fn base() -> &'static str {
    "https://docs.example.com/guide/start.md"
}

#[test]
fn the_first_top_heading_is_the_title_and_leaves_the_body() {
    let drawn = page("[Skip](#main)\n\n# The Title\n\nBody *here*.", None);
    assert_eq!(drawn.title.as_deref(), Some("The Title"));
    assert!(!drawn.html.contains("The Title"));
    assert!(drawn.html.contains("<p>Body <em>here</em>.</p>"));
}

#[test]
fn a_front_matter_title_is_used_unless_a_heading_names_the_page() {
    let drawn = page("---\ntitle: \"Quoted\"\n---\nText", None);
    assert_eq!(drawn.title.as_deref(), Some("Quoted"));
    assert_eq!(drawn.html, "<p>Text</p>");
    let drawn = page("---\ntitle: Front\n---\n# Heading\nText", None);
    assert_eq!(drawn.title.as_deref(), Some("Heading"));
}

#[test]
fn text_is_escaped_and_scripts_never_become_markup() {
    let out = html(
        "a <b onclick=x> & \"q\"\n\n<script>alert(1)</script>\n\n<img src=\"javascript:alert(1)\">",
    );
    assert!(!out.contains("<script"));
    assert!(!out.contains("javascript:"));
    assert!(!out.contains("onclick"));
    assert!(out.contains("alert(1)"));
    assert!(out.contains("&amp;"));
}

#[test]
fn only_web_and_mail_addresses_are_links() {
    assert_eq!(html("[x](javascript:alert(1))"), "<p>x</p>");
    assert_eq!(html("[x](data:text/html,hi)"), "<p>x</p>");
    assert_eq!(
        html("[x](https://a.example/p?q=1 \"T\")"),
        "<p><a href=\"https://a.example/p?q=1\" title=\"T\">x</a></p>"
    );
    assert_eq!(
        html("[m](mailto:a@b.example)"),
        "<p><a href=\"mailto:a@b.example\">m</a></p>"
    );
    assert_eq!(address("#top", None).as_deref(), Some("#top"));
    assert_eq!(address("ftp://x.example/", None), None);
    assert_eq!(address("", None), None);
}

#[test]
fn relative_links_are_made_whole_against_the_copy() {
    let drawn = page("[next](../api/index.md) ![p](pic.png)", Some(base()));
    assert!(
        drawn
            .html
            .contains("href=\"https://docs.example.com/api/index.md\"")
    );
    assert!(
        drawn
            .html
            .contains("src=\"https://docs.example.com/guide/pic.png\"")
    );
    // Nothing to resolve against: not an address.
    assert_eq!(html("[x](page.md)"), "<p>x</p>");
}

#[test]
fn headings_get_github_ids_and_repeats_are_numbered() {
    let out = html("## Hello, World!\n\n## Hello, World!\n\n### `Code` thing");
    assert!(out.contains("<h2 id=\"hello-world\">Hello, World!</h2>"));
    assert!(out.contains("<h2 id=\"hello-world-1\">"));
    assert!(out.contains("<h3 id=\"code-thing\"><code>Code</code> thing</h3>"));
    assert_eq!(slug("A_b c-d"), "a_b-c-d");
}

#[test]
fn emphasis_strike_and_code_spans() {
    assert_eq!(
        html("**bold** *it* _it_ ~~gone~~ `a*b*`"),
        "<p><strong>bold</strong> <em>it</em> <em>it</em> <del>gone</del> <code>a*b*</code></p>"
    );
    // Inside a word an underscore is just one.
    assert_eq!(html("snake_case_name"), "<p>snake_case_name</p>");
    // A mark inside an address is not a mark.
    assert!(html("[a](https://x.example/*b*/c)").contains("href=\"https://x.example/*b*/c\""));
}

#[test]
fn fenced_code_is_escaped_with_its_language() {
    assert_eq!(
        html("```rust\nlet a = 1 < 2;\n```"),
        "<pre><code class=\"language-rust\">let a = 1 &lt; 2;</code></pre>"
    );
    // Four spaces are not code (MDX).
    assert_eq!(html("    indented"), "<p>indented</p>");
}

#[test]
fn lists_nest_count_and_carry_task_boxes() {
    assert_eq!(
        html("- one\n- two\n  - inner\n- [x] done"),
        "<ul><li>one</li><li>two\n<ul><li>inner</li></ul></li><li><input type=\"checkbox\" checked disabled> done</li></ul>"
    );
    assert!(html("3. a\n4. b").starts_with("<ol start=\"3\">"));
    // A sentence ending in a number on a new line stays a sentence.
    assert_eq!(
        html("It was in\n1984. Then"),
        "<p>It was in\n1984. Then</p>"
    );
    // Loose items keep their paragraphs.
    assert!(html("- a\n\n- b").contains("<li><p>a</p></li>"));
}

#[test]
fn tables_align_and_scroll_in_their_own_box() {
    let out = html("| a | b |\n|:--|--:|\n| 1 | 2 |");
    assert!(out.starts_with("<div class=\"office-wide\"><table>"));
    assert!(out.contains("<th style=\"text-align:left\">a</th>"));
    assert!(out.contains("<td style=\"text-align:right\">2</td>"));
}

#[test]
fn quotes_rules_and_hard_breaks() {
    assert_eq!(html("> a\n> b"), "<blockquote><p>a\nb</p></blockquote>");
    assert_eq!(html("a\n\n---\n\nb"), "<p>a</p>\n<hr>\n<p>b</p>");
    assert_eq!(html("a  \nb"), "<p>a<br>b</p>");
    // A note to agents says nothing to a reader.
    assert_eq!(html("> Read llms.txt for the index\n\nText"), "<p>Text</p>");
}

#[test]
fn references_entities_and_bare_links() {
    assert_eq!(
        html("[a][r] &amp; &copy;\n\n[r]: https://r.example/"),
        "<p><a href=\"https://r.example/\">a</a> &amp; &copy;</p>"
    );
    assert_eq!(
        html("see https://x.example/a. ok"),
        "<p>see <a href=\"https://x.example/a\">https://x.example/a</a>. ok</p>"
    );
}

#[test]
fn components_give_titles_and_callouts_become_quotes() {
    assert_eq!(
        html("<Note>\nHeads up\n</Note>"),
        "<blockquote>\n<p>Heads up</p>\n</blockquote>"
    );
    assert_eq!(
        html("<Card title=\"Go\" href=\"https://x.example/\" />"),
        "<p class=\"office-part\"><a href=\"https://x.example/\"><strong>Go</strong></a></p>"
    );
    // A capital is a component, not the table tag.
    assert_eq!(html("<Table>"), "");
    assert_eq!(html("<br>"), "<br>");
}

#[test]
fn an_empty_text_draws_nothing() {
    assert_eq!(
        page("", None),
        super::Page {
            title: None,
            html: String::new()
        }
    );
    assert_eq!(page("<!-- gone -->", None).html, "");
}
