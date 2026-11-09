//! A width as the edit panel shows and reads it, in the config file's words: `auto`, `span N`,
//! `px N`. Range problems (`span 0`, a width past the bar) parse; validation reports them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::Width;
use tb_core::model::WidthTextError;

fn parse(text: &str) -> Result<Width, WidthTextError> {
    text.parse()
}

#[test]
fn auto_span_and_px_parse_to_their_widths() {
    assert_eq!(parse("auto").unwrap(), Width::Auto);
    assert_eq!(parse("span 2").unwrap(), Width::Span(2));
    assert_eq!(parse("px 120").unwrap(), Width::Px(120));
}

#[test]
fn spaces_around_a_width_are_ignored() {
    assert_eq!(parse("  span 3 ").unwrap(), Width::Span(3));
    assert_eq!(parse(" auto").unwrap(), Width::Auto);
}

#[test]
fn text_that_is_not_a_width_is_an_error() {
    for text in ["spam", "span", "px -1", "px", "", "span two", "12"] {
        assert!(parse(text).is_err(), "{text:?} is not a width");
    }
}

#[test]
fn widths_out_of_the_bar_range_still_parse() {
    assert_eq!(parse("span 0").unwrap(), Width::Span(0));
    assert_eq!(parse("px 4000").unwrap(), Width::Px(4000));
}

#[test]
fn a_width_shows_as_the_text_that_parses_back_to_it() {
    assert_eq!(Width::Auto.to_string(), "auto");
    assert_eq!(Width::Span(2).to_string(), "span 2");
    assert_eq!(Width::Px(120).to_string(), "px 120");
    for w in [Width::Auto, Width::Span(5), Width::Px(64)] {
        assert_eq!(parse(&w.to_string()).unwrap(), w);
    }
}
