use eyre::WrapErr;
use pindexer::{IndexEntry, MarkdownIndexOptions};

use crate::Result;

use super::Page;

fn remove_shortcodes<S: Into<String>>(raw_markdown: S) -> Result<String> {
    let mut raw_markdown = raw_markdown.into();

    while let Some(code) = crate::discover::shortcode::find_next(&raw_markdown)
        .wrap_err("Failed locating shortcodes while building index")?
    {
        // required for https://github.com/rust-lang/rust/issues/59159
        let range = code.range().clone();

        raw_markdown.replace_range(range, "");
    }

    Ok(raw_markdown)
}

pub fn gen_index(page: &Page) -> Result<IndexEntry> {
    let raw_markdown = remove_shortcodes(page.raw_markdown().as_ref()).wrap_err_with(|| {
        format!(
            "Failed to remove shortcodes while building index for page '{}'",
            page.path()
        )
    })?;
    let mut entry = pindexer::index_entry_from_markdown(raw_markdown, MarkdownIndexOptions::all())
        .wrap_err_with(|| format!("Failed to generate index for page '{}'", page.path()))?;
    let keywords = serde_json::to_value(&page.frontmatter().keywords).wrap_err_with(|| {
        format!(
            "Failed to convert page keywords to JSON for page '{}'",
            page.path()
        )
    })?;
    entry.insert("keywords", keywords);

    Ok(entry)
}

#[cfg(test)]
mod test {
    use crate::core::page::test_page::new_page;
    use pindexer::IndexEntry;

    #[test]
    fn generates_search_index() {
        let page = new_page(
            r#"
+++
+++
# header level 1
## header level 2
### header level 3
document content"#,
            "doc.md",
        )
        .unwrap();
        let entry = super::gen_index(&page).expect("failed to create index entry");

        let expected = {
            let headers = vec!["header level 1", "header level 2", "header level 3"];
            let keywords: Vec<String> = vec![];
            let mut expected = IndexEntry::new();
            expected.insert("headers", serde_json::to_value(headers).unwrap());
            expected.insert("content", serde_json::to_value("document content").unwrap());
            expected.insert("keywords", serde_json::to_value(keywords).unwrap());
            expected
        };

        assert_eq!(entry, expected);
    }

    #[test]
    fn removes_shortcodes() {
        let page = new_page(
            r#"
+++
+++
one
{{ shortcode() }}
two
{% shortcode() %}
  delete me
{% end %}three"#,
            "doc.md",
        )
        .unwrap();
        let entry = super::gen_index(&page).expect("failed to create index entry");

        let expected = {
            let headers: Vec<String> = vec![];
            let keywords: Vec<String> = vec![];
            let mut expected = IndexEntry::new();
            expected.insert("headers", serde_json::to_value(headers).unwrap());
            expected.insert("content", serde_json::to_value("one two three").unwrap());
            expected.insert("keywords", serde_json::to_value(keywords).unwrap());
            expected
        };

        assert_eq!(entry, expected);
    }
}
