use crate::core::PageStore;

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

fn render<M: AsRef<str>>(raw_markdown: M, page_store: &PageStore) -> String {
    use pulldown_cmark::{html, Options, Parser};

    let raw_markdown = raw_markdown.as_ref();
    let options = Options::all();
    let mut buf = String::new();

    // Sample implementation for working with pulldown_cmark and identifying links for rewriting
    //
    // let _href_re = crate::util::static_regex!(r#"(^[[:alnum:]]*:.*)|(^[[:digit:]]*\..*)|(^/.*)"#);
    //
    // let parser = Parser::new_ext(raw_markdown, options).map(|ev| match ev {
    //     Event::Start(Tag::Link(LinkType::Inline, href, title)) => {
    //         if href_re.is_match(&href) {
    //             Event::Start(Tag::Link(LinkType::Inline, href, title))
    //         } else {
    //             let new_href = compose_relative_path(&href);
    //             Event::Start(Tag::Link(
    //                 LinkType::Inline,
    //                 CowStr::Boxed(new_href.into_boxed_str()),
    //                 title,
    //             ))
    //         }
    //     }
    //     _ => ev,
    // });
    let parser = Parser::new_ext(raw_markdown, options);

    html::push_html(&mut buf, parser);

    buf
}
