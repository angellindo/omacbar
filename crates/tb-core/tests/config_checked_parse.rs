//! A checked parse turns config text into a config only when it has no issue; otherwise it lists
//! every issue. Text that is not valid TOML is one `Syntax` issue.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::config::{Config, ConfigIssue, parse_valid, to_toml};
use tb_core::{BarSize, BarTheme, Chord, SpacingRules};

fn checked(src: &str) -> Result<Config, Vec<ConfigIssue>> {
    parse_valid(src, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback())
}

fn button_label_mut<'a>(config: &'a mut Config, id: &str) -> &'a mut String {
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let button =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == id).unwrap();
    &mut button.label
}

#[test]
fn the_default_config_text_checks_back_to_the_default_config() {
    let text = to_toml(&Config::default()).unwrap();
    assert_eq!(checked(&text), Ok(Config::default()), "\n{text}");
}

#[test]
fn text_that_is_not_valid_toml_is_exactly_one_syntax_issue() {
    let issues = checked("version = ").unwrap_err();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(matches!(&issues[0], ConfigIssue::Syntax(message) if !message.is_empty()), "{issues:?}");
}

#[test]
fn a_parsed_config_with_problems_lists_every_issue() {
    let mut config = Config { version: 2, ..Config::default() };
    *button_label_mut(&mut config, "f5") = String::new();
    let issues = checked(&to_toml(&config).unwrap()).unwrap_err();
    assert!(issues.contains(&ConfigIssue::Version(2)), "{issues:?}");
    assert!(
        issues.iter().any(
            |i| matches!(i, ConfigIssue::EmptyLabel { chord: Chord::Default, id } if id.as_str() == "f5")
        ),
        "{issues:?}"
    );
    assert_eq!(issues.len(), 2, "{issues:?}");
}
