use crate::core::{PageStore, Uri};

#[derive(Debug)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self
    }
    #[allow(clippy::unused_self)]
    pub fn render<M: AsRef<str>>(&self, raw_markdown: M, page_store: &PageStore) -> String {
        render(raw_markdown, page_store)
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
enum CustomHref {
    Internal(Uri),
}

impl CustomHref {
    pub fn into_boxed_str(self) -> Box<str> {
        match self {
            Self::Internal(uri) => uri.to_string().into_boxed_str(),
        }
    }
}

fn render<M: AsRef<str>>(raw_markdown: M, page_store: &PageStore) -> String {
    use pulldown_cmark::{html, CowStr, Event, LinkType, Options, Parser, Tag};

    let raw_markdown = raw_markdown.as_ref();
    let options = Options::all();
    let mut buf = String::new();

    // Sample implementation for working with pulldown_cmark and identifying links for rewriting
    let parser = Parser::new_ext(raw_markdown, options).map(|ev| match ev {
        Event::Start(Tag::Link(LinkType::Inline, href, title)) => {
            if let Some(custom) = get_custom_href(&href) {
                Event::Start(Tag::Link(
                    LinkType::Inline,
                    CowStr::Boxed(custom.into_boxed_str()),
                    title,
                ))
            } else {
                Event::Start(Tag::Link(LinkType::Inline, href, title))
            }
        }
        _ => ev,
    });
    let parser = Parser::new_ext(raw_markdown, options);

    html::push_html(&mut buf, parser);

    buf
}

fn get_custom_href(href: &str) -> Option<CustomHref> {
    match href.as_bytes() {
        [b'@', b'/', uri] => Some(CustomHref::Internal(Uri::from_path(uri.to_string()))),
        _ => None,
    }
}
