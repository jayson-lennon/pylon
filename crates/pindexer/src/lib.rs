pub mod index;

use enumflags2::{bitflags, BitFlags};
use std::collections::BTreeMap;

pub type Result<T> = eyre::Result<T>;

#[derive(Clone, Debug, PartialEq)]
pub struct IndexEntry {
    inner: BTreeMap<String, serde_json::Value>,
}

impl IndexEntry {
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

impl Default for IndexEntry {
    fn default() -> Self {
        Self::new()
    }
}

#[bitflags]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkdownIndexOptions {
    Headers,
    Text,
}

impl MarkdownIndexOptions {
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
    pub fn to_entry(&self) -> Result<IndexEntry> {
        let mut entry = IndexEntry::new();
        let paragraphs = self.paragraphs.join(" ");
        entry.insert("headers", serde_json::to_value(&self.headers)?);
        entry.insert("content", serde_json::to_value(paragraphs)?);
        Ok(entry)
    }
}

fn get_markdown_content<O: Into<BitFlags<MarkdownIndexOptions>>>(
    raw_markdown: &str,
    options: O,
) -> MarkdownContent {
    use pulldown_cmark::{Event, Parser, Tag};

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
                if options.contains(MarkdownIndexOptions::Text) && in_paragraph {
                    paragraphs.push(text.to_string());
                }
                if options.contains(MarkdownIndexOptions::Headers) && in_heading {
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

pub fn index_entry_from_markdown<O: Into<BitFlags<MarkdownIndexOptions>>>(
    raw_markdown: &str,
    options: O,
) -> Result<IndexEntry> {
    let markdown_content = get_markdown_content(raw_markdown, options);
    markdown_content.to_entry()
}

#[cfg(test)]
mod test {
    use crate::*;

    #[test]
    fn extracts_markdown_paragraphs() {
        let options = MarkdownIndexOptions::Text;
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
        let options = MarkdownIndexOptions::Headers;
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
        let options = MarkdownIndexOptions::all();
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
mod text_index_entry {
    use crate::IndexEntry;

    fn str_val(s: &str) -> serde_json::Value {
        serde_json::to_value(s).unwrap()
    }

    #[test]
    fn inserts() {
        let mut entry = IndexEntry::default();
        assert!(entry.inner.is_empty());

        entry.insert("key", str_val("value"));
        assert_eq!(entry.inner.len(), 1);
    }

    #[test]
    fn into_value() {
        use serde_json::json;

        let mut entry = IndexEntry::new();
        entry.insert("key", str_val("value"));
        entry.insert("abc", str_val("123"));

        let value = entry.into_value().unwrap();
        assert_eq!(value, json!({"key": "value", "abc": "123"}));
    }
}

#[cfg(test)]
mod text_markdown_content {
    use crate::{IndexEntry, MarkdownContent};

    #[test]
    fn converts_to_entry() {
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

        let entry = mdcontent.to_entry().expect("failed to create IndexEntry");

        let mut expected = IndexEntry::new();
        expected.insert("headers", serde_json::to_value(headers).unwrap());
        expected.insert(
            "content",
            serde_json::to_value(paragraphs.join(" ")).unwrap(),
        );

        assert_eq!(entry, expected);
    }
}
