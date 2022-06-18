use crate::render::template::TemplateName;

use crate::Result;

use eyre::{eyre, WrapErr};

use typed_path::ConfirmedPath;

use std::{collections::HashSet, path::PathBuf};
use tracing::trace_span;

use super::FrontMatter;
use super::RawMarkdown;

const DEFAULT_TEMPLATE_NAME: &str = "default.tera";

pub fn split_raw_doc<S: AsRef<str>>(raw: S) -> Result<(FrontMatter, RawMarkdown)> {
    let raw = raw.as_ref();

    let (raw_frontmatter, raw_markdown) = split_document(raw)
        .wrap_err_with(|| String::from("failed to split raw document into component parts"))?;

    let frontmatter: FrontMatter = toml::from_str(raw_frontmatter)
        .wrap_err_with(|| String::from("failed parsing frontmatter into TOML"))?;
    let raw_markdown = RawMarkdown(raw_markdown.to_string());
    Ok((frontmatter, raw_markdown))
}

pub fn find_default_template(
    all_templates: &HashSet<String>,
    path: &ConfirmedPath<pathmarker::MdFile>,
) -> Result<TemplateName> {
    let _span = trace_span!("no template specified").entered();
    match get_default_template_name(all_templates, path) {
        Some(template) => Ok(template),
        None => {
            return Err(eyre!(
                "no template provided and unable to find a default template for page {}",
                path
            ))
        }
    }
}

fn get_default_template_name(
    default_template_names: &HashSet<String>,
    path: &ConfirmedPath<pathmarker::MdFile>,
) -> Option<TemplateName> {
    let mut path = path.as_sys_path().base().to_path_buf();
    dbg!(&path);

    loop {
        let template_name = {
            let mut candidate = PathBuf::from(&path);
            candidate.push(DEFAULT_TEMPLATE_NAME);
            dbg!(&candidate);
            candidate.to_string_lossy().to_string()
        };
        if default_template_names.contains(template_name.as_str()) {
            return Some(TemplateName::new(template_name));
        }
        if !path.pop() {
            return None;
        }
    }
}

fn split_document(raw: &str) -> Result<(&str, &str)> {
    let re = crate::util::static_regex!(
        r#"^[[:space:]]*\+\+\+[[:space:]]*\n?((?s).*)\n[[:space:]]*\+\+\+[[:space:]]*((?s).*)"#
    );
    match re
        .captures(raw)
        .wrap_err("Failed generating captures when splitting document")?
    {
        Some(captures) => {
            let frontmatter = captures
                .get(1)
                .map(|m| m.as_str())
                .ok_or_else(|| eyre!("unable to read frontmatter"))?;

            let markdown = captures
                .get(2)
                .map(|m| m.as_str())
                .ok_or_else(|| eyre!("unable to read markdown"))?;
            Ok((frontmatter, markdown))
        }
        None => Err(eyre!("improperly formed document")),
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use crate::test::{abs, rel};
    use temptree::temptree;
    use typed_path::{AbsPath, SysPath};

    use super::find_default_template;

    #[test]
    fn finds_default_template_in_same_dir() {
        let doc = r#"+++
+++
"#;
        let tree = temptree! {
            content: {
                "sample.md": doc
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();

        let mut all_templates = HashSet::new();
        all_templates.insert("content/default.tera".to_string());

        let path = SysPath::new(&root, rel!("content"), rel!("sample.md"))
            .typed(pathmarker::MdFile)
            .confirm()
            .expect("failed to confirm existence of path");

        let default =
            find_default_template(&all_templates, &path).expect("failed to find default template");

        assert_eq!(default, "content/default.tera".into());
    }

    #[test]
    fn finds_default_template_in_parent_dir() {
        let doc = r#"+++
+++
"#;
        let tree = temptree! {
            content: {
                "sample.md": doc
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();

        let mut all_templates = HashSet::new();
        all_templates.insert("default.tera".to_string());

        let path = SysPath::new(&root, rel!("content"), rel!("sample.md"))
            .typed(pathmarker::MdFile)
            .confirm()
            .expect("failed to confirm existence of path");

        let default =
            find_default_template(&all_templates, &path).expect("failed to find default template");

        assert_eq!(default, "default.tera".into());
    }

    #[test]
    fn uses_correct_default_template() {
        let doc = r#"+++
+++
"#;
        let tree = temptree! {
            content: {
                "sample.md": doc
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();

        let mut all_templates = HashSet::new();
        all_templates.insert("default.tera".to_string());
        all_templates.insert("content/default.tera".to_string());

        let path = SysPath::new(&root, rel!("content"), rel!("sample.md"))
            .typed(pathmarker::MdFile)
            .confirm()
            .expect("failed to confirm existence of path");

        let default =
            find_default_template(&all_templates, &path).expect("failed to find default template");

        assert_eq!(default, "content/default.tera".into());
    }

    #[test]
    fn fails_to_find_missing_template() {
        let doc = r#"+++
+++
"#;
        let tree = temptree! {
            content: {
                "sample.md": doc
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();

        let mut all_templates = HashSet::new();

        let path = SysPath::new(&root, rel!("content"), rel!("sample.md"))
            .typed(pathmarker::MdFile)
            .confirm()
            .expect("failed to confirm existence of path");

        let template = find_default_template(&all_templates, &path);

        assert!(template.is_err());
    }
}
