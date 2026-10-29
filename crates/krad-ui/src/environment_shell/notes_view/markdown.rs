use gpui_kit::base::text::{
    markdown_ast as mdast, MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
};
use gpui_kit::base::TextView;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, img, px, rgb, AnyElement, App, Global, Image, ImageFormat, InteractiveElement,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::components::{icon, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, RADIUS_MD, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM,
};

const INLINE_MATH: &str = "krad-math";
const BLOCK_MATH: &str = "krad-math-block";
const MERMAID: &str = "krad-mermaid";
const CALLOUT: &str = "krad-callout";
const MERMAID_LANGUAGE: &str = "mermaid";
const RENDER_SCALE: f32 = 2.0;
const MATH_FONT_PX: f32 = 16.0;
const MATH_PADDING: f64 = 1.0;
const MATH_STROKE: f64 = 1.5;
const PT_PER_PX: f32 = 0.75;
const VIEW_BOX: &str = "viewBox=\"";
const CALLOUT_OPEN: &str = "[!";
const CALLOUT_CLOSE: char = ']';
const FOLDED_MARK: char = '-';
const OPEN_MARK: char = '+';
const CALLOUT_LINE_MARK: &str = "> ";
const CALLOUT_EMPTY_LINE: &str = ">";
const CALLOUT_RADIUS_WASH: u32 = 0x1a;
const CALLOUT_BAR_W: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalloutKind {
    Note,
    Info,
    Tip,
    Success,
    Question,
    Warning,
    Danger,
    Quote,
    Example,
    Embed,
    Video,
}

impl CalloutKind {
    fn parse(word: &str) -> Self {
        match word.to_ascii_lowercase().as_str() {
            "info" | "todo" => Self::Info,
            "tip" | "hint" | "important" => Self::Tip,
            "success" | "check" | "done" => Self::Success,
            "question" | "help" | "faq" => Self::Question,
            "warning" | "caution" | "attention" => Self::Warning,
            "danger" | "error" | "bug" | "failure" | "fail" | "missing" => Self::Danger,
            "quote" | "cite" => Self::Quote,
            "example" => Self::Example,
            krad_domain::EMBED_CALLOUT => Self::Embed,
            krad_domain::VIDEO_CALLOUT => Self::Video,
            _ => Self::Note,
        }
    }

    const fn glyph(self) -> IconName {
        match self {
            Self::Note | Self::Quote => IconName::Note,
            Self::Info => IconName::Bell,
            Self::Tip => IconName::Flash,
            Self::Success => IconName::CheckCircle,
            Self::Question => IconName::Support,
            Self::Warning | Self::Danger => IconName::CancelCircle,
            Self::Example => IconName::Layers,
            Self::Embed => IconName::Book,
            Self::Video => IconName::Play,
        }
    }

