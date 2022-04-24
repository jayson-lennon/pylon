use crate::{
    core::{Page, PageStore},
    discover, util, Result,
};
use anyhow::anyhow;

#[derive(Debug)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self
    }
    #[allow(clippy::unused_self)]
    pub fn render(&self, page: &Page, page_store: &PageStore) -> Result<String> {
        render(page, page_store)
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn render(page: &Page, page_store: &PageStore) -> Result<String> {
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
                use discover::UrlType;
                match discover::get_url_type(&href) {
                    // internal doc links get converted into target Uri
                    UrlType::InternalDoc(ref uri) => {
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
                    // no changes needed for absolute targets or offsite targets
                    UrlType::Absolute | UrlType::Offsite => {
                        events.push(Event::Start(Tag::Link(LinkType::Inline, href, title)));
                    }
                    // relative links need to get converted to absolute links
                    UrlType::Relative(target) => {
                        let target = util::rel_to_abs(&target, &page.src_path);
                        events.push(Event::Start(Tag::Link(
                            LinkType::Inline,
                            CowStr::Boxed(target.into_boxed_str()),
                            title,
                        )));
                    }
                }
            }
            other => events.push(other),
        }
    }

    html::push_html(&mut buf, events.into_iter());

    Ok(buf)
}

#[cfg(test)]
mod test {
    #![allow(clippy::all)]

    use crate::core::{page::page::test::new_page, Page, PageStore};
    use regex::Regex;

    use super::MarkdownRenderer;

    fn internal_doc_link_render(test_page: Page, linked_page: Page) -> String {
        let mut store = PageStore::new();
        let key = store.insert(test_page);
        store.insert(linked_page);

        let renderer = MarkdownRenderer::new();

        let test_page = store
            .get_with_key(key)
            .expect("page is missing from page store");
        let rendered_page = renderer
            .render(&test_page, &store)
            .expect("failed to render test page");
        rendered_page
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
    fn internal_doc_link() {
        let test_page = new_page(
            r#"+++
            template_name = "content_only.tera"
            +++
            [internal link](@/test/doc.md)"#,
            "test/test.md",
            "src",
            "target",
        )
        .unwrap();

        let linked_page = new_page(
            r#"+++
            template_name = "empty.tera"
            +++"#,
            "test/doc.md",
            "src",
            "target",
        )
        .unwrap();

        let rendered = internal_doc_link_render(test_page, linked_page);
        let href = get_href_attr(&rendered);

        assert_eq!(href, "/test/doc.html");
    }

    #[test]
    fn internal_doc_link_at_root() {
        let test_page = new_page(
            r#"+++
            template_name = "content_only.tera"
            +++
            [internal link](@/doc.md)"#,
            "test/test.md",
            "src",
            "target",
        )
        .unwrap();

        let linked_page = new_page(
            r#"+++
            template_name = "empty.tera"
            +++"#,
            "doc.md",
            "src",
            "target",
        )
        .unwrap();

        let rendered = internal_doc_link_render(test_page, linked_page);
        let href = get_href_attr(&rendered);

        assert_eq!(href, "/doc.html");
    }
}
