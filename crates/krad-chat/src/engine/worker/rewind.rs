use krad_domain::RewindPoint;

use super::super::session::{read_records, restore, Record};
use super::{Phase, Worker};
use crate::control::LiveEvent;
use crate::rewind::{RewindError, RewindRequest, Rewound};

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn rewind(&mut self, request: &RewindRequest) {
        let outcome = self.cut_back(request);
        (self.sink)(LiveEvent::Rewound(outcome));
    }

    fn cut_back(&mut self, request: &RewindRequest) -> Result<Rewound, RewindError> {
        let RewindPoint::Prompt(prompt) = request.point else {
            return Err(RewindError::UnknownPoint);
        };
        if !matches!(self.phase, Phase::Idle) {
            return Err(RewindError::Busy);
        }
        if prompt >= self.prompts {
            return Err(RewindError::MissingPrompt);
        }
        if request.scope.restores_files() {
            self.snapshots.restore_from(prompt)?;
        }
        self.session.append(&Record::Rewound { prompt })?;
        let restored = restore(read_records(&self.session.path)?);
        self.messages = restored.messages;
        self.prompts = restored.prompts;
        self.turn = None;
        self.context = 0;
        self.queued.clear();
        self.snapshots.forget_from(prompt)?;
        Ok(Rewound::InPlace)
    }
}
