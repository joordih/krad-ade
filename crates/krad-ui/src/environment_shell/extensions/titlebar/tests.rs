use super::view::popover_max_height;
use super::*;
use krad_extensions::api::manifest::{Contributions, Launch, Program};
use krad_extensions::api::{ApiVersion, IdError};

fn item(
    id: &str,
    side: TitlebarSide,
    action: TitlebarAction,
) -> Result<TitlebarItemContribution, IdError> {
    Ok(TitlebarItemContribution {
        id: id.parse()?,
        title: id.to_owned(),
        icon: Icon::Play,
        side,
        action,
    })
}

fn player(placement: Placement) -> Result<ExtensionManifest, IdError> {
    Ok(ExtensionManifest {
        id: "player".parse()?,
        name: "Player".to_owned(),
        version: "0.1.0".to_owned(),
        description: None,
        api: ApiVersion::new(1, 1),
        program: Some(Program {
            placement,
            permissions: Vec::new(),
            launch: Launch::Runtime {
                runtime: "node".parse()?,
                args: Vec::new(),
            },
        }),
        activation: Vec::new(),
        contributes: Contributions {
            titlebar_items: vec![
                item(
                    "now-playing",
                    TitlebarSide::Left,
                    TitlebarAction::View("now-playing".parse()?),
                )?,
                item(
                    "next",
                    TitlebarSide::Right,
                    TitlebarAction::Command("next".parse()?),
                )?,
                item(
                    "queue",
                    TitlebarSide::Right,
                    TitlebarAction::Command("queue".parse()?),
                )?,
            ],
            ..Contributions::default()
        },
    })
}

fn key(environment: Option<&str>) -> Result<SlotKey, IdError> {
    Ok(SlotKey {
        extension: "player".parse()?,
        environment: environment.map(|name| EnvironmentName::new(name.to_owned())),
    })
}

fn params(item: &str) -> Result<SetTitlebarItemParams, IdError> {
    Ok(SetTitlebarItemParams {
        item: item.parse()?,
        icon: None,
        title: None,
        active: None,
        hidden: None,
    })
}

fn shown_ids(
    titlebar: &Titlebar,
    manifest: &ExtensionManifest,
    environment: Option<&EnvironmentName>,
) -> Vec<String> {
    titlebar
        .shown(manifest, environment)
        .map(|button| button.item.id.to_string())
        .collect()
}

#[test]
fn buttons_keep_manifest_order_and_their_declared_look() -> Result<(), IdError> {
    let manifest = player(Placement::App)?;
    let titlebar = Titlebar::default();
    assert_eq!(
        shown_ids(&titlebar, &manifest, None),
        ["now-playing", "next", "queue"]
    );
    let first = titlebar
        .shown(&manifest, None)
        .next()
        .ok_or(IdError::Empty)?;
    assert_eq!(first.icon, Icon::Play);
    assert_eq!(first.title, "now-playing");
    assert!(!first.active);
    Ok(())
}

#[test]
fn the_program_changes_the_icon_tooltip_highlight_and_visibility() -> Result<(), IdError> {
    let manifest = player(Placement::App)?;
    let mut titlebar = Titlebar::default();
    let key = key(None)?;
    titlebar.apply(
        &key,
        SetTitlebarItemParams {
            icon: Some(Icon::Stop),
            title: Some("Pause".to_owned()),
            active: Some(true),
            ..params("now-playing")?
        },
    );
    titlebar.apply(
        &key,
        SetTitlebarItemParams {
            active: Some(false),
            ..params("now-playing")?
        },
    );
    let first = titlebar
        .shown(&manifest, None)
        .next()
        .ok_or(IdError::Empty)?;
    assert_eq!(
        (first.icon, first.title, first.active),
        (Icon::Stop, "Pause", false)
    );
    titlebar.apply(
        &key,
        SetTitlebarItemParams {
            hidden: Some(true),
            ..params("next")?
        },
    );
    assert_eq!(
        shown_ids(&titlebar, &manifest, None),
        ["now-playing", "queue"]
    );
    titlebar.forget(&key);
    assert_eq!(
        shown_ids(&titlebar, &manifest, None),
        ["now-playing", "next", "queue"]
    );
    Ok(())
}

#[test]
fn an_environment_program_sets_the_buttons_of_its_environment_only() -> Result<(), IdError> {
    let manifest = player(Placement::Environment)?;
    let mut titlebar = Titlebar::default();
    titlebar.apply(
        &key(Some("work"))?,
        SetTitlebarItemParams {
            hidden: Some(true),
            ..params("queue")?
        },
    );
    let work = EnvironmentName::new("work".to_owned());
    let home = EnvironmentName::new("home".to_owned());
    assert_eq!(environment_of(&manifest, &work), Some(&work));
    assert_eq!(
        shown_ids(&titlebar, &manifest, Some(&work)),
        ["now-playing", "next"]
    );
    assert_eq!(
        shown_ids(&titlebar, &manifest, Some(&home)),
        ["now-playing", "next", "queue"]
    );
    assert_eq!(environment_of(&player(Placement::App)?, &work), None);
    Ok(())
}

#[test]
fn only_the_item_that_names_a_view_opens_it() -> Result<(), IdError> {
    let manifest = player(Placement::App)?;
    let view: ViewId = "now-playing".parse()?;
    let openers: Vec<&TitlebarItemId> = manifest
        .contributes
        .titlebar_items
        .iter()
        .filter(|item| opens_view(item, &view))
        .map(|item| &item.id)
        .collect();
    assert_eq!(openers, [&manifest.contributes.titlebar_items[0].id]);
    Ok(())
}

#[test]
fn the_popover_fits_short_windows_and_stops_growing_in_tall_ones() {
    let short = 300.0;
    assert!(popover_max_height(short) + crate::theme::TOOLBAR_H < short);
    assert!((popover_max_height(10_000.0) - popover_max_height(20_000.0)).abs() < f32::EPSILON);
    assert!(popover_max_height(0.0) > 0.0);
}
