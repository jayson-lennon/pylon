use crate::{
    core::{Page, PageStore},
    discover,
    render::highlight::SyntectHighlighter,
    util, Result,
};
use anyhow::anyhow;

#[derive(Debug)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self
    }
    #[allow(clippy::unused_self)]
    pub fn render(
        &self,
        page: &Page,
        page_store: &PageStore,
        highlighter: &SyntectHighlighter,
    ) -> Result<String> {
        render(page, page_store, highlighter)
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn render(page: &Page, page_store: &PageStore, highlighter: &SyntectHighlighter) -> Result<String> {
    use pulldown_cmark::{html, CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag};

    let raw_markdown = page.raw_markdown.as_ref();
    let options = Options::all();
    let mut buf = String::new();

    // Sample implementation for working with pulldown_cmark and identifying links for rewriting
    let parser = Parser::new_ext(raw_markdown, options);

    let mut events = vec![];

    {
        let mut code_block_lang = None;

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
                Event::Start(Tag::CodeBlock(kind)) => {
                    match kind {
                        CodeBlockKind::Indented => code_block_lang = Some("".to_string()),
                        CodeBlockKind::Fenced(name) => {
                            if name == "".into() {
                                code_block_lang = Some("".to_string());
                            } else {
                                code_block_lang = Some(name.clone().to_string());
                            }
                        }
                    }
                    events.push(Event::Html("<pre><code>".into()));
                }
                Event::End(Tag::CodeBlock(_)) => {
                    events.push(Event::Html("</code></pre>".into()));
                }
                Event::Text(content) => {
                    let rendered = match code_block_lang.take() {
                        Some(lang) => {
                            render_code_block(lang.as_str(), &content, highlighter)?.join("")
                        }
                        None => content.to_string(),
                    };
                    events.push(Event::Html(rendered.into()));
                }
                Event::Code(content) => {
                    events.push(Event::Html(
                        format!("<pre><code>{content}</code></pre>").into(),
                    ));
                }
                other => {
                    dbg!(&other);
                    events.push(other);
                }
            }
        }
    }

    html::push_html(&mut buf, events.into_iter());

    Ok(buf)
}

fn render_code_block<S: AsRef<str>>(
    lang: S,
    content: S,
    highlighter: &SyntectHighlighter,
) -> Result<Vec<String>> {
    let lang = lang.as_ref();
    if lang.is_empty() {
        Ok(content.as_ref().lines().map(ToString::to_string).collect())
    } else {
        let syntax = highlighter
            .get_syntax_by_token(lang)
            .ok_or_else(|| anyhow!("unable to find theme for syntax {}", lang))?;
        dbg!(content.as_ref());
        Ok(highlighter.highlight(syntax, content))
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::all)]

    

    use crate::{
        core::{page::page::test::new_page, Page, PageStore},
        render::highlight::{syntect_highlighter::THEME_CLASS_PREFIX, SyntectHighlighter},
    };
    use regex::Regex;

    use super::MarkdownRenderer;

    fn internal_doc_link_render(test_page: Page, linked_page: Page) -> String {
        let mut store = PageStore::new();
        let key = store.insert(test_page);
        store.insert(linked_page);

        let md_renderer = MarkdownRenderer::new();
        let highlighter = SyntectHighlighter::new().unwrap();

        let test_page = store
            .get_with_key(key)
            .expect("page is missing from page store");
        let rendered_page = md_renderer
            .render(&test_page, &store, &highlighter)
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
            +++
            [internal link](@/doc.md)"#,
            "test/test.md",
            "src",
            "target",
        )
        .unwrap();

        let linked_page = new_page(
            r#"+++
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

    #[test]
    fn handles_code_fence_with_no_language_specified() {
        let page = new_page(
            r#"+++
            +++
            ```
code sample here
```
            "#,
            "test/test.md",
            "src",
            "target",
        )
        .unwrap();

        let mut store = PageStore::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered =
            super::render(page, &store, &highlighter).expect("failed to render markdown");
        assert_eq!(rendered, "<pre><code>code sample here</code></pre>");
    }

    #[test]
    fn nothing_strange_happens_with_inline_code_blocks() {
        let page = new_page(
            r#"+++
            +++
            inline `let x = 1;` code
            "#,
            "test/test.md",
            "src",
            "target",
        )
        .unwrap();

        let mut store = PageStore::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered =
            super::render(page, &store, &highlighter).expect("failed to render markdown");

        assert_eq!(
            rendered,
            "<p>inline <pre><code>let x = 1;</code></pre> code</p>\n"
        );
    }

    #[test]
    fn handles_code_fence_with_language_specified() {
        let page = new_page(
            r#"+++
            +++
            ```rust
let x = 1;
```
            "#,
            "test/test.md",
            "src",
            "target",
        )
        .unwrap();

        let mut store = PageStore::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered =
            super::render(page, &store, &highlighter).expect("failed to render markdown");
        let expected = r#"<pre><code><span class="syn-source syn-rust"><span class="syn-storage syn-type syn-rust">let</span> x <span class="syn-keyword syn-operator syn-rust">=</span> <span class="syn-constant syn-numeric syn-integer syn-decimal syn-rust">1</span><span class="syn-punctuation syn-terminator syn-rust">;</span>
</code></pre>"#.replace("syn-", THEME_CLASS_PREFIX);
        assert_eq!(rendered, expected);
    }
}
