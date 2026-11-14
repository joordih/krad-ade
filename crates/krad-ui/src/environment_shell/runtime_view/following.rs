use futures::{FutureExt as _, StreamExt as _};
use krad_client::{ClientError, Cursor, Feeds, RuntimeClient};
use krad_protocol::{Feed, Hello};
use std::sync::Arc;

const BUILD: &str = env!("CARGO_PKG_VERSION");

pub(in crate::environment_shell) enum Batch {
    Feeds(Vec<Feed>),
    Ended,
    Stopped,
}

pub(in crate::environment_shell) struct Following {
    client: Arc<dyn RuntimeClient>,
    cursor: Cursor,
    feeds: Option<Feeds>,
}

impl Following {
    pub(in crate::environment_shell) fn new(client: Arc<dyn RuntimeClient>) -> Self {
        Self::resuming(client, Cursor::default())
    }

    pub(in crate::environment_shell) fn resuming(
        client: Arc<dyn RuntimeClient>,
        cursor: Cursor,
    ) -> Self {
        Self {
            client,
            cursor,
            feeds: None,
        }
    }

    pub(in crate::environment_shell) const fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub(in crate::environment_shell) async fn next_batch(&mut self) -> Batch {
        let mut feeds = match self.feeds.take() {
            Some(feeds) => feeds,
            None => match self.connect().await {
                Ok(feeds) => feeds,
                Err(stopped) => return stopped,
            },
        };
        let Some(first) = feeds.next().await else {
            return Batch::Ended;
        };
        let mut batch = vec![first];
        while let Some(Some(more)) = feeds.next().now_or_never() {
            batch.push(more);
        }
        for feed in &batch {
            self.cursor.heard(feed);
        }
        self.feeds = Some(feeds);
        Batch::Feeds(batch)
    }

    async fn connect(&mut self) -> Result<Feeds, Batch> {
        match self.client.hello(Hello::new(BUILD)).await {
            Ok(welcome) => {
                let since = self.cursor.welcomed(&welcome);
                Ok(self.client.subscribe(since))
            }
            Err(ClientError::Stopped) => Err(Batch::Stopped),
            Err(refused @ (ClientError::Refused { .. } | ClientError::Failed { .. })) => {
                tracing::warn!(%refused, "the runtime refused this window");
                Err(Batch::Stopped)
            }
        }
    }
}

#[cfg(test)]
mod tests;
