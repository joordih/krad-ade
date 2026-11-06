use krad_domain::{ChatImage, Mention, Prompt};
use std::io;
use std::path::Path;

use crate::acp::content::{
    ContentBlock, EmbeddedResource, EmbeddedResourceResource, ImageContent, ResourceLink,
    TextContent, TextResourceContents,
};
use crate::acp::setup::PromptCapabilities;
use crate::mention::{read_image, served_image, served_path};
use crate::process::Runner;

const FILE_SCHEME: &str = "file://";
const URI_SEPARATOR: char = '/';
const WINDOWS_SEPARATOR: char = '\\';
const URI_KEPT: &[u8] = b"-._~/:";
const MAX_EMBEDDED_BYTES: u64 = 256 * 1024;
const ESCAPE: char = '%';
const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";
const NIBBLE_BITS: u8 = 4;
const LOW_NIBBLE: u8 = 0x0F;

pub(super) fn file_uri(served: &str) -> String {
    let slashed = served.replace(WINDOWS_SEPARATOR, "/");
    let mut uri = String::from(FILE_SCHEME);
    if !slashed.starts_with(URI_SEPARATOR) {
        uri.push(URI_SEPARATOR);
    }
    for byte in slashed.bytes() {
        if byte.is_ascii_alphanumeric() || URI_KEPT.contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push(ESCAPE);
            uri.push(char::from(HEX_DIGITS[usize::from(byte >> NIBBLE_BITS)]));
            uri.push(char::from(HEX_DIGITS[usize::from(byte & LOW_NIBBLE)]));
        }
    }
    uri
}

fn text(text: String) -> ContentBlock {
    ContentBlock::Text(TextContent { text })
}

fn image(
    runner: &Runner,
    image: &ChatImage,
    capabilities: PromptCapabilities,
) -> io::Result<ContentBlock> {
    if !capabilities.image {
        return served_image(runner, image).map(text);
    }
    let encoded = read_image(image)?;
    Ok(ContentBlock::Image(ImageContent {
        data: encoded.data,
        mime_type: encoded.media_type.to_owned(),
        uri: None,
    }))
}

fn embedded(runner: &Runner, path: &Path, uri: &str) -> Option<ContentBlock> {
    let bytes = runner.link().read(path, MAX_EMBEDDED_BYTES + 1).ok()?;
    if u64::try_from(bytes.len()).ok()? > MAX_EMBEDDED_BYTES {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    Some(ContentBlock::Resource(EmbeddedResource {
        resource: EmbeddedResourceResource::Text(TextResourceContents {
            uri: uri.to_owned(),
            text,
            mime_type: None,
        }),
    }))
}

fn file(runner: &Runner, path: &Path, capabilities: PromptCapabilities) -> ContentBlock {
    let uri = file_uri(&served_path(runner, path));
    let fits = runner
        .link()
        .metadata(path)
        .is_ok_and(|metadata| !metadata.is_dir() && metadata.len <= MAX_EMBEDDED_BYTES);
    if capabilities.embedded_context && fits {
        if let Some(block) = embedded(runner, path, &uri) {
            return block;
        }
    }
    ContentBlock::ResourceLink(ResourceLink {
        name: path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        uri,
        title: None,
        description: None,
        mime_type: None,
        size: None,
    })
}

pub(super) fn prompt_blocks(
    prompt: &Prompt,
    runner: &Runner,
    capabilities: PromptCapabilities,
) -> io::Result<Vec<ContentBlock>> {
    let mut blocks = Vec::with_capacity(prompt.mentions.len() + 1);
    if !prompt.text.is_empty() {
        blocks.push(text(prompt.text.clone()));
    }
    for mention in &prompt.mentions {
        blocks.push(match mention {
            Mention::Image(chat_image) => image(runner, chat_image, capabilities)?,
            Mention::Path(path) => file(runner, path, capabilities),
        });
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ImageFormat;
    use std::path::PathBuf;

    fn runner() -> Runner {
        Runner::Native(PathBuf::from("agent"))
    }

    fn scratch(name: &str) -> std::io::Result<PathBuf> {
        let folder =
            std::env::temp_dir().join(format!("krad-acp-prompt-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder)?;
        Ok(folder)
    }

    #[test]
    fn served_paths_become_file_uris_with_unsafe_bytes_escaped() {
        assert_eq!(
            file_uri("/home/me/my file.rs"),
            "file:///home/me/my%20file.rs"
        );
        assert_eq!(file_uri(r"C:\work\a.rs"), "file:///C:/work/a.rs");
    }

    #[test]
    fn a_file_is_embedded_only_when_the_agent_takes_embedded_context(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = scratch("file")?.join("notes.md");
        std::fs::write(&path, "hello")?;
        let prompt = Prompt::new("look", vec![Mention::Path(path.clone())]);
        let linked = prompt_blocks(&prompt, &runner(), PromptCapabilities::default())?;
        assert!(matches!(
            linked.as_slice(),
            [ContentBlock::Text(_), ContentBlock::ResourceLink(link)] if link.name == "notes.md"
        ));
        let capabilities = PromptCapabilities {
            embedded_context: true,
            ..PromptCapabilities::default()
        };
        let embedded = prompt_blocks(&prompt, &runner(), capabilities)?;
        assert!(matches!(
            embedded.as_slice(),
            [_, ContentBlock::Resource(EmbeddedResource {
                resource: EmbeddedResourceResource::Text(contents)
            })] if contents.text == "hello"
        ));
        Ok(())
    }

    #[test]
    fn an_image_goes_as_an_image_only_when_the_agent_takes_images(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = scratch("image")?.join("shot.png");
        std::fs::write(&path, [1_u8, 2, 3])?;
        let prompt = Prompt::new(
            "",
            vec![Mention::Image(ChatImage {
                path: path.clone(),
                format: ImageFormat::Png,
            })],
        );
        let as_path = prompt_blocks(&prompt, &runner(), PromptCapabilities::default())?;
        assert!(matches!(
            as_path.as_slice(),
            [ContentBlock::Text(TextContent { text })] if Path::new(text) == path
        ));
        let capabilities = PromptCapabilities {
            image: true,
            ..PromptCapabilities::default()
        };
        let as_image = prompt_blocks(&prompt, &runner(), capabilities)?;
        assert!(matches!(
            as_image.as_slice(),
            [ContentBlock::Image(ImageContent { mime_type, data, .. })] if mime_type == "image/png" && data == "AQID"
        ));
        Ok(())
    }
}
