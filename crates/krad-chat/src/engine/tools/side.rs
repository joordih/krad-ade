use krad_domain::GatewayAccess;
use std::sync::atomic::AtomicBool;

use crate::engine::attachments::Attachments;
use crate::engine::provider::stream;
use crate::engine::wire::{Message, NativeTool, Protocol, Reply, Request};
use crate::gateway::{GatewayError, Vendor};

#[derive(Clone, Debug)]
pub(crate) struct SideRequest {
    pub(crate) access: GatewayAccess,
    pub(crate) model: String,
    pub(crate) vendor: Vendor,
}

pub(super) struct Asked<'a> {
    pub(super) system: &'a str,
    pub(super) text: String,
    pub(super) native: Option<NativeTool>,
    pub(super) max_tokens: u64,
}

impl SideRequest {
    pub(super) fn ask(&self, asked: Asked<'_>, cancel: &AtomicBool) -> Result<Reply, GatewayError> {
        let messages = [Message::user(asked.text)];
        let request = Request {
            model: &self.model,
            system: asked.system,
            messages: &messages,
            images: Attachments::none(),
            tools: &[],
            native: asked.native,
            effort: None,
            max_tokens: asked.max_tokens,
        };
        stream(
            &self.access,
            Protocol::from(self.vendor),
            &request,
            cancel,
            &mut |_| {},
        )
    }
}
