use krad_domain::{
    EventKind, HttpUrlError, Webhook, WebhookId, WebhookName, WebhookNameError, WebhookSecret,
    WebhookSigning, WebhookUrl,
};
use std::collections::BTreeSet;
use std::fmt;

pub(super) struct Draft {
    pub(super) editing: Option<Webhook>,
    pub(super) name: String,
    pub(super) url: String,
    pub(super) secret: String,
    pub(super) kinds: BTreeSet<EventKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DraftError {
    Name(WebhookNameError),
    Url(HttpUrlError),
    NoKinds,
}

impl fmt::Display for DraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(error) => error.fmt(formatter),
            Self::Url(error) => error.fmt(formatter),
            Self::NoKinds => formatter.write_str("Pick at least one event to send"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DraftTarget {
    New,
    Existing,
}

pub(super) struct ReadyWebhook {
    pub(super) hook: Webhook,
    pub(super) new_secret: Option<WebhookSecret>,
    pub(super) target: DraftTarget,
}

impl Draft {
    pub(super) fn ready(
        &self,
        issue_id: impl FnOnce() -> WebhookId,
    ) -> Result<ReadyWebhook, DraftError> {
        let name: WebhookName = self.name.parse().map_err(DraftError::Name)?;
        let url: WebhookUrl = self.url.parse().map_err(DraftError::Url)?;
        if self.kinds.is_empty() {
            return Err(DraftError::NoKinds);
        }
        let new_secret = WebhookSecret::parse(&self.secret);
        let kept_signature = self
            .editing
            .as_ref()
            .is_some_and(|hook| hook.signing == WebhookSigning::Signed);
        let signing = if new_secret.is_some() || kept_signature {
            WebhookSigning::Signed
        } else {
            WebhookSigning::Unsigned
        };
        let (id, target) = match &self.editing {
            Some(hook) => (hook.id, DraftTarget::Existing),
            None => (issue_id(), DraftTarget::New),
        };
        let hook = Webhook {
            id,
            name,
            url,
            kinds: self.kinds.clone(),
            signing,
        };
        Ok(ReadyWebhook {
            hook,
            new_secret,
            target,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEXT: WebhookId = WebhookId(7);

    fn draft(secret: &str, editing: Option<Webhook>) -> Draft {
        Draft {
            editing,
            name: " Deploys ".to_owned(),
            url: "https://hooks.example.test/krad".to_owned(),
            secret: secret.to_owned(),
            kinds: [EventKind::CiFailed].into_iter().collect(),
        }
    }

    fn ready(draft: &Draft) -> ReadyWebhook {
        draft
            .ready(|| NEXT)
            .unwrap_or_else(|error| unreachable!("{error}"))
    }

    #[test]
    fn a_new_webhook_takes_the_next_id_and_is_signed_only_with_a_secret() {
        let signed = ready(&draft(" s3cr3t ", None));
        assert_eq!(signed.hook.id, NEXT);
        assert_eq!(signed.target, DraftTarget::New);
        assert_eq!(signed.hook.name.as_str(), "Deploys");
        assert_eq!(signed.hook.signing, WebhookSigning::Signed);
        assert_eq!(
            signed.new_secret.as_ref().map(WebhookSecret::expose),
            Some("s3cr3t")
        );
        let unsigned = ready(&draft("", None));
        assert_eq!(unsigned.hook.signing, WebhookSigning::Unsigned);
        assert!(unsigned.new_secret.is_none());
    }

    #[test]
    fn editing_keeps_the_id_and_a_blank_secret_keeps_the_saved_one() {
        let saved = ready(&draft("old", None)).hook;
        let edited = ready(&draft(
            "  ",
            Some(Webhook {
                id: WebhookId(2),
                ..saved
            }),
        ));
        assert_eq!(edited.hook.id, WebhookId(2));
        assert_eq!(edited.target, DraftTarget::Existing);
        assert_eq!(edited.hook.signing, WebhookSigning::Signed);
        assert!(edited.new_secret.is_none());
    }

    #[test]
    fn an_id_is_issued_only_for_a_valid_new_webhook() {
        let mut issued = 0;
        let mut nameless = draft("", None);
        nameless.name = " ".to_owned();
        assert!(nameless
            .ready(|| {
                issued += 1;
                NEXT
            })
            .is_err());
        assert_eq!(issued, 0);
    }

    #[test]
    fn a_draft_without_its_essentials_is_explained() {
        let mut nameless = draft("", None);
        nameless.name = " ".to_owned();
        assert_eq!(
            nameless.ready(|| NEXT).err(),
            Some(DraftError::Name(WebhookNameError::Empty))
        );
        let mut plain = draft("", None);
        plain.url = "http://hooks.example.test".to_owned();
        assert_eq!(
            plain.ready(|| NEXT).err(),
            Some(DraftError::Url(HttpUrlError::PlainHttpToRemote))
        );
        let mut quiet = draft("", None);
        quiet.kinds.clear();
        assert_eq!(quiet.ready(|| NEXT).err(), Some(DraftError::NoKinds));
    }
}
