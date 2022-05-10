use crate::core::engine::GlobalEnginePaths;
use crate::core::pagestore::SearchKey;
use crate::render::template::TemplateName;
use crate::CheckedFilePath;

use crate::pathmarker;
use crate::Renderers;
use crate::Result;
use crate::SysPath;
use eyre::{eyre, WrapErr};
use serde::Serialize;
use typed_path::RelPath;
use typed_uri::Uri;

use std::{collections::HashSet, path::PathBuf};
use tracing::{instrument, trace_span};

use super::FrontMatter;
use super::{PageKey, RawMarkdown};

const DEFAULT_TEMPLATE_NAME: &str = "default.tera";

#[derive(Clone, Debug, Serialize)]
pub struct Page {
    #[serde(skip)]
    pub engine_paths: GlobalEnginePaths,

    pub path: CheckedFilePath<pathmarker::Md>,

    pub raw_doc: String,
    pub page_key: PageKey,

    pub frontmatter: FrontMatter,
    pub raw_markdown: RawMarkdown,
}

impl Page {
    #[instrument(skip(renderers), ret)]
    pub fn from_file(
        engine_paths: GlobalEnginePaths,
        file_path: CheckedFilePath<pathmarker::Md>,
        renderers: &Renderers,
    ) -> Result<Self> {
        let mut file = std::fs::File::open(file_path.as_sys_path().to_absolute_path())
            .wrap_err_with(|| format!("failed opening source file {}", file_path))?;

        Self::from_reader(engine_paths, file_path, &mut file, renderers)
    }

    #[instrument(skip(renderers, reader), ret)]
    pub fn from_reader<R>(
        engine_paths: GlobalEnginePaths,
        file_path: CheckedFilePath<pathmarker::Md>,
        reader: &mut R,
        renderers: &Renderers,
    ) -> Result<Self>
    where
        R: std::io::Read,
    {
        let mut raw_doc = String::new();

        reader.read_to_string(&mut raw_doc).wrap_err_with(|| {
            format!(
                "error reading document into string for path {}",
                file_path.display()
            )
        })?;

        let (mut frontmatter, raw_markdown) = split_raw_doc(&raw_doc)
            .wrap_err_with(|| format!("failed parsing raw document for {}", file_path.display()))?;

        if frontmatter.template_name.is_none() {
            let all_templates = renderers.tera.get_template_names().collect::<HashSet<_>>();
            let template = find_default_template(&all_templates, &file_path)
                .wrap_err("Failed to locate default templates when creating new page")?;

            frontmatter.template_name = Some(template);
        }

        Ok(Self {
            engine_paths,
            path: file_path,

            raw_doc,
            page_key: PageKey::default(),

            frontmatter,
            raw_markdown,
        })
    }

    pub fn set_page_key(&mut self, key: PageKey) {
        self.page_key = key;
    }

    pub fn engine_paths(&self) -> GlobalEnginePaths {
        self.engine_paths.clone()
    }

    #[instrument(ret)]
    pub fn path(&self) -> &CheckedFilePath<pathmarker::Md> {
        &self.path
    }

    #[instrument(ret)]
    pub fn target(&self) -> SysPath {
        self.path()
            .as_sys_path()
            .clone()
            .with_base(self.engine_paths.output_dir())
            .with_extension("html")
    }

    pub fn uri(&self) -> Uri {
        let uri = format!(
            "/{}",
            self.target()
                .with_base(&RelPath::from_relative(""))
                .with_extension("html")
                .to_relative_path()
                .to_string()
        );
        // always has a starting slash
        Uri::new(uri).unwrap()
    }

    pub fn search_key(&self) -> SearchKey {
        let mut target_path = PathBuf::from("/");
        target_path.push(self.path().as_sys_path().target());
        SearchKey::new(target_path.to_string_lossy())
    }

    #[instrument(ret)]
    pub fn template_name(&self) -> TemplateName {
        debug_assert!(self.frontmatter.template_name.is_some());
        self.frontmatter.template_name.as_ref().cloned().unwrap()
    }
}

fn split_raw_doc<S: AsRef<str>>(raw: S) -> Result<(FrontMatter, RawMarkdown)> {
    let raw = raw.as_ref();

    let (raw_frontmatter, raw_markdown) = split_document(raw)
        .wrap_err_with(|| String::from("failed to split raw document into component parts"))?;

    let frontmatter: FrontMatter = toml::from_str(raw_frontmatter)
        .wrap_err_with(|| String::from("failed parsing frontmatter into TOML"))?;
    let raw_markdown = RawMarkdown(raw_markdown.to_string());
    Ok((frontmatter, raw_markdown))
}

