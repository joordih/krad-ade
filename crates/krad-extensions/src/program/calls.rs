use krad_extension_api::protocol::{
    CloseView, Confirm, ConfirmParams, GetSecret, GetSecretParams, GetSettings, InputBox,
    InputBoxParams, Log, LogParams, Notification, OpenFile, OpenFileParams, OpenTerminal,
    OpenTerminalParams, OpenView, OpenViewParams, PatchView, PatchViewParams, Pick, PickParams,
    Progress, ProgressParams, Request, RunCommand, RunCommandParams, SendTerminal,
    SendTerminalParams, SetSecret, SetSecretParams, SetTitlebarItem, SetTitlebarItemParams,
    SubscribeEvents, SubscribeEventsParams, Toast, ToastParams, ViewRef, WorkspaceContext,
};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum AppCall {
    Confirm(ConfirmParams),
    Pick(PickParams),
    InputBox(InputBoxParams),
    OpenTerminal(OpenTerminalParams),
    WorkspaceContext,
    OpenFile(OpenFileParams),
    GetSettings,
    GetSecret(GetSecretParams),
    SetSecret(SetSecretParams),
    RunCommand(RunCommandParams),
    OpenView(OpenViewParams),
    CloseView(ViewRef),
    SubscribeEvents(SubscribeEventsParams),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppNotice {
    Log(LogParams),
    PatchView(PatchViewParams),
    Toast(ToastParams),
    Progress(ProgressParams),
    SendTerminal(SendTerminalParams),
    SetTitlebarItem(SetTitlebarItemParams),
}

fn request_params<R: Request>(params: Value) -> Result<R::Params, serde_json::Error> {
    serde_json::from_value(params)
}

fn notice_params<N: Notification>(params: Value) -> Result<N::Params, serde_json::Error> {
    serde_json::from_value(params)
}

impl AppCall {
    pub(crate) fn parse(method: &str, params: Value) -> Option<Result<Self, serde_json::Error>> {
        let call = match method {
            Confirm::METHOD => request_params::<Confirm>(params).map(Self::Confirm),
            Pick::METHOD => request_params::<Pick>(params).map(Self::Pick),
            InputBox::METHOD => request_params::<InputBox>(params).map(Self::InputBox),
            OpenTerminal::METHOD => request_params::<OpenTerminal>(params).map(Self::OpenTerminal),
            WorkspaceContext::METHOD => Ok(Self::WorkspaceContext),
            OpenFile::METHOD => request_params::<OpenFile>(params).map(Self::OpenFile),
            GetSettings::METHOD => Ok(Self::GetSettings),
            GetSecret::METHOD => request_params::<GetSecret>(params).map(Self::GetSecret),
            SetSecret::METHOD => request_params::<SetSecret>(params).map(Self::SetSecret),
            RunCommand::METHOD => request_params::<RunCommand>(params).map(Self::RunCommand),
            OpenView::METHOD => request_params::<OpenView>(params).map(Self::OpenView),
            CloseView::METHOD => request_params::<CloseView>(params).map(Self::CloseView),
            SubscribeEvents::METHOD => {
                request_params::<SubscribeEvents>(params).map(Self::SubscribeEvents)
            }
            _ => return None,
        };
        Some(call)
    }
}

impl AppNotice {
    pub(crate) fn parse(method: &str, params: Value) -> Option<Result<Self, serde_json::Error>> {
        let notice = match method {
            Log::METHOD => notice_params::<Log>(params).map(Self::Log),
            PatchView::METHOD => notice_params::<PatchView>(params).map(Self::PatchView),
            Toast::METHOD => notice_params::<Toast>(params).map(Self::Toast),
            Progress::METHOD => notice_params::<Progress>(params).map(Self::Progress),
            SendTerminal::METHOD => notice_params::<SendTerminal>(params).map(Self::SendTerminal),
            SetTitlebarItem::METHOD => {
                notice_params::<SetTitlebarItem>(params).map(Self::SetTitlebarItem)
            }
            _ => return None,
        };
        Some(notice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_api::view::Tone;
    use serde_json::json;

    #[test]
    fn known_methods_read_into_their_typed_call() {
        let call = AppCall::parse(
            "window/confirm",
            json!({"title": "Remove web?", "confirm": "Remove", "tone": "error"}),
        );
        assert!(matches!(
            call,
            Some(Ok(AppCall::Confirm(ConfirmParams {
                tone: Tone::Error,
                ..
            })))
        ));
        assert!(matches!(
            AppCall::parse("workspace/context", Value::Null),
            Some(Ok(AppCall::WorkspaceContext))
        ));
    }

    #[test]
    fn an_event_subscription_reads_into_its_call() {
        let call = AppCall::parse("events/subscribe", json!({"kinds": ["agent_waiting"]}));
        let Some(Ok(AppCall::SubscribeEvents(params))) = call else {
            unreachable!("a subscription call");
        };
        assert!(params
            .kinds
            .contains(&krad_extension_api::protocol::AppEventKind::AgentWaiting));
    }

    #[test]
    fn unknown_methods_and_bad_params_are_told_apart() {
        assert!(AppCall::parse("view/render", json!({})).is_none());
        assert!(matches!(
            AppCall::parse("window/confirm", json!({"title": 3})),
            Some(Err(_))
        ));
        assert!(AppNotice::parse("window/unknown", json!({})).is_none());
    }

    #[test]
    fn a_titlebar_state_reads_into_its_notice() {
        assert!(matches!(
            AppNotice::parse(
                "titlebar/set",
                json!({"item": "now-playing", "hidden": true})
            ),
            Some(Ok(AppNotice::SetTitlebarItem(SetTitlebarItemParams {
                hidden: Some(true),
                ..
            })))
        ));
    }
}