    const fn accent(self, pal: &Palette) -> u32 {
        match self {
            Self::Note | Self::Info | Self::Embed | Self::Video => pal.info_fg,
            Self::Tip | Self::Success => pal.success_fg,
            Self::Question | Self::Warning => pal.warning_fg,
            Self::Danger => pal.error_fg,
            Self::Quote | Self::Example => pal.text_muted,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Info => "Info",
            Self::Tip => "Tip",
            Self::Success => "Success",
            Self::Question => "Question",
            Self::Warning => "Warning",
            Self::Danger => "Danger",
            Self::Quote => "Quote",
            Self::Example => "Example",
            Self::Embed => "Embedded note",
            Self::Video => "Video",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Callout {
    kind: CalloutKind,
    title: String,
    folded: bool,
    body: String,
}

fn parse_callout(source: &str) -> Option<Callout> {
    let mut lines = source.lines().map(|line| {
        line.strip_prefix(CALLOUT_LINE_MARK)
            .or_else(|| line.strip_prefix(CALLOUT_EMPTY_LINE))
            .unwrap_or(line)
    });
    let first = lines.next()?.trim_start();
    let rest = first.strip_prefix(CALLOUT_OPEN)?;
    let (word, after) = rest.split_once(CALLOUT_CLOSE)?;
    let (folded, title) = match after.chars().next() {
        Some(FOLDED_MARK) => (true, &after[1..]),
        Some(OPEN_MARK) => (false, &after[1..]),
        _ => (false, after),
    };
    let kind = CalloutKind::parse(word.trim());
    let title = title.trim();
    Some(Callout {
        kind,
        title: if title.is_empty() {
            kind.label().to_owned()
        } else {
            title.to_owned()
        },
        folded,
        body: lines.collect::<Vec<_>>().join("\n"),
    })
}

#[derive(Default)]
struct DrawnSvgs(HashMap<(String, u32), Option<Drawn>>);

impl Global for DrawnSvgs {}

#[derive(Clone)]
struct Drawn {
    image: Arc<Image>,
    width: f32,
    height: f32,
}

fn view_box(svg: &str) -> Option<(f32, f32)> {
    let start = svg.find(VIEW_BOX)? + VIEW_BOX.len();
    let end = svg[start..].find('"')? + start;
    let numbers: Vec<f32> = svg[start..end]
        .split_whitespace()
        .filter_map(|number| number.parse().ok())
        .collect();
    Some((*numbers.get(2)?, *numbers.get(3)?))
}

fn drawn(
    key: (String, u32),
    draw: impl FnOnce() -> Option<(String, f32)>,
    cx: &mut App,
) -> Option<Drawn> {
    if let Some(known) = cx
        .try_global::<DrawnSvgs>()
        .and_then(|drawn| drawn.0.get(&key))
    {
        return known.clone();
    }
    let result = draw().and_then(|(svg, unit)| {
        let (width, height) = view_box(&svg)?;
        Some(Drawn {
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
            width: width * unit,
            height: height * unit,
        })
    });
    cx.default_global::<DrawnSvgs>()
        .0
        .insert(key, result.clone());
    result
}

fn color_key(color: u32) -> u32 {
    color
}

fn draw_math(source: &str, display: bool, color: u32) -> Option<(String, f32)> {
    use ratex_types::color::Color;
    use ratex_types::math_style::MathStyle;
    let ast = ratex_parser::parse(source).ok()?;
    let channel =
        |shift: u32| f32::from(u8::try_from((color >> shift) & 0xff).unwrap_or(0)) / 255.0;
    let options = ratex_layout::LayoutOptions {
        style: if display {
            MathStyle::Display
        } else {
            MathStyle::Text
        },
        color: Color {
            r: channel(16),
            g: channel(8),
            b: channel(0),
            a: 1.0,
        },
        ..ratex_layout::LayoutOptions::default()
    };
    let laid = ratex_layout::layout(&ast, &options);
    let list = ratex_layout::to_display_list(&laid);
    let svg = ratex_svg::render_to_svg(
        &list,
        &ratex_svg::SvgOptions {
            font_size: f64::from(MATH_FONT_PX * RENDER_SCALE),
            padding: MATH_PADDING,
            stroke_width: MATH_STROKE,
            embed_glyphs: true,
            ..ratex_svg::SvgOptions::default()
        },
    );
    Some((svg, 1.0 / (RENDER_SCALE * PT_PER_PX)))
}

fn draw_mermaid(source: &str, dark: bool) -> Option<(String, f32)> {
    let mut options = mermaid_rs_renderer::RenderOptions::default();
    if dark {
        options.theme = mermaid_rs_renderer::Theme::dark();
    }
    let svg = mermaid_rs_renderer::render_with_options(source, options).ok()?;
    Some((svg, 1.0))
}

fn source_fallback(source: &str, pal: &Palette) -> AnyElement {
    div()
        .text_color(rgb(pal.error_fg))
        .child(source.to_owned())
        .into_any_element()
}

fn drawn_image(drawn: &Drawn) -> AnyElement {
    img(drawn.image.clone())
        .w(px(drawn.width))
        .h(px(drawn.height))
        .flex_none()
        .into_any_element()
}

struct MathPlugin {
    display: bool,
}

impl MarkdownPlugin for MathPlugin {
    fn is_block(&self) -> bool {
        self.display
    }

    fn name(&self) -> &str {
        if self.display {
            BLOCK_MATH
        } else {
            INLINE_MATH
        }
    }

    fn parse(&self, node: &mdast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let value = match (node, self.display) {
            (mdast::Node::InlineMath(math), false) => math.value.clone(),
            (mdast::Node::Math(math), true) => math.value.clone(),
            _ => return None,
        };
        Some(MarkdownNode::new(self.name().to_owned(), value.clone()).text(value))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pal = palette(cx);
        let source = node.data::<String>().cloned().unwrap_or_default();
        let color = pal.text_strong;
        let display = self.display;
        let key = (format!("{display}:{source}"), color_key(color));
        match drawn(key, || draw_math(&source, display, color), cx) {
            Some(drawn) if display => div()
                .flex()
                .justify_center()
                .py(px(SPACE_2))
                .child(drawn_image(&drawn))
                .into_any_element(),
            Some(drawn) => drawn_image(&drawn),
            None => source_fallback(&source, pal),
        }
    }
}

struct MermaidPlugin;

impl MarkdownPlugin for MermaidPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        MERMAID
    }

    fn parse(&self, node: &mdast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::Code(code) = node else {
            return None;
        };
        (code.lang.as_deref() == Some(MERMAID_LANGUAGE))
            .then(|| MarkdownNode::new(MERMAID, code.value.clone()).text(code.value.clone()))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pal = palette(cx);
        let dark = crate::theme::appearance(cx) == crate::theme::Appearance::Dark;
        let source = node.data::<String>().cloned().unwrap_or_default();
        let key = (format!("mermaid:{source}"), u32::from(dark));
        match drawn(key, || draw_mermaid(&source, dark), cx) {
            Some(drawn) => div()
                .id(SharedString::from(format!("mermaid-{}", source.len())))
                .overflow_x_scroll()
                .py(px(SPACE_2))
                .child(drawn_image(&drawn))
                .into_any_element(),
            None => source_fallback(&source, pal),
        }
    }
}

struct CalloutPlugin;

impl MarkdownPlugin for CalloutPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        CALLOUT
    }

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::Blockquote(_) = node else {
            return None;
        };
        let source = cx.node_source(node)?;
        let callout = parse_callout(source)?;
        Some(MarkdownNode::new(CALLOUT, callout.clone()).text(callout.title))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pal = palette(cx);
        let Some(callout) = node.data::<Callout>().cloned() else {
            return div().into_any_element();
        };
        let accent = callout.kind.accent(pal);
        let body = (!callout.folded && !callout.body.trim().is_empty()).then(|| {
            div().child(reading_text(
                SharedString::from(format!("callout-{}-{}", callout.title, callout.body.len())),
                callout.body.clone(),
            ))
        });
        div()
            .flex()
            .flex_row()
            .rounded(px(RADIUS_MD))
            .bg(crate::theme::tinted(accent, CALLOUT_RADIUS_WASH))
            .overflow_hidden()
            .child(div().w(px(CALLOUT_BAR_W)).flex_none().bg(rgb(accent)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_1))
                    .py(px(SPACE_2))
                    .px(px(SPACE_3))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(SPACE_1_5))
                            .text_size(px(TEXT_SM))
                            .text_color(rgb(accent))
                            .child(icon(callout.kind.glyph(), ICON_SM, accent))
                            .child(reading_text(
                                SharedString::from(format!("callout-title-{}", callout.title)),
                                callout.title.clone(),
                            )),
                    )
                    .when_some(body, ParentElement::child),
            )
            .into_any_element()
    }
}

