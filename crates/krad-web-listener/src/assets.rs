include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

const INDEX: &str = "index.html";
const NOT_BUILT: &[u8] = b"<!doctype html><meta charset=utf-8><title>Krad Web</title><p>Krad Web was not built into this runtime.";
const HTML: &str = "text/html; charset=utf-8";
const CONTENT_TYPES: [(&str, &str); 13] = [
    ("html", HTML),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("json", "application/json"),
    ("webmanifest", "application/manifest+json"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("ico", "image/x-icon"),
    ("woff2", "font/woff2"),
    ("woff", "font/woff"),
    ("ttf", "font/ttf"),
    ("wasm", "application/wasm"),
];
const OCTETS: &str = "application/octet-stream";

pub(crate) struct Asset {
    pub(crate) content_type: &'static str,
    pub(crate) body: &'static [u8],
}

fn content_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    CONTENT_TYPES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(extension))
        .map_or(OCTETS, |(_, content_type)| content_type)
}

fn embedded(name: &str) -> Option<Asset> {
    WEB_ASSETS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(name, body)| Asset {
            content_type: content_type(name),
            body,
        })
}

fn index() -> Asset {
    embedded(INDEX).unwrap_or(Asset {
        content_type: HTML,
        body: NOT_BUILT,
    })
}

pub(crate) fn asset(path: &str) -> Option<Asset> {
    let name = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_start_matches('/');
    if name.is_empty() {
        return Some(index());
    }
    if name.split('/').any(|part| part == "..") {
        return None;
    }
    if let Some(found) = embedded(name) {
        return Some(found);
    }
    let last = name.rsplit('/').next().unwrap_or_default();
    (!last.contains('.')).then(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_of_the_app_fall_back_to_its_page_and_missing_files_do_not() {
        assert!(asset("/").is_some());
        assert!(asset("/sessions/12?tab=chat").is_some());
        assert!(asset("/assets/missing.js").is_none());
        assert!(asset("/../secrets").is_none());
        assert_eq!(content_type("index.HTML"), HTML);
        assert_eq!(content_type("ghostty.wasm"), "application/wasm");
    }
}
