use krad_session::{PumpMsg, PumpSink};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};

use super::terminal::Generation;
use crate::input::Input;

const PUMP_CAPACITY: usize = 256;

pub(crate) struct Pumped {
    pub(crate) generation: Generation,
    pub(crate) msg: PumpMsg,
}

#[derive(Clone, Debug)]
pub(crate) struct PumpQueue {
    pumped: SyncSender<Pumped>,
    wake: SyncSender<Input>,
}

impl PumpQueue {
    pub(crate) fn open(wake: SyncSender<Input>) -> (Self, Receiver<Pumped>) {
        let (pumped, received) = sync_channel(PUMP_CAPACITY);
        (Self { pumped, wake }, received)
    }

    pub(crate) fn of(&self, generation: Generation) -> JobPump {
        JobPump {
            queue: self.clone(),
            generation,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct JobPump {
    queue: PumpQueue,
    generation: Generation,
}

impl PumpSink for JobPump {
    fn deliver(&mut self, msg: PumpMsg) -> bool {
        let pumped = Pumped {
            generation: self.generation,
            msg,
        };
        if self.queue.pumped.send(pumped).is_err() {
            return false;
        }
        !matches!(
            self.queue.wake.try_send(Input::Wake),
            Err(TrySendError::Disconnected(_))
        )
    }
}
