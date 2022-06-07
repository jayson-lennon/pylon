pub mod collection;

pub use collection::SearchDocs;

use enumflags2::{bitflags, BitFlags};
use serde::Serialize;
use std::collections::BTreeMap;

pub type Result<T> = eyre::Result<T>;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SearchDoc {
    inner: BTreeMap<String, serde_json::Value>,
}

impl SearchDoc {
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    pub fn insert<K>(&mut self, key: K, value: serde_json::Value)
    where
        K: Into<String>,
    {
        let key = key.into();
        self.inner.insert(key, value);
    }

    pub fn into_value(self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self.inner)?)
    }

    pub fn as_value(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.inner)?)
    }
}

impl Default for SearchDoc {
    fn default() -> Self {
        Self::new()
    }
}

#[bitflags]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContentOptions {
    Headers,
    Text,
}

impl ContentOptions {
    pub fn default() -> BitFlags<Self> {
        Self::Text | Self::Headers
    }
    pub fn all() -> BitFlags<Self> {
        Self::Text | Self::Headers
    }
}

#[derive(Clone, Debug, PartialEq)]
struct MarkdownContent {
    pub headers: Vec<String>,
    pub paragraphs: Vec<String>,
}

impl MarkdownContent {
    pub fn to_doc(&self) -> Result<SearchDoc> {
        let mut doc = SearchDoc::new();
        let paragraphs = self.paragraphs.join(" ");
        doc.insert("headers", serde_json::to_value(&self.headers)?);
        doc.insert("content", serde_json::to_value(paragraphs)?);
        Ok(doc)
    }
}

fn get_markdown_content<M, O>(raw_markdown: M, options: O) -> MarkdownContent
where
    M: AsRef<str>,
    O: Into<BitFlags<ContentOptions>>,
{
    use pulldown_cmark::{Event, Parser, Tag};

    let raw_markdown = raw_markdown.as_ref();
    let options = options.into();

    let parser = Parser::new_ext(raw_markdown, pulldown_cmark::Options::all());

    let mut headers: Vec<String> = vec![];
    let mut paragraphs: Vec<String> = vec![];

    let mut in_paragraph = false;
    let mut in_heading = false;

    for ev in parser {
        match ev {
            Event::Start(Tag::Heading(_, _, _)) => in_heading = true,
            Event::End(Tag::Heading(_, _, _)) => in_heading = false,

            Event::Start(Tag::Paragraph) => in_paragraph = true,
            Event::End(Tag::Paragraph) => in_paragraph = false,

            Event::Text(text) => {
                if options.contains(ContentOptions::Text) && in_paragraph {
                    paragraphs.push(text.to_string());
                }
                if options.contains(ContentOptions::Headers) && in_heading {
                    headers.push(text.to_string());
                }
            }
            _ => (),
        }
    }

    MarkdownContent {
        headers,
        paragraphs,
    }
}

pub fn search_doc_from_markdown<M, O>(raw_markdown: M, options: O) -> Result<SearchDoc>
where
    M: AsRef<str>,
    O: Into<BitFlags<ContentOptions>>,
{
    let markdown_content = get_markdown_content(raw_markdown, options);
    markdown_content.to_doc()
}

#[cfg(test)]
mod test {
    use crate::*;

    #[test]
    fn extracts_markdown_paragraphs() {
        let options = ContentOptions::Text;
        let content = get_markdown_content(
            r#"
# heading
# heading with `cvode`
some text
- list item

* another list item
```code
in a code block
```
after code block

| Syntax      | Description |
| ----------- | ----------- |
| Header      | Title       |
| Paragraph   | Text        |

"#,
            options,
        );

        assert_eq!(
            content,
            MarkdownContent {
                headers: vec![],
                paragraphs: vec!["some text".into(), "after code block".into()],
            }
        );
    }

    #[test]
    fn extracts_markdown_headers() {
        let options = ContentOptions::Headers;
        let content = get_markdown_content(
            r#"
# heading
# heading with `cvode`
some text
- list item

* another list item
```code
in a code block
```
after code block

| Syntax      | Description |
| ----------- | ----------- |
| Header      | Title       |
| Paragraph   | Text        |

"#,
            options,
        );

        assert_eq!(
            content,
            MarkdownContent {
                headers: vec!["heading".into(), "heading with ".into()],
                paragraphs: vec![],
            }
        );
    }

    #[test]
    fn extracts_everything() {
        let options = ContentOptions::all();
        let content = get_markdown_content(
            r#"
# heading
# heading with `cvode`
some text
- list item

* another list item
```code
in a code block
```
after code block

| Syntax      | Description |
| ----------- | ----------- |
| Header      | Title       |
| Paragraph   | Text        |

"#,
            options,
        );

        assert_eq!(
            content,
            MarkdownContent {
                headers: vec!["heading".into(), "heading with ".into()],
                paragraphs: vec!["some text".into(), "after code block".into()],
            }
        );
    }
}

#[cfg(test)]
mod text_doc {
    use crate::SearchDoc;

    fn str_val(s: &str) -> serde_json::Value {
        serde_json::to_value(s).unwrap()
    }

    #[test]
    fn inserts() {
        let mut doc = SearchDoc::default();
        assert!(doc.inner.is_empty());

        doc.insert("key", str_val("value"));
        assert_eq!(doc.inner.len(), 1);
    }

    #[test]
    fn into_value() {
        use serde_json::json;

        let mut doc = SearchDoc::new();
        doc.insert("key", str_val("value"));
        doc.insert("abc", str_val("123"));

        let value = doc.into_value().unwrap();
        assert_eq!(value, json!({"key": "value", "abc": "123"}));
    }
}

#[cfg(test)]
mod text_markdown_content {
    use crate::{MarkdownContent, SearchDoc};

    #[test]
    fn converts_to_doc() {
        let headers = vec!["header1".into(), "header2".into()];
        let paragraphs = vec![
            "paragraph 1".into(),
            "paragraph 2".into(),
            "paragraph 3".into(),
            "paragraph 4".into(),
            "paragraph 5".into(),
            "paragraph 6".into(),
        ];

        let mdcontent = MarkdownContent {
            headers: headers.clone(),
            paragraphs: paragraphs.clone(),
        };

        let doc = mdcontent.to_doc().expect("failed to create doc");

        let mut expected = SearchDoc::new();
        expected.insert("headers", serde_json::to_value(headers).unwrap());
        expected.insert(
            "content",
            serde_json::to_value(paragraphs.join(" ")).unwrap(),
        );

        assert_eq!(doc, expected);
    }
}
