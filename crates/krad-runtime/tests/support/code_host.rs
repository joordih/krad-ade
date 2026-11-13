use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

const CONTENT_LENGTH: &str = "content-length:";
const QUERY_MARK: char = '?';
const LINE_END: &str = "\r\n";
const MISSING: &str = "{\"message\":\"404 Not Found\"}";
const OK_STATUS: u16 = 200;
const MISSING_STATUS: u16 = 404;

pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
    pub body: &'static str,
}

pub struct FakeCodeHost {
    pub url: String,
    address: SocketAddr,
    seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl FakeCodeHost {
    pub fn serve(routes: Vec<Route>) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (heard, stopped) = (Arc::clone(&seen), Arc::clone(&stop));
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else {
                    continue;
                };
                if let Some(line) = answer(&stream, &routes) {
                    heard
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(line);
                }
            }
        });
        Ok(Self {
            url: format!("http://{address}"),
            address,
            seen,
            stop,
        })
    }

    pub fn seen(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for FakeCodeHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        drop(TcpStream::connect(self.address));
    }
}

fn answer(stream: &TcpStream, routes: &[Route]) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let request_line = request_line.trim().to_owned();
    let mut length = 0;
    let mut header = String::new();
    while reader
        .read_line(&mut header)
        .is_ok_and(|read| read > LINE_END.len())
    {
        if let Some(value) = header.to_ascii_lowercase().strip_prefix(CONTENT_LENGTH) {
            length = value.trim().parse().unwrap_or(0);
        }
        header.clear();
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    let mut parts = request_line.split_whitespace();
    let (method, target) = (parts.next()?, parts.next()?);
    let path = target.split(QUERY_MARK).next().unwrap_or(target);
    let (status, body) = routes
        .iter()
        .find(|route| route.method == method && route.path == path)
        .map_or((MISSING_STATUS, MISSING), |route| (OK_STATUS, route.body));
    let reply = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    (&*stream).write_all(reply.as_bytes()).ok()?;
    Some(request_line)
}
