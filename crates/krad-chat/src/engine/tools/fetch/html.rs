use super::url::WebUrl;

const SKIPPED_TAGS: [&str; 7] = [
    "script", "style", "noscript", "svg", "template", "iframe", "canvas",
];
const BLOCK_TAGS: [&str; 26] = [
    "p",
    "div",
    "br",
    "ul",
    "ol",
    "tr",
    "table",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "main",
    "aside",
    "pre",
    "blockquote",
    "hr",
    "form",
    "dl",
    "dt",
    "dd",
    "figure",
    "figcaption",
    "title",
    "body",
    "li",
];
const CELL_TAGS: [&str; 2] = ["td", "th"];
const HEADING_TAGS: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];
const LINK_TAG: &str = "a";
const LINK_OPEN: &str = " (";
const LINK_CLOSE: char = ')';
const ITEM_TAG: &str = "li";
const ITEM_MARK: &str = "- ";
const HEADING_MARK: char = '#';
const COMMENT_OPEN: &str = "<!--";
const COMMENT_CLOSE: &str = "-->";
const TAG_OPEN: char = '<';
const TAG_CLOSE: char = '>';
const CLOSING_MARK: char = '/';
const HREF_ATTRIBUTE: &str = "href=";
const QUOTES: [char; 2] = ['"', '\''];
const ENTITY_OPEN: char = '&';
const ENTITY_CLOSE: char = ';';
const NUMERIC_MARK: char = '#';
const HEX_MARKS: [char; 2] = ['x', 'X'];
const HEX_RADIX: u32 = 16;
const DECIMAL_RADIX: u32 = 10;
const LONGEST_ENTITY: usize = 10;
const SKIPPED_LINKS: [&str; 4] = ["#", "javascript:", "mailto:", "data:"];
const NAMED_ENTITIES: [(&str, &str); 16] = [
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", " "),
    ("mdash", "—"),
    ("ndash", "–"),
    ("hellip", "…"),
    ("copy", "©"),
    ("reg", "®"),
    ("lsquo", "‘"),
    ("rsquo", "’"),
    ("ldquo", "“"),
    ("rdquo", "”"),
    ("middot", "·"),
];

struct Link {
    href: String,
    start: usize,
}

fn entity(name: &str) -> Option<String> {
    if let Some(number) = name.strip_prefix(NUMERIC_MARK) {
        let code = match number.strip_prefix(HEX_MARKS) {
            Some(hex) => u32::from_str_radix(hex, HEX_RADIX).ok()?,
            None => u32::from_str_radix(number, DECIMAL_RADIX).ok()?,
        };
        return char::from_u32(code).map(String::from);
    }
    NAMED_ENTITIES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, text)| (*text).to_owned())
}

pub(super) fn decode(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(ENTITY_OPEN) {
        decoded.push_str(&rest[..at]);
        rest = &rest[at..];
        let named = rest[1..]
            .find(ENTITY_CLOSE)
            .filter(|end| *end <= LONGEST_ENTITY)
            .and_then(|end| Some((entity(&rest[1..=end])?, end + 2)));
        if let Some((text, used)) = named {
            decoded.push_str(&text);
            rest = &rest[used..];
        } else {
            decoded.push(ENTITY_OPEN);
            rest = &rest[ENTITY_OPEN.len_utf8()..];
        }
    }
    decoded.push_str(rest);
    decoded
}

fn push_text(out: &mut String, text: &str) {
    let decoded = decode(text);
    out.extend(
        decoded
            .chars()
            .map(|char| if char.is_whitespace() { ' ' } else { char }),
    );
}

fn tag_name(tag: &str) -> String {
    tag.trim_start_matches(CLOSING_MARK)
        .split(|char: char| char.is_whitespace() || char == CLOSING_MARK)
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn skip_element<'a>(rest: &'a str, name: &str) -> &'a str {
    let closing = format!("{TAG_OPEN}{CLOSING_MARK}{name}");
    let Some(at) = rest.to_ascii_lowercase().find(&closing) else {
        return "";
    };
    let after = &rest[at..];
    after
        .find(TAG_CLOSE)
        .map_or("", |end| &after[end + TAG_CLOSE.len_utf8()..])
}

