use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::FutureExt as _;
use gpui_kit::http_client::http::HeaderValue;
use gpui_kit::http_client::{
    anyhow, AsyncBody, HttpClient, Method, Request, Response, Result as HttpResult, Url,
};
use std::sync::Arc;
use std::thread;

const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;
const FETCH_THREAD_NAME: &str = "krad-http-fetch";
const ONLY_GET: &str = "only GET requests are supported";
const FETCH_DROPPED: &str = "the download thread stopped before answering";

struct UreqClient {
    agent: ureq::Agent,
}

#[must_use]
pub fn http_client() -> Arc<dyn HttpClient> {
    Arc::new(UreqClient {
        agent: ureq::Agent::config_builder().build().into(),
    })
}

fn fetch(agent: &ureq::Agent, uri: &str) -> HttpResult<Response<AsyncBody>> {
    let mut response = agent.get(uri).call()?;
    let status = response.status();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_vec()?;
    Ok(Response::builder()
        .status(status.as_u16())
        .body(AsyncBody::from(body))?)
}

impl HttpClient for UreqClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, HttpResult<Response<AsyncBody>>> {
        if request.method() != Method::GET {
            return async { Err(anyhow!(ONLY_GET)) }.boxed();
        }
        let uri = request.uri().to_string();
        let agent = self.agent.clone();
        let (answer, answered) = oneshot::channel();
        let spawned = thread::Builder::new()
            .name(FETCH_THREAD_NAME.to_owned())
            .spawn(move || {
                drop(answer.send(fetch(&agent, &uri)));
            });
        async move {
            spawned?;
            answered.await.map_err(|_| anyhow!(FETCH_DROPPED))?
        }
        .boxed()
    }
}
