use std::path::PathBuf;

use crate::{
    core::{Page, PageStore, RelSystemPath, Uri},
    Result,
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

#[derive(Debug, Clone)]
enum HrefType {
    Offsite,
    Absolute,
    Relative(String),
    InternalDoc(Uri),
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
                match get_href_target(&href) {
                    // internal doc links get converted into target Uri
                    HrefType::InternalDoc(ref uri) => {
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
                    HrefType::Absolute | HrefType::Offsite => {
                        events.push(Event::Start(Tag::Link(LinkType::Inline, href, title)));
                    }
                    // relative links need to get converted to absolute links
                    HrefType::Relative(target) => {
                        let target = build_absolute_target(&target, &page.src_path);
                        events.push(Event::Start(Tag::Link(
                            LinkType::Inline,
                            CowStr::Boxed(target),
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

fn build_absolute_target<S: AsRef<str>>(
    relative_target: S,
    from_page_path: &RelSystemPath,
) -> Box<str> {
    let mut abs_path = PathBuf::from("/");
    abs_path.push(from_page_path.with_base("").to_path_buf().parent().unwrap());
    abs_path.push(&relative_target.as_ref());
    abs_path.to_string_lossy().to_string().into_boxed_str()
}

fn get_href_target<S: AsRef<str>>(href: S) -> HrefType {
    use std::str::from_utf8;
    match href.as_ref().as_bytes() {
        // Internal doc: @/
        [b'@', b'/', target @ ..] => {
            HrefType::InternalDoc(Uri::from_path(from_utf8(target).unwrap()))
        }
        // Absolute: /
        [b'/', ..] => HrefType::Absolute,
        // Relative: ./
        [b'.', b'/', target @ ..] => HrefType::Relative(from_utf8(target).unwrap().to_owned()),
        [..] => HrefType::Offsite,
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::all)]

    use crate::core::{
        page::page::test::page_from_doc_with_paths, Page, PageStore, RelSystemPath, Uri,
    };
    use regex::Regex;

    use super::{HrefType, MarkdownRenderer};

    #[test]
    fn builds_absolute_target() {
        let rel_target = "some/resource.txt";
        let page_path = RelSystemPath::new("src", "1/2/3.md");
        let abs_target = super::build_absolute_target(rel_target, &page_path);
        assert_eq!(&*abs_target, "/1/2/some/resource.txt");
    }

    #[test]
    fn builds_absolute_target_when_at_root() {
        let rel_target = "resource.txt";
        let page_path = RelSystemPath::new("src", "page.md");
        let abs_target = super::build_absolute_target(rel_target, &page_path);
        assert_eq!(&*abs_target, "/resource.txt");
    }

    #[test]
    fn get_href_target_identifies_internal_doc() {
        let internal_link = "@/some/path/page.md";
        let href = super::get_href_target(internal_link);
        match href {
            HrefType::InternalDoc(uri) => assert_eq!(uri, Uri::from_path("some/path/page.md")),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_href_target_identifies_absolute_target() {
        let abs_target = "/some/path/page.md";
        let href = super::get_href_target(abs_target);
        match href {
            HrefType::Absolute => (),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_href_target_identifies_relative_target() {
        let rel_target = "./some/path/page.md";
        let href = super::get_href_target(rel_target);
        match href {
            HrefType::Relative(target) => assert_eq!(target, "some/path/page.md"),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_href_target_identifies_offsite_target() {
        let offsite_target = "http://example.com";
        let href = super::get_href_target(offsite_target);
        match href {
            HrefType::Offsite => (),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_href_target_identifies_offsite_target_without_protocol() {
        let offsite_target = "example.com";
        let href = super::get_href_target(offsite_target);
        match href {
            HrefType::Offsite => (),
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