pub(super) fn reading_extensions() -> MarkdownExtensions {
    MarkdownExtensions::default()
        .plugin(MathPlugin { display: false })
        .plugin(MathPlugin { display: true })
        .plugin(MermaidPlugin)
        .plugin(CalloutPlugin)
}

fn reading_text(id: SharedString, markdown: String) -> TextView {
    TextView::markdown(id, markdown).markdown_extensions(reading_extensions())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callouts_read_their_kind_title_and_fold() {
        let callout = parse_callout("> [!warning]- Check this\n> Body line\n>\n> More")
            .unwrap_or_else(|| Callout {
                kind: CalloutKind::Note,
                title: String::new(),
                folded: false,
                body: String::new(),
            });
        assert_eq!(callout.kind, CalloutKind::Warning);
        assert_eq!(callout.title, "Check this");
        assert!(callout.folded);
        assert_eq!(callout.body, "Body line\n\nMore");
        let plain = parse_callout("> [!TIP]\n> Hi");
        assert_eq!(plain.map(|callout| callout.title), Some("Tip".to_owned()));
        assert_eq!(parse_callout("> just a quote"), None);
    }

    #[test]
    fn math_and_diagrams_draw_to_svg_with_a_view_box() {
        let (math, _) =
            draw_math(r"\frac{a}{b} + \sum_{i=1}^{n} x_i", true, 0x00ff_ffff).unwrap_or_default();
        assert!(math.contains("<path"), "glyphs are paths");
        assert!(view_box(&math).is_some_and(|(w, h)| w > 0.0 && h > 0.0));
        assert!(
            draw_math(r"\frac{", false, 0).is_none(),
            "bad input draws nothing"
        );
        let (diagram, _) = draw_mermaid("flowchart LR; A-->B", true).unwrap_or_default();
        assert!(view_box(&diagram).is_some());
    }
}
