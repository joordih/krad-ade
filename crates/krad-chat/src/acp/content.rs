use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text(TextContent),
    Image(ImageContent),
    Audio(AudioContent),
    ResourceLink(ResourceLink),
    Resource(EmbeddedResource),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    pub data: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioContent {
    pub data: String,
    pub mime_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLink {
    pub name: String,
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedResource {
    pub resource: EmbeddedResourceResource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddedResourceResource {
    Text(TextResourceContents),
    Blob(BlobResourceContents),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextResourceContents {
    pub uri: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobResourceContents {
    pub uri: String,
    pub blob: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_prompt_carries_text_links_and_embedded_files_as_the_spec_writes_them() {
        let blocks = vec![
            ContentBlock::Text(TextContent {
                text: "Review this".to_owned(),
            }),
            ContentBlock::ResourceLink(ResourceLink {
                name: "main.py".to_owned(),
                uri: "file:///home/user/project/main.py".to_owned(),
                title: None,
                description: None,
                mime_type: Some("text/x-python".to_owned()),
                size: None,
            }),
            ContentBlock::Resource(EmbeddedResource {
                resource: EmbeddedResourceResource::Text(TextResourceContents {
                    uri: "file:///home/user/script.py".to_owned(),
                    text: "print('hi')".to_owned(),
                    mime_type: None,
                }),
            }),
        ];
        assert_eq!(
            serde_json::to_value(&blocks).ok(),
            Some(json!([
                {"type": "text", "text": "Review this"},
                {"type": "resource_link", "name": "main.py", "uri": "file:///home/user/project/main.py", "mimeType": "text/x-python"},
                {"type": "resource", "resource": {"uri": "file:///home/user/script.py", "text": "print('hi')"}}
            ]))
        );
    }

    #[test]
    fn an_embedded_blob_reads_as_a_blob() {
        let block: Result<ContentBlock, _> = serde_json::from_value(json!({
            "type": "resource",
            "resource": {"uri": "file:///a.png", "blob": "iVBOR", "mimeType": "image/png"}
        }));
        assert!(matches!(
            block,
            Ok(ContentBlock::Resource(EmbeddedResource {
                resource: EmbeddedResourceResource::Blob(_)
            }))
        ));
    }
}
