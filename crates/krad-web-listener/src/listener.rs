use krad_runner::{Listener, RuntimeServer, Transport};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use crate::config::{read_web_access, WebAccess, WebAccessError};
use crate::guard::Guard;
use crate::push::{watch, Push};
use crate::routes::Routes;
use crate::socket::WebStream;
use crate::stopping::Stopping;
use crate::tls::{Certificates, Wire};

const ACCEPT_THREAD: &str = "krad-web-accept";
const CONNECTION_THREAD: &str = "krad-web-connection";
const SERVE_THREAD: &str = "krad-web-serve";
const MAX_CONNECTIONS: usize = 64;
const PENDING_STREAMS: usize = 16;
const ACCEPT_STOPPED: &str = "web access stopped accepting connections";

pub struct WebListener {
    streams: Mutex<Receiver<WebStream>>,
    stopping: Stopping,
    pub address: SocketAddr,
}

impl Listener for WebListener {
    type Stream = WebStream;

    fn accept(&self) -> io::Result<WebStream> {
        let stream = self
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, ACCEPT_STOPPED))?;
        if self.stopping.stopped() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, ACCEPT_STOPPED));
        }
        Ok(stream)
    }
}

#[must_use]
pub struct ServedWebAccess {
    pub address: SocketAddr,
    stopping: Stopping,
}

impl ServedWebAccess {
    pub fn stop(self) {
        self.stopping.stop();
        if let Err(error) = TcpStream::connect(self.address) {
            tracing::debug!(%error, "web access had already stopped accepting");
        }
        tracing::info!(address = %self.address, "web access stopped");
    }
}

struct Acceptor {
    routes: Arc<Routes>,
    certificates: Option<Certificates>,
    open: Arc<AtomicUsize>,
    stopping: Stopping,
}

struct Admitted(Arc<AtomicUsize>);

impl Drop for Admitted {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Acceptor {
    fn wire(&self, socket: TcpStream) -> io::Result<Wire> {
        match &self.certificates {
            Some(certificates) => certificates.wrap(socket),
            None => Ok(Wire::Plain(socket)),
        }
    }

    fn take(&self, socket: TcpStream) {
        if self.open.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            self.open.fetch_sub(1, Ordering::AcqRel);
            tracing::debug!("web access is at its connection limit");
            drop(socket);
            return;
        }
        let admitted = Admitted(Arc::clone(&self.open));
        let wire = match self.wire(socket) {
            Ok(wire) => wire,
            Err(error) => {
                tracing::debug!(%error, "a web connection could not start");
                return;
            }
        };
        let routes = Arc::clone(&self.routes);
        let spawned = thread::Builder::new()
            .name(CONNECTION_THREAD.to_owned())
            .spawn(move || {
                let _admitted = admitted;
                routes.serve(wire);
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not serve a web connection");
        }
    }

    fn run(&self, listener: &TcpListener) {
        for socket in listener.incoming() {
            if self.stopping.stopped() {
                return;
            }
            match socket {
                Ok(socket) => self.take(socket),
                Err(error) => tracing::debug!(%error, "a web connection was not accepted"),
            }
        }
    }
}

fn start_push(server: &Arc<RuntimeServer>, stopping: &Stopping) -> Option<Arc<Push>> {
    let push = match Push::open(server.root()) {
        Ok(push) => Arc::new(push),
        Err(error) => {
            tracing::warn!(%error, "push notifications are off");
            return None;
        }
    };
    if let Err(error) = watch(server, Arc::clone(&push), stopping) {
        tracing::warn!(%error, "push notifications could not watch the runtime");
        return None;
    }
    Some(push)
}

/// # Errors
///
/// Fails when the settings are refused, the certificate cannot be loaded or the address cannot
/// be bound.
pub fn bind_web_access(
    access: &WebAccess,
    server: &Arc<RuntimeServer>,
) -> Result<WebListener, WebAccessError> {
    access.check()?;
    let certificates = access.tls.clone().map(Certificates::load).transpose()?;
    let socket = TcpListener::bind(access.bind)?;
    let address = socket.local_addr()?;
    let (sender, streams) = mpsc::sync_channel(PENDING_STREAMS);
    let stopping = Stopping::default();
    let push = start_push(server, &stopping);
    let acceptor = Acceptor {
        routes: Arc::new(Routes {
            server: Arc::clone(server),
            guard: Guard::new(address, &access.names, access.secure()),
            secure: access.secure(),
            streams: sender,
            push,
            stopping: stopping.clone(),
        }),
        certificates,
        open: Arc::new(AtomicUsize::new(0)),
        stopping: stopping.clone(),
    };
    thread::Builder::new()
        .name(ACCEPT_THREAD.to_owned())
        .spawn(move || acceptor.run(&socket))?;
    Ok(WebListener {
        streams: Mutex::new(streams),
        stopping,
        address,
    })
}

/// # Errors
///
/// Fails when the settings are refused, the certificate cannot be loaded, the address cannot be
/// bound or the serving thread cannot start.
pub fn serve_web_access(
    access: &WebAccess,
    server: &Arc<RuntimeServer>,
) -> Result<ServedWebAccess, WebAccessError> {
    let listener = bind_web_access(access, server)?;
    let handle = ServedWebAccess {
        address: listener.address,
        stopping: listener.stopping.clone(),
    };
    let server = Arc::clone(server);
    thread::Builder::new()
        .name(SERVE_THREAD.to_owned())
        .spawn(move || {
            while let Ok(stream) = listener.accept() {
                server.serve_stream(stream);
            }
        })?;
    tracing::info!(address = %handle.address, secure = access.secure(), "web access listening");
    Ok(handle)
}

/// # Errors
///
/// Fails when the runtime's web access settings are invalid or cannot be served.
pub fn start_web_access(
    root: &Path,
    server: &Arc<RuntimeServer>,
) -> Result<Option<SocketAddr>, WebAccessError> {
    let Some(access) = read_web_access(root)? else {
        tracing::debug!("web access is off");
        return Ok(None);
    };
    Ok(Some(serve_web_access(&access, server)?.address))
}

pub fn web_transport() -> Transport {
    Box::new(|root, server| {
        if let Err(error) = start_web_access(root, server) {
            tracing::warn!(%error, "web access did not start");
        }
    })
}
