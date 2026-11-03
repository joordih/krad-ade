use gpui_kit::component::input::{
    CompletionProvider, DefinitionProvider, HoverProvider, Lsp, Rope, RopeExt as _,
};
use gpui_kit::http_client::Result;
use gpui_kit::{App, AppContext as _, Task, WeakEntity, Window};
use krad_lsp::lsp_types::request::{Completion, GotoDefinition, HoverRequest};
use krad_lsp::lsp_types::{
    CompletionContext, CompletionParams, CompletionResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, Location, LocationLink, PartialResultParams,
    ServerCapabilities, ShowDocumentParams, TextDocumentIdentifier, TextDocumentPositionParams,
    Uri, WorkDoneProgressParams,
};
use krad_lsp::{file_uri, uri_path, LspClient};
use std::rc::Rc;

use super::super::EnvironmentShell;
use super::Document;

const WORD_JOINER: char = '_';

#[derive(Clone)]
pub(super) struct ServerDocument {
    pub(super) client: LspClient,
    pub(super) uri: Uri,
    pub(super) document: Document,
    triggers: Rc<[String]>,
    acts: bool,
    pub(super) shell: WeakEntity<EnvironmentShell>,
}

fn location_link(location: Location) -> LocationLink {
    LocationLink {
        origin_selection_range: None,
        target_uri: location.uri,
        target_range: location.range,
        target_selection_range: location.range,
    }
}

fn links(response: Option<GotoDefinitionResponse>) -> Vec<LocationLink> {
    match response {
        None => Vec::new(),
        Some(GotoDefinitionResponse::Scalar(location)) => vec![location_link(location)],
        Some(GotoDefinitionResponse::Array(locations)) => {
            locations.into_iter().map(location_link).collect()
        }
        Some(GotoDefinitionResponse::Link(links)) => links,
    }
}

impl ServerDocument {
    pub(super) fn new(
        client: LspClient,
        document: &Document,
        uri: Option<Uri>,
        capabilities: &ServerCapabilities,
        shell: WeakEntity<EnvironmentShell>,
    ) -> Option<Self> {
        let triggers = capabilities
            .completion_provider
            .as_ref()
            .and_then(|completion| completion.trigger_characters.clone())
            .unwrap_or_default();
        Some(Self {
            client,
            uri: match uri {
                Some(uri) => uri,
                None => file_uri(&document.path).ok()?,
            },
            document: document.clone(),
            triggers: triggers.into(),
            acts: capabilities.code_action_provider.is_some(),
            shell,
        })
    }

    pub(super) fn position(&self, text: &Rope, offset: usize) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(self.uri.clone()),
            position: text.offset_to_position(offset),
        }
    }

    pub(super) fn install(&self, lsp: &mut Lsp) {
        let shared = Rc::new(self.clone());
        lsp.completion_provider = Some(shared.clone());
        lsp.hover_provider = Some(shared.clone());
        lsp.definition_provider = Some(shared.clone());
        lsp.code_action_providers = if self.acts { vec![shared] } else { Vec::new() };
        let shell = self.shell.clone();
        let own = self.uri.clone();
        let document = self.document.clone();
        lsp.show_document = Some(Rc::new(
            move |params: &ShowDocumentParams, _: &mut Window, cx: &mut App| {
                if params.uri == own {
                    return false;
                }
                let at = params.selection.map(|range| range.start);
                match uri_path(&params.uri) {
                    Some(path) => shell
                        .update(cx, |shell, cx| shell.open_file_at(&path, at, cx))
                        .is_ok(),
                    None => shell
                        .update(cx, |shell, cx| {
                            shell.open_library_document(&document, params.uri.clone(), at, cx);
                        })
                        .is_ok(),
                }
            },
        ));
    }
}

impl CompletionProvider for ServerDocument {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        trigger: CompletionContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let params = CompletionParams {
            text_document_position: self.position(text, offset),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: Some(trigger),
        };
        let answer = self.client.request::<Completion>(&params);
        cx.background_spawn(async move {
            Ok(answer
                .await?
                .unwrap_or_else(|| CompletionResponse::Array(Vec::new())))
        })
    }

    fn is_completion_trigger(&self, _: usize, new_text: &str, _: &mut App) -> bool {
        let typed_word = new_text
            .chars()
            .last()
            .is_some_and(|last| last.is_alphanumeric() || last == WORD_JOINER);
        typed_word
            || self
                .triggers
                .iter()
                .any(|trigger| new_text.ends_with(trigger.as_str()))
    }
}

impl HoverProvider for ServerDocument {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Option<Hover>>> {
        let params = HoverParams {
            text_document_position_params: self.position(text, offset),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let answer = self.client.request::<HoverRequest>(&params);
        let (shell, document) = (self.shell.clone(), self.document.clone());
        cx.spawn(async move |cx| {
            let hover = answer.await.ok().flatten();
            shell.update(cx, |shell, cx| {
                shell.show_hover(&document, offset, hover, cx);
            })?;
            Ok(None)
        })
    }
}

impl DefinitionProvider for ServerDocument {
    fn definitions(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Vec<LocationLink>>> {
        let params = GotoDefinitionParams {
            text_document_position_params: self.position(text, offset),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let answer = self.client.request::<GotoDefinition>(&params);
        cx.background_spawn(async move { Ok(links(answer.await?)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::{Position, Range};
    use std::str::FromStr;

    #[test]
    fn every_definition_shape_becomes_location_links() {
        let Ok(uri) = Uri::from_str("file:///work/lib.rs") else {
            unreachable!("a literal file URI parses");
        };
        let range = Range::new(Position::new(3, 4), Position::new(3, 9));
        let location = Location::new(uri, range);
        assert_eq!(links(None), Vec::new());
        let scalar = links(Some(GotoDefinitionResponse::Scalar(location.clone())));
        assert_eq!(scalar.len(), 1);
        assert_eq!(scalar[0].target_selection_range, range);
        let array = links(Some(GotoDefinitionResponse::Array(vec![
            location.clone(),
            location,
        ])));
        assert_eq!(array.len(), 2);
    }
}