fn href(tag: &str, base: &WebUrl) -> Option<String> {
    let at = tag.to_ascii_lowercase().find(HREF_ATTRIBUTE)?;
    let value = &tag[at + HREF_ATTRIBUTE.len()..];
    let value = match value.chars().next() {
        Some(quote) if QUOTES.contains(&quote) => value[1..].split(quote).next()?,
        _ => value.split_whitespace().next()?,
    };
    let value = decode(value.trim());
    if value.is_empty()
        || SKIPPED_LINKS
            .iter()
            .any(|skipped| value.to_ascii_lowercase().starts_with(skipped))
    {
        return None;
    }
    base.join(&value).ok().map(|url| url.to_string())
}

fn close_link(out: &mut String, link: &Link) {
    let text = out[link.start..].trim();
    if text.is_empty() || text == link.href {
        return;
    }
    out.push_str(LINK_OPEN);
    out.push_str(&link.href);
    out.push(LINK_CLOSE);
}

fn collapse(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        if line.starts_with(HEADING_MARK) && !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(line);
    }
    lines.join("\n")
}

pub(super) fn readable(html: &str, base: &WebUrl) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut link: Option<Link> = None;
    let mut rest = html;
    while let Some(at) = rest.find(TAG_OPEN) {
        push_text(&mut out, &rest[..at]);
        rest = &rest[at..];
        if rest.starts_with(COMMENT_OPEN) {
            rest = rest
                .find(COMMENT_CLOSE)
                .map_or("", |end| &rest[end + COMMENT_CLOSE.len()..]);
            continue;
        }
        let Some(end) = rest.find(TAG_CLOSE) else {
            rest = "";
            break;
        };
        let tag = &rest[TAG_OPEN.len_utf8()..end];
        rest = &rest[end + TAG_CLOSE.len_utf8()..];
        let closing = tag.starts_with(CLOSING_MARK);
        let name = tag_name(tag);
        if !closing && SKIPPED_TAGS.contains(&name.as_str()) {
            rest = skip_element(rest, &name);
            continue;
        }
        if name == LINK_TAG {
            match (closing, link.take()) {
                (true, Some(open)) => close_link(&mut out, &open),
                (false, _) => {
                    link = href(tag, base).map(|href| Link {
                        href,
                        start: out.len(),
                    });
                }
                (true, None) => {}
            }
            continue;
        }
        if let Some(level) = HEADING_TAGS.iter().position(|heading| *heading == name) {
            out.push('\n');
            if !closing {
                out.extend(std::iter::repeat_n(HEADING_MARK, level + 1));
                out.push(' ');
            }
            continue;
        }
        if BLOCK_TAGS.contains(&name.as_str()) {
            out.push('\n');
            if name == ITEM_TAG && !closing {
                out.push_str(ITEM_MARK);
            }
        } else if CELL_TAGS.contains(&name.as_str()) {
            out.push(' ');
        }
    }
    push_text(&mut out, rest);
    collapse(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> WebUrl {
        "https://x.org/docs/index.html"
            .parse()
            .unwrap_or_else(|_| unreachable!("the base address parses"))
    }

    #[test]
    fn markup_scripts_and_styles_go_and_text_stays_readable() {
        let html = "<html><head><title>Guide</title><style>p{color:red}</style>\
            <script>alert('x < y')</script></head><body><!-- note -->\
            <h1>Intro</h1><p>Fish &amp; chips&nbsp;&#8212; &#x41;   ok</p>\
            <ul><li>one</li><li>two</li></ul></body></html>";
        assert_eq!(
            readable(html, &base()),
            "Guide\n\n# Intro\nFish & chips — A ok\n- one\n- two"
        );
    }

    #[test]
    fn links_keep_their_resolved_address_after_the_text() {
        let html = r##"<p>See <a href="/api">the API</a>, <a href='next.html'>next</a>
            and <a href="#top">top</a>.</p>"##;
        assert_eq!(
            readable(html, &base()),
            "See the API (https://x.org/api), next (https://x.org/docs/next.html) and top."
        );
    }

    #[test]
    fn unknown_entities_and_stray_ampersands_survive() {
        assert_eq!(decode("a & b &bogus; &lt;c&gt;"), "a & b &bogus; <c>");
    }
}
