use krad_lsp::lsp_types::{Hover, HoverContents, MarkedString, MarkupContent, MarkupKind};

const FENCE: &str = "```";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum HoverBlock {
    Code {
        language: Option<String>,
        text: String,
    },
    Text(String),
}

fn text_block(text: &str) -> Option<HoverBlock> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| HoverBlock::Text(trimmed.to_owned()))
}

pub(super) fn markdown_blocks(markdown: &str) -> Vec<HoverBlock> {
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut code: Option<(Option<String>, String)> = None;
    for line in markdown.lines() {
        let fence = line.trim_start().strip_prefix(FENCE);
        match (fence, code.take()) {
            (Some(_), Some((language, body))) => blocks.push(HoverBlock::Code {
                language,
                text: body.trim_end().to_owned(),
            }),
            (Some(language), None) => {
                blocks.extend(text_block(&text));
                text.clear();
                let language = language.trim();
                code = Some((
                    (!language.is_empty()).then(|| language.to_owned()),
                    String::new(),
                ));
            }
            (None, Some((language, mut body))) => {
                body.push_str(line);
                body.push('\n');
                code = Some((language, body));
            }
            (None, None) => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    if let Some((language, body)) = code {
        blocks.push(HoverBlock::Code {
            language,
            text: body.trim_end().to_owned(),
        });
    }
    blocks.extend(text_block(&text));
    blocks
}

fn marked_blocks(marked: &MarkedString) -> Vec<HoverBlock> {
    match marked {
        MarkedString::String(markdown) => markdown_blocks(markdown),
        MarkedString::LanguageString(code) => vec![HoverBlock::Code {
            language: Some(code.language.clone()),
            text: code.value.trim_end().to_owned(),
        }],
    }
}

pub(super) fn hover_blocks(hover: &Hover) -> Vec<HoverBlock> {
    match &hover.contents {
        HoverContents::Scalar(marked) => marked_blocks(marked),
        HoverContents::Array(marked) => marked.iter().flat_map(marked_blocks).collect(),
        HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }) => markdown_blocks(value),
        HoverContents::Markup(MarkupContent {
            kind: MarkupKind::PlainText,
            value,
        }) => text_block(value).into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_code_becomes_its_own_block_between_the_text() {
        let blocks = markdown_blocks(
            "```java\npublic void load(File jar)\n```\n---\nLoads the plugin.\n\n```\nraw\n```",
        );
        assert_eq!(
            blocks,
            [
                HoverBlock::Code {
                    language: Some("java".to_owned()),
                    text: "public void load(File jar)".to_owned(),
                },
                HoverBlock::Text("---\nLoads the plugin.".to_owned()),
                HoverBlock::Code {
                    language: None,
                    text: "raw".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn plain_text_and_language_strings_keep_their_kind() {
        let hover = Hover {
            contents: HoverContents::Array(vec![
                MarkedString::LanguageString(krad_lsp::lsp_types::LanguageString {
                    language: "rust".to_owned(),
                    value: "fn main()\n".to_owned(),
                }),
                MarkedString::String("  ".to_owned()),
            ]),
            range: None,
        };
        assert_eq!(
            hover_blocks(&hover),
            [HoverBlock::Code {
                language: Some("rust".to_owned()),
                text: "fn main()".to_owned(),
            }]
        );
    }
}
