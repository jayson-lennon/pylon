#[derive(Debug)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self
    }
    pub fn render<M: AsRef<str>>(&self, raw_markdown: M) -> String {
        do_render(raw_markdown)
    }
}

fn do_render<M: AsRef<str>>(raw_markdown: M) -> String {
    use pulldown_cmark::{html, CowStr, Event, LinkType, Options, Parser, Tag};

    let raw_markdown = raw_markdown.as_ref();
    let options = Options::all();
    let mut buf = String::new();

    let href_re = crate::util::static_regex!(r#"(^[[:alnum:]]*:.*)|(^[[:digit:]]*\..*)|(^/.*)"#);

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

fn compose_relative_path(href: &str) -> String {
    if href.is_empty() {
        String::new()
    } else {
        // TODO: make link re-writing work with anchors
        match href.as_bytes() {
            // relative internal link
            [b'@', b'.', b'/', path @ ..] => {
                let path = std::str::from_utf8(path)
                    .expect("failed to convert utf8 bytes back into &str. this is a bug");
                if path.ends_with(".md") {
                    let mut new_url = String::from(&path[..path.len() - 3]);
                    new_url.push_str(".html");
                    new_url
                } else {
                    path.to_owned()
                }
            }
            // absolute internal link
            [b'@', b'/', path @ ..] => {
                let path = std::str::from_utf8(path)
                    .expect("failed to convert utf8 bytes back into &str. this is a bug");
                let mut new_url = String::from("/");
                if path.ends_with(".md") {
                    new_url.push_str(&path[..path.len() - 3]);
                    new_url.push_str(".html");
                } else {
                    new_url.push_str(&path);
                }
                new_url
            }
            _ => href.to_owned(),
        }
    }
}
