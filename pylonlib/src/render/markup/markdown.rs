use crate::{
    core::{page::RawMarkdown, Library, Page},
    discover,
    render::highlight::SyntectHighlighter,
    Result,
};

use eyre::eyre;

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
        library: &Library,
        highlighter: &SyntectHighlighter,
        raw_markdown: &RawMarkdown,
    ) -> Result<String> {
        render(page, library, highlighter, raw_markdown)
    }

    #[allow(clippy::unused_self)]
    pub fn render_toc(&self, page: &Page) -> String {
        use pulldown_cmark_toc::TableOfContents;

        let toc_options = pulldown_cmark_toc::Options::default();
        let md_toc = TableOfContents::new(&page.raw_markdown).to_cmark_with_options(toc_options);

        let parser = pulldown_cmark::Parser::new(&md_toc);

        let mut rendered = String::new();
        pulldown_cmark::html::push_html(&mut rendered, parser);
        rendered
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::too_many_lines)]
fn render(
    page: &Page,
    library: &Library,
    highlighter: &SyntectHighlighter,
    raw_markdown: &RawMarkdown,
) -> Result<String> {
    use pulldown_cmark::{
        html, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Options, Parser, Tag,
    };

    let options = Options::all();
    let mut buf = String::new();

    // Sample implementation for working with pulldown_cmark and identifying links for rewriting
    let parser = Parser::new_ext(raw_markdown, options);

    let mut events = vec![];

    {
        let mut code_block_lang: Option<String> = None;

        let mut heading_level: Option<HeadingLevel> = None;

        for event in parser {
            match event {
                Event::Start(Tag::FootnoteDefinition(ref s)) => {
                    let content = format!("<div class=\"footnote-definition\" id=\"{}\"><span class=\"footnote-definition-label\">{}</span>",s, s);
                    events.push(Event::Html(content.into()));
                }
                Event::End(Tag::FootnoteDefinition(_)) => {
                    events.push(Event::Html("</div>".into()));
                }
                Event::Start(Tag::Link(LinkType::Inline, href, title)) => {
                    use discover::UrlType;
                    match discover::get_url_type(&href) {
                        // internal doc links get converted into target Uri
                        UrlType::InternalDoc(ref target) => {
                            let page = library.get(&target.into()).ok_or_else(|| {
                                eyre!(
                                    "unable to find internal link '{}' on page '{}'",
                                    &target,
                                    page.uri()
                                )
                            })?;
                            events.push(Event::Start(Tag::Link(
                                LinkType::Inline,
                                CowStr::Boxed(page.uri().into_boxed_str()),
                                title,
                            )));
                        }
                        // no changes needed for absolute targets or offsite targets
                        UrlType::Absolute | UrlType::Offsite => {
                            events.push(Event::Start(Tag::Link(LinkType::Inline, href, title)));
                        }
                        // relative links need to get converted to absolute links
                        UrlType::Relative(uri) => {
                            let uri = crate::util::based_uri_from_sys_path(&page.target(), uri)?;
                            events.push(Event::Start(Tag::Link(
                                LinkType::Inline,
                                CowStr::Boxed(uri.into_boxed_str()),
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
                    if let Some(heading_level) = heading_level {
                        let id = dashify(&content);
                        match heading_level {
                            HeadingLevel::H1 => events.push(Event::Html(
                                format!("<h1 id=\"{id}\">{content}</h1>").into(),
                            )),
                            HeadingLevel::H2 => events.push(Event::Html(
                                format!("<h2 id=\"{id}\">{content}</h2>").into(),
                            )),
                            HeadingLevel::H3 => events.push(Event::Html(
                                format!("<h3 id=\"{id}\">{content}</h3>").into(),
                            )),
                            HeadingLevel::H4 => events.push(Event::Html(
                                format!("<h4 id=\"{id}\">{content}</h4>").into(),
                            )),
                            HeadingLevel::H5 => events.push(Event::Html(
                                format!("<h5 id=\"{id}\">{content}</h5>").into(),
                            )),
                            HeadingLevel::H6 => events.push(Event::Html(
                                format!("<h6 id=\"{id}\">{content}</h6>").into(),
                            )),
                        }
                    } else {
                        let rendered = match code_block_lang.take() {
                            Some(lang) => {
                                render_code_block(lang.as_str(), &content, highlighter)?.join("")
                            }
                            None => content.to_string(),
                        };
                        events.push(Event::Html(rendered.into()));
                    }
                }
                Event::Code(content) => {
                    events.push(Event::Html(format!("<code>{content}</code>").into()));
                }
                Event::Start(Tag::Heading(level, _id, _classes)) => {
                    heading_level = Some(level);
                    // events.push(Event::Start(Tag::Heading(level, id, classes)));
                }
                Event::End(Tag::Heading(_level, _id, _classes)) => {
                    heading_level = None;
                    // events.push(Event::End(Tag::Heading(level, id, classes)));
                }
                other => {
                    events.push(other);
                }
            }
        }
    }

    html::push_html(&mut buf, events.into_iter());

    Ok(buf)
}

fn dashify<S: AsRef<str>>(data: S) -> String {
    data.as_ref()
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || c == &'-' || c == &'_')
        .collect()
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
            .ok_or_else(|| eyre!("unable to find theme for syntax {}", lang))?;
        highlighter.highlight(syntax, content)
    }
}

#[cfg(test)]
mod test {

    #![allow(clippy::all)]
    #![allow(warnings, unused)]

    use temptree::temptree;

    use crate::{
        core::{
            page::test_page::{new_page, new_page_with_tree},
            Library, Page,
        },
        render::highlight::{syntect_highlighter::THEME_CLASS_PREFIX, SyntectHighlighter},
    };
    use fancy_regex::Regex;

    use super::MarkdownRenderer;

    fn internal_doc_link_render(test_page: Page, linked_page: Page) -> String {
        let mut store = Library::new();
        let key = store.insert(test_page);
        store.insert(linked_page);

        let md_renderer = MarkdownRenderer::new();
        let highlighter = SyntectHighlighter::new().unwrap();

        let test_page = store
            .get_with_key(key)
            .expect("page is missing from page store");
        let rendered_page = md_renderer
            .render(&test_page, &store, &highlighter, test_page.raw_markdown())
            .expect("failed to render test page");
        rendered_page
    }

    fn get_href_attr(rendered: &str) -> String {
        let re = Regex::new(r#"href="(.*)""#).unwrap();
        let capture = re
            .captures_iter(&rendered)
            .next()
            .expect("missing href attribute on link")
            .unwrap();
        capture[1].to_string()
    }

    #[test]
    fn internal_doc_link_nested() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {
                "default.tera": "",
                "empty.tera": "",
            },
            target: {},
            src: {
                "test.md": "",
                level_1: {
                    "doc.md": ""
                }
            },
            syntax_themes: {},
        };

        let test_page = new_page_with_tree(
            &tree,
            &tree.path().join("src/test.md"),
            r#"+++
            +++
            [internal link](@/level_1/doc.md)"#,
        )
        .unwrap();

        let linked_page = new_page_with_tree(
            &tree,
            &tree.path().join("src/level_1/doc.md"),
            r#"+++
            template_name = "empty.tera"
            +++"#,
        )
        .unwrap();

        let rendered = internal_doc_link_render(test_page, linked_page);
        let href = get_href_attr(&rendered);

        assert_eq!(href, "/level_1/doc.html");
    }

    #[test]
    fn internal_doc_link_at_root() {
        let test_page = new_page(
            r#"+++
            +++
            [internal link](@/doc.md)"#,
            "test.md",
        )
        .unwrap();

        let linked_page = new_page(
            r#"+++
            +++"#,
            "doc.md",
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
            "test.md",
        )
        .unwrap();

        let mut store = Library::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered = super::render(page, &store, &highlighter, page.raw_markdown())
            .expect("failed to render markdown");
        assert_eq!(rendered, "<pre><code>code sample here</code></pre>");
    }

    #[test]
    fn handles_inline_code_blocks() {
        let page = new_page(
            r#"+++
            +++
            inline `let x = 1;` code
            "#,
            "test.md",
        )
        .unwrap();

        let mut store = Library::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered = super::render(page, &store, &highlighter, page.raw_markdown())
            .expect("failed to render markdown");

        assert_eq!(rendered, "<p>inline <code>let x = 1;</code> code</p>\n");
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
            "test.md",
        )
        .unwrap();

        let mut store = Library::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered = super::render(page, &store, &highlighter, page.raw_markdown())
            .expect("failed to render markdown");
        let expected = include_str!("test_data/handles_code_fence_with_language_specified.html")
            .replace("syn-", THEME_CLASS_PREFIX);
        assert_eq!(rendered, expected);
    }

    #[test]
    fn adds_ids_to_headings_for_toc_anchors() {
        let page = new_page(
            r#"+++
            +++
# h1
## h2
### h3
#### h4
##### h5
###### h6"#,
            "test.md",
        )
        .unwrap();

        let mut store = Library::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered = super::render(page, &store, &highlighter, page.raw_markdown())
            .expect("failed to render markdown");
        assert_eq!(
            rendered,
            include_str!("test_data/adds_ids_to_headings_for_toc_anchors.html")
        );
    }

    #[test]
    fn dashifies_headers() {
        let page = new_page(
            r#"+++
            +++
# h1 is a HEADER
## h2 is a HEADER
### h3 is a HEADER
#### h4 is a HEADER
##### h5 is a HEADER
###### h6 is a HEADER"#,
            "test.md",
        )
        .unwrap();

        let mut store = Library::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered = super::render(page, &store, &highlighter, page.raw_markdown())
            .expect("failed to render markdown");
        assert_eq!(rendered, include_str!("test_data/dashifies_headers.html"));
    }

    #[cfg(test)]
    mod dashify {
        use crate::render::markup::markdown::dashify;

        macro_rules! test {
            ($name:ident: $input:literal => $expected:literal) => {
                #[test]
                fn $name() {
                    assert_eq!($expected, dashify($input));
                }
            };
        }

        test!(no_changes_to_basic_alphanumeric: "abc123" => "abc123");
        test!(makes_lowercase: "TEST" => "test");
        test!(removes_punctuation: "test.<>/!@#$%^&*()=+" => "test");
        test!(preserves_dashes: "test-test" => "test-test");
        test!(spaces_to_dashes: "test test" => "test-test");
        test!(preservse_underscores: "test_test" => "test_test");
    }

    #[test]
    fn no_sup_tag_on_footnote_definitions() {
        let page = new_page(
            r#"+++
+++
sample[^1]

[^1]: definition
"#,
            "test.md",
        )
        .unwrap();

        let mut store = Library::new();
        let key = store.insert(page);

        let page = store
            .get_with_key(key)
            .expect("page is missing from page store");

        let highlighter = SyntectHighlighter::new().unwrap();

        let rendered = super::render(page, &store, &highlighter, page.raw_markdown())
            .expect("failed to render markdown");

        assert_eq!(
            rendered,
            include_str!("test_data/no_sup_tag_on_footnote_definitions.html")
        );
    }
}
