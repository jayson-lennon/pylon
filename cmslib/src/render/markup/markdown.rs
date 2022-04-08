use crate::core::{Page, PageStore, Uri};
use anyhow::anyhow;

#[derive(Debug)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self
    }
    #[allow(clippy::unused_self)]
    pub fn render(&self, page: &Page, page_store: &PageStore) -> Result<String, anyhow::Error> {
        render(page, page_store)
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
enum CustomHref {
    InternalLink(Uri),
}

fn render(page: &Page, page_store: &PageStore) -> Result<String, anyhow::Error> {
    use pulldown_cmark::{html, CowStr, Event, LinkType, Options, Parser, Tag};

    let raw_markdown = page.raw_markdown.as_ref();
    let options = Options::all();
    let mut buf = String::new();

    // Sample implementation for working with pulldown_cmark and identifying links for rewriting
    let parser = Parser::new_ext(raw_markdown, options);

    let mut events = vec![];

    for event in parser {
        match event {
            Event::Start(Tag::Link(LinkType::Inline, href, title)) => {
                if let Some(custom) = get_custom_href(&href) {
                    match custom {
                        CustomHref::InternalLink(ref uri) => {
                            let page = page_store.get(uri).ok_or_else(|| {
                                anyhow!(
                                    "unable to find internal link '{}' on page '{}'",
                                    &uri,
                                    page.uri()
                                )
                            })?;
                            events.push(Event::Start(Tag::Link(
                                LinkType::Inline,
                                CowStr::Boxed(page.uri.into_boxed_str()),
                                title,
                            )));
                        }
                    }
                } else {
                    events.push(Event::Start(Tag::Link(LinkType::Inline, href, title)));
                }
            }
            other => events.push(other),
        }
    }

    html::push_html(&mut buf, events.into_iter());

    Ok(buf)
}

fn get_custom_href<S: AsRef<str>>(href: S) -> Option<CustomHref> {
    use std::str::from_utf8;
    match href.as_ref().as_bytes() {
        [b'@', b'/', uri @ ..] => Some(CustomHref::InternalLink(Uri::from_path(
            from_utf8(uri).unwrap(),
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::all)]

    use crate::core::{page::test::page_from_doc_with_paths, Uri};
    use crate::core::{Page, PageStore};
    use regex::Regex;

    use super::{CustomHref, MarkdownRenderer};

    #[test]
    fn identifies_internal_link() {
        let internal_link = "@/some/path/page.md";
        let href = super::get_custom_href(internal_link).unwrap();
        match href {
            CustomHref::InternalLink(uri) => assert_eq!(uri, Uri::from_path("some/path/page.md")),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    fn internal_doc_link_render(test_page: Page, linked_page: Page) -> String {
        let mut store = PageStore::new();
        let key = store.insert(test_page);
        store.insert(linked_page);

        let renderer = MarkdownRenderer::new();

        let test_page = store
            .get_with_key(key)
            .expect("page is missing from page store");
        let rendered = renderer
            .render(&test_page, &store)
            .expect("failed to render test page");
        rendered
    }

    fn get_href_attr(rendered: &str) -> String {
        let re = Regex::new(r#"href="(.*)""#).unwrap();
        let capture = re
            .captures_iter(&rendered)
            .next()
            .expect("missing href attribute on link");
        capture[1].to_string()
    }

    #[test]
    fn internal_doc_link_use_index() {
        let test_page = page_from_doc_with_paths(
            r#"+++
            template_name = "content_only.tera"
            +++
            [internal link](@/test/doc.md)"#,
            "src",
            "target",
            "test/test.md",
        )
        .unwrap();

        let linked_page = page_from_doc_with_paths(
            r#"+++
            use_index = true
            template_name = "empty.tera"
            +++"#,
            "src",
            "target",
            "test/doc.md",
        )
        .unwrap();

        let rendered = internal_doc_link_render(test_page, linked_page);
        let href = get_href_attr(&rendered);

        assert_eq!(href, "/test/doc");
    }

    #[test]
    fn internal_doc_link_use_index_root() {
        let test_page = page_from_doc_with_paths(
            r#"+++
            template_name = "content_only.tera"
            +++
            [internal link](@/doc.md)"#,
            "src",
            "target",
            "test/test.md",
        )
        .unwrap();

        let linked_page = page_from_doc_with_paths(
            r#"+++
            use_index = true
            template_name = "empty.tera"
            +++"#,
            "src",
            "target",
            "doc.md",
        )
        .unwrap();

        let rendered = internal_doc_link_render(test_page, linked_page);
        let href = get_href_attr(&rendered);

        assert_eq!(href, "/doc");
    }

    #[test]
    fn internal_doc_link_no_index() {
        let test_page = page_from_doc_with_paths(
            r#"+++
            template_name = "content_only.tera"
            +++
            [internal link](@/test/doc.md)"#,
            "src",
            "target",
            "test/test.md",
        )
        .unwrap();

        let linked_page = page_from_doc_with_paths(
            r#"+++
            use_index = false
            template_name = "empty.tera"
            +++"#,
            "src",
            "target",
            "test/doc.md",
        )
        .unwrap();

        let rendered = internal_doc_link_render(test_page, linked_page);
        let href = get_href_attr(&rendered);

        assert_eq!(href, "/test/doc.html");
    }

    #[test]
    fn internal_doc_link_no_index_root() {
        let test_page = page_from_doc_with_paths(
            r#"+++
            template_name = "content_only.tera"
            +++
            [internal link](@/doc.md)"#,
            "src",
            "target",
            "test/test.md",
        )
        .unwrap();

        let linked_page = page_from_doc_with_paths(
            r#"+++
            use_index = false
            template_name = "empty.tera"
            +++"#,
            "src",
            "target",
            "doc.md",
        )
        .unwrap();

        let rendered = internal_doc_link_render(test_page, linked_page);
        let href = get_href_attr(&rendered);

        assert_eq!(href, "/doc.html");
    }
}
