use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ureq::typestate::{WithBody, WithoutBody};
use ureq::RequestBuilder;

use super::{OpencodeError, OpencodeServer};

const LOCATION_QUERY: &str = "location[directory]";
const NOT_FOUND: u16 = 404;

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

impl OpencodeServer {
    fn url_of(&self, path: &str) -> String {
        format!("{}{path}", self.url())
    }

    fn located<B>(request: RequestBuilder<B>, directory: Option<&str>) -> RequestBuilder<B> {
        match directory {
            Some(directory) => request.query(LOCATION_QUERY, directory),
            None => request,
        }
    }

    fn without_body(&self, request: RequestBuilder<WithoutBody>) -> RequestBuilder<WithoutBody> {
        self.endpoint().sign(request)
    }

    fn with_body(&self, request: RequestBuilder<WithBody>) -> RequestBuilder<WithBody> {
        self.endpoint().sign(request)
    }

    pub(crate) fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        directory: Option<&str>,
    ) -> Result<T, OpencodeError> {
        let request = Self::located(
            self.without_body(self.agent.get(&self.url_of(path))),
            directory,
        );
        let envelope: Envelope<T> = request.call()?.body_mut().read_json()?;
        Ok(envelope.data)
    }

    pub(crate) fn read_located<T: DeserializeOwned>(
        &self,
        path: &str,
        directory: &str,
    ) -> Result<T, OpencodeError> {
        let request = Self::located(
            self.without_body(self.agent.get(&self.url_of(path))),
            Some(directory),
        );
        Ok(request.call()?.body_mut().read_json()?)
    }

    pub(crate) fn read<T: DeserializeOwned>(&self, path: &str) -> Result<T, OpencodeError> {
        let request = self.without_body(self.agent.get(&self.url_of(path)));
        Ok(request.call()?.body_mut().read_json()?)
    }

    pub(crate) fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, OpencodeError> {
        let request = self.with_body(self.agent.post(&self.url_of(path)));
        let envelope: Envelope<T> = request.send_json(body)?.body_mut().read_json()?;
        Ok(envelope.data)
    }

    pub(crate) fn send<B: Serialize>(&self, path: &str, body: &B) -> Result<(), OpencodeError> {
        self.with_body(self.agent.post(&self.url_of(path)))
            .send_json(body)?;
        Ok(())
    }

    pub(crate) fn nudge(&self, path: &str) -> Result<(), OpencodeError> {
        self.with_body(self.agent.post(&self.url_of(path)))
            .send_empty()?;
        Ok(())
    }

    pub(crate) fn delete(&self, path: &str) -> Result<(), OpencodeError> {
        self.without_body(self.agent.delete(&self.url_of(path)))
            .call()?;
        Ok(())
    }

    pub(crate) fn exists(&self, path: &str) -> Result<bool, OpencodeError> {
        match self.without_body(self.agent.get(&self.url_of(path))).call() {
            Ok(_) => Ok(true),
            Err(ureq::Error::StatusCode(NOT_FOUND)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn patch<B: Serialize>(&self, path: &str, body: &B) -> Result<(), OpencodeError> {
        self.with_body(self.agent.patch(&self.url_of(path)))
            .send_json(body)?;
        Ok(())
    }

    pub(crate) fn put<B: Serialize>(&self, path: &str, body: &B) -> Result<(), OpencodeError> {
        self.with_body(self.agent.put(&self.url_of(path)))
            .send_json(body)?;
        Ok(())
    }
}
