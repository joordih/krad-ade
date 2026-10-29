use gpui_kit::{div, px, rgb, AnyElement, FontWeight, IntoElement, ParentElement, Styled};
use krad_domain::review::{Commit, Pipeline};

use super::words::HostWords;
use std::time::Duration;

use super::summary::{run_time, short_time};
use super::Remote;
use super::{ci_color, ci_glyph, ci_label, meta_line};
use crate::components::chip;
use crate::theme::{Palette, ICON_SM, LINE_XS, SPACE_1, SPACE_2, SPACE_3, TEXT_XS};

const LOADING: &str = "Loading…";
const NO_COMMITS: &str = "No commits.";
const SHORT_SHA_LEN: usize = 8;
const META_SEPARATOR: &str = " · ";

pub(super) fn commits(commits: &Remote<Vec<Commit>>, pal: &Palette) -> AnyElement {
    let rows = ready_rows(commits, NO_COMMITS, pal, |commit| {
        let when = commit.authored_date.as_deref().map(short_time);
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(
                div()
                    .text_color(rgb(pal.text_strong))
                    .child(commit.title.clone()),
            )
            .child(meta_line(
                &[
                    Some(commit.short_id.clone()),
                    Some(commit.author_name.clone()),
                    when,
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(META_SEPARATOR),
                pal,
            ))
            .into_any_element()
    });
    list(rows)
}

pub(super) fn pipelines(
    pipelines: &Remote<Vec<Pipeline>>,
    words: &HostWords,
    pal: &Palette,
) -> AnyElement {
    let rows = ready_rows(pipelines, words.no_pipelines, pal, |pipeline| {
        let sha: String = pipeline.sha.0.chars().take(SHORT_SHA_LEN).collect();
        let status = Some(pipeline.status);
        let ran = pipeline
            .duration
            .map(|seconds| run_time(Duration::from_secs(seconds)));
        let when = pipeline.created_at.as_deref().map(short_time);
        let meta: Vec<String> = [Some(pipeline.git_ref.clone()), Some(sha), when, ran]
            .into_iter()
            .flatten()
            .collect();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(ci_glyph(status, ICON_SM, pal))
            .child(
                div()
                    .text_color(rgb(ci_color(status, pal)))
                    .font_weight(FontWeight::MEDIUM)
                    .child(ci_label(pipeline.status)),
            )
            .child(chip(format!("#{}", pipeline.id), pal))
            .child(meta_line(&meta.join(META_SEPARATOR), pal))
            .into_any_element()
    });
    list(rows)
}

fn ready_rows<T>(
    remote: &Remote<Vec<T>>,
    empty: &str,
    pal: &Palette,
    row: impl Fn(&T) -> AnyElement,
) -> Vec<AnyElement> {
    match remote {
        Remote::Loading => vec![meta_line(LOADING, pal)],
        Remote::Failed(reason) => vec![meta_line(reason, pal)],
        Remote::Ready(items) if items.is_empty() => vec![meta_line(empty, pal)],
        Remote::Ready(items) => items.iter().map(row).collect(),
    }
}

fn list(rows: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_3))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .children(rows)
        .into_any_element()
}
