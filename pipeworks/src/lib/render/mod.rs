pub mod html;
pub mod markdown;

use crate::{RawFrontMatter, RawMarkdown};

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

pub fn split_document<D: AsRef<str>>(document: D) -> Option<(RawFrontMatter, RawMarkdown)> {
    let doc = document.as_ref();
    let re = regex!(
        r#"^[[:space:]]*\+\+\+[[:space:]]*\n((?s).*)\n[[:space:]]*\+\+\+[[:space:]]*((?s).*)"#
    );
    match re.captures(doc) {
        Some(captures) => {
            let frontmatter = RawFrontMatter::new(&captures[1]);
            let markdown = RawMarkdown::new(&captures[2]);
            Some((frontmatter, markdown))
        }
        None => None,
    }
}

#[cfg(test)]
mod test {
    use super::split_document;

    #[test]
    fn splits_well_formed_document() {
        let data = r"+++
a=1
b=2
c=3
+++
content here";
        let (frontmatter, markdown) = split_document(data).unwrap();
        assert_eq!(frontmatter.0, "a=1\nb=2\nc=3");
        assert_eq!(markdown.0, "content here");
    }

    #[test]
    fn splits_well_formed_document_with_newlines() {
        let data = r"+++
a=1
b=2
c=3

+++
content here

some newlines

";
        let (frontmatter, markdown) = split_document(data).unwrap();
        assert_eq!(frontmatter.0, "a=1\nb=2\nc=3\n");
        assert_eq!(markdown.0, "content here\n\nsome newlines\n\n");
    }
}