#[instrument(skip_all, fields(page=?path))]
fn find_default_template(
    all_templates: &HashSet<&str>,
    path: &CheckedFilePath<pathmarker::Md>,
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

#[instrument(ret)]
fn get_default_template_name(
    default_template_names: &HashSet<&str>,
    path: &CheckedFilePath<pathmarker::Md>,
) -> Option<TemplateName> {
    let mut path = path.as_sys_path().target().to_path_buf();

    loop {
        let template_name = {
            let mut candidate = PathBuf::from(&path);
            candidate.push(DEFAULT_TEMPLATE_NAME);
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
    match re.captures(raw) {
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
pub mod test {
    #![allow(clippy::all)]
    #![allow(warnings, unused)]

    use std::io;
    use std::path::Path;

    use crate::core::pagestore::SearchKey;
    use crate::test::{default_test_paths, rel};
    use crate::{CheckedFilePath, SysPath};
    use tempfile::TempDir;
    use temptree::temptree;

    use crate::{render::template::TemplateName, Renderers, Result};

    use super::Page;

    pub mod doc {
        pub mod broken {
            pub const MALFORMED_FRONTMATTER: &str = r#"
                +++
                whoops = 
                +++
                content"#;
            pub const MISSING_OPENING_DELIMITER: &str = r#"
                whoops = 
                +++
                content"#;

            pub const MISSING_CLOSING_DELIMITER: &str = r#"
                +++
                whoops = 
                content"#;

            pub const WRONG_DELIMETERS: &str = r#"
                ++
                whoops = 
                content
                ++"#;
            pub const INVALID_STARTING_CHARACTERS: &str = r#"
                whoops
                +++
                whoops = 
                +++
                content"#;
        }
        pub const MINIMAL: &str = r#"
            +++
            template_name = "empty.tera"
            +++
            content"#;
        pub const NO_CONTENT: &str = r#"
            +++
            template_name = "empty.tera"
            +++"#;

        pub const EMPTY_LINE_CONTENT: &str = r#"
            +++
            template_name = "empty.tera"
            +++"#;

        pub const EMPTY_FRONTMATTER_WITH_NEWLINES: &str = r#"
                    
            +++

            +++
            content"#;

        pub const EMPTY_FRONTMATTER: &str = r#"
            +++
            +++
            content"#;
    }

    pub fn new_page_with_tree(tree: &TempDir, file_path: &Path, content: &str) -> Result<Page> {
        let paths = crate::test::default_test_paths(&tree);

        let renderers =
            Renderers::new(tree.path().join("templates")).expect("Failed to create renderers");

        std::fs::write(&file_path, content).expect("failed to write doc");

        // relative to src directory in project folder
        let relative_target_path = file_path
            .strip_prefix(tree.path())
            .unwrap()
            .strip_prefix(paths.src_dir())
            .unwrap();

        let sys_path = SysPath::new(
            paths.project_root(),
            paths.src_dir(),
            rel!(relative_target_path),
        );

        let checked_path =
            CheckedFilePath::try_from(&sys_path).expect("failed to create checked file path");

        Page::from_file(paths, checked_path, &renderers)
    }

    pub fn new_page(doc: &str, file_name: &str) -> Result<Page> {
        let (paths, tree) = crate::test::simple_init();
        let renderers =
            Renderers::new(tree.path().join("templates")).expect("Failed to create renderers");

        let doc_path = tree.path().join("src").join(file_name);
        std::fs::write(&doc_path, doc).expect("failed to write doc");

        let sys_path = SysPath::new(paths.project_root(), paths.src_dir(), rel!(file_name));
        let checked_path =
            CheckedFilePath::try_from(&sys_path).expect("failed to create checked file path");

        Page::from_file(paths, checked_path, &renderers)
    }

    macro_rules! new_page_ok {
        ($name:ident => $doc:path) => {
            #[test]
            fn $name() {
                let page = new_page($doc, "doc.md");
                assert!(page.is_ok());
            }
        };
    }

    macro_rules! new_page_err {
        ($name:ident => $doc:path) => {
            #[test]
            fn $name() {
                let page = new_page($doc, "doc.md");
                assert!(page.is_err());
            }
        };
    }

    new_page_err!(err_on_missing_closing_delimeter => doc::broken::MISSING_CLOSING_DELIMITER);
    new_page_err!(err_on_missing_opening_delimeter => doc::broken::MISSING_OPENING_DELIMITER);
    new_page_err!(err_on_wrong_delimeters => doc::broken::WRONG_DELIMETERS);
    new_page_err!(err_on_malformed_frontmatter => doc::broken::MALFORMED_FRONTMATTER);
    new_page_err!(err_on_extra_characters => doc::broken::INVALID_STARTING_CHARACTERS);

    new_page_ok!(ok_with_empty_frontmatter => doc::EMPTY_FRONTMATTER);
    new_page_ok!(ok_with_no_content => doc::NO_CONTENT);
    new_page_ok!(ok_with_newlines_in_frontmatter => doc::EMPTY_FRONTMATTER_WITH_NEWLINES);
    new_page_ok!(ok_with_newline_content => doc::EMPTY_LINE_CONTENT);

    #[test]
    fn make_new_happy_paths() {
        let page = new_page(
            r#"
        +++
        +++
        sample content"#,
            "doc.md",
        )
        .unwrap();

        assert_eq!(page.template_name(), TemplateName::new("default.tera"));
        assert_eq!(page.search_key(), SearchKey::new("/doc.md"));
    }

    #[test]
    fn sets_page_key() {
        use crate::core::page::PageKey;
        use slotmap::SlotMap;

        let mut map: SlotMap<PageKey, _> = SlotMap::with_key();
        let mut page = new_page(
            r#"
        +++
        +++
        sample content"#,
            "doc.md",
        )
        .unwrap();
        map.insert(page.clone());
        let new_key = map.insert(page.clone());

        page.set_page_key(new_key);
        assert_eq!(page.page_key, new_key);
    }
}
