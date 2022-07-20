pub mod frontmatter;
pub mod lint;
pub mod render;
pub mod util;

use std::collections::HashSet;
use std::ops::Deref;

use crate::core::engine::GlobalEnginePaths;
use crate::core::library::SearchKey;
use crate::render::template::TemplateName;
use crate::Renderers;
use crate::SysPath;
use eyre::WrapErr;
pub use frontmatter::FrontMatter;
pub use lint::{lint, LintLevel, LintResult};
pub use render::{render, RenderedPage, RenderedPageCollection};
use serde::Deserialize;
use serde::Serialize;
use typed_path::AbsPath;
use typed_path::ConfirmedPath;
use typed_path::RelPath;
use typed_uri::Uri;

slotmap::new_key_type! {
    pub struct PageKey;
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextItem {
    pub identifier: String,
    pub data: serde_json::Value,
}

impl ContextItem {
    pub fn new<S: AsRef<str>>(identifier: S, data: serde_json::Value) -> Self {
        Self {
            identifier: identifier.as_ref().to_string(),
            data,
        }
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct RawMarkdown(String);

impl RawMarkdown {
    pub fn from_raw<S: Into<String>>(raw: S) -> Self {
        Self(raw.into())
    }
}

impl AsRef<str> for RawMarkdown {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Deref for RawMarkdown {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Page {
    #[serde(skip)]
    pub engine_paths: GlobalEnginePaths,

    pub path: ConfirmedPath<pathmarker::MdFile>,

    pub raw_doc: String,
    pub page_key: PageKey,

    pub frontmatter: FrontMatter,
    pub raw_markdown: RawMarkdown,
}

impl Page {
    pub fn from_file(
        engine_paths: GlobalEnginePaths,
        file_path: ConfirmedPath<pathmarker::MdFile>,
        renderers: &Renderers,
    ) -> crate::Result<Self> {
        let mut file = std::fs::File::open(file_path.as_sys_path().to_absolute_path())
            .wrap_err_with(|| format!("failed opening source file {}", file_path))?;

        Self::from_reader(engine_paths, file_path, &mut file, renderers)
    }

    pub fn from_reader<R>(
        engine_paths: GlobalEnginePaths,
        file_path: ConfirmedPath<pathmarker::MdFile>,
        reader: &mut R,
        renderers: &Renderers,
    ) -> crate::Result<Self>
    where
        R: std::io::Read,
    {
        let mut raw_doc = String::new();

        reader.read_to_string(&mut raw_doc).wrap_err_with(|| {
            format!("error reading document into string for path {}", file_path)
        })?;

        let (mut frontmatter, raw_markdown) = util::split_raw_doc(&raw_doc)
            .wrap_err_with(|| format!("failed parsing raw document for {}", file_path))?;

        if frontmatter.template_name.is_none() {
            let all_templates = renderers
                .tera()
                .get_template_names()
                .into_iter()
                .collect::<HashSet<_>>();
            let template = util::find_default_template(&all_templates, &file_path)
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

    pub fn path(&self) -> &ConfirmedPath<pathmarker::MdFile> {
        &self.path
    }

    pub fn raw_markdown(&self) -> &RawMarkdown {
        &self.raw_markdown
    }

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
        );
        // always has a starting slash
        Uri::new(&uri, &uri).unwrap()
    }

    pub fn search_keys(&self) -> Vec<SearchKey> {
        vec![
            self.uri().as_str().into(),
            self.path()
                .as_sys_path()
                .with_root(&AbsPath::from_absolute("/"))
                .with_base(&RelPath::from_relative(""))
                .to_string()
                .into(),
        ]
    }

    pub fn template_name(&self) -> TemplateName {
        debug_assert!(self.frontmatter.template_name.is_some());
        self.frontmatter.template_name.as_ref().cloned().unwrap()
    }

    pub fn frontmatter(&self) -> &FrontMatter {
        &self.frontmatter
    }
}

pub mod script {
    #[allow(clippy::wildcard_imports)]
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::page::{ContextItem, Page};

        #[rhai_fn(name = "uri")]
        pub fn uri(page: &mut Page) -> String {
            page.uri().to_string()
        }

        #[rhai_fn(get = "frontmatter", return_raw)]
        pub fn frontmatter(page: &mut Page) -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            crate::core::page::frontmatter::script::rhai_module::frontmatter(&mut page.frontmatter)
        }

        /// Returns all attached metadata.
        #[rhai_fn(get = "meta", return_raw)]
        pub fn all_meta(page: &mut Page) -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            crate::core::page::frontmatter::script::rhai_module::all_meta(&mut page.frontmatter)
        }

        /// Returns the value found at the provided key. Returns `()` if the key wasn't found.
        #[rhai_fn()]
        pub fn meta(page: &mut Page, key: &str) -> rhai::Dynamic {
            crate::core::page::frontmatter::script::rhai_module::get_meta(
                &mut page.frontmatter,
                key,
            )
        }

        /// Generates a new context for use within the page template.
        #[rhai_fn(return_raw)]
        pub fn new_context(map: rhai::Map) -> Result<Vec<ContextItem>, Box<EvalAltResult>> {
            let mut context_items = vec![];
            for (k, v) in map {
                let value: serde_json::Value = rhai::serde::from_dynamic(&v)?;
                let item = ContextItem::new(k, value);
                context_items.push(item);
            }
            Ok(context_items)
        }

        #[cfg(test)]
        mod test {
            use super::rhai_module;
            use crate::core::page::test_page::doc::MINIMAL;
            use crate::core::page::test_page::new_page_with_tree;

            use temptree::temptree;

            #[test]
            fn uri_fn() {
                let tree = temptree! {
                    "rules.rhai": "",
                    templates: {
                        "default.tera": "",
                        "empty.tera": "",
                    },
                    target: {},
                    src: {
                        "test.md": "",
                    },
                    syntax_themes: {},
                };
                let mut page =
                    new_page_with_tree(&tree, &tree.path().join("src/test.md"), MINIMAL).unwrap();
                let uri = rhai_module::uri(&mut page);
                assert_eq!(uri, String::from("/test.html"));
            }

            #[test]
            fn get_frontmatter() {
                let tree = temptree! {
                    "rules.rhai": "",
                    templates: {
                        "default.tera": "",
                        "empty.tera": "",
                    },
                    target: {},
                    src: {
                        "test.md": "",
                    },
                    syntax_themes: {},
                };
                let mut page =
                    new_page_with_tree(&tree, &tree.path().join("src/test.md"), MINIMAL).unwrap();
                let frontmatter = rhai_module::frontmatter(&mut page);
                assert_eq!(frontmatter.unwrap().type_name(), "map");
            }

            #[test]
            fn get_all_meta() {
                let tree = temptree! {
                    "rules.rhai": "",
                    templates: {
                        "default.tera": "",
                        "empty.tera": "",
                    },
                    target: {},
                    src: {
                        "test.md": "",
                    },
                    syntax_themes: {},
                };
                let mut page =
                    new_page_with_tree(&tree, &tree.path().join("src/test.md"), MINIMAL).unwrap();

                let dynamic = rhai_module::all_meta(&mut page);
                assert!(dynamic.is_ok());

                assert_eq!(dynamic.unwrap().type_name(), "map");
            }

            #[test]
            fn get_existing_meta_item() {
                let tree = temptree! {
                    "rules.rhai": "",
                    templates: {
                        "default.tera": "",
                        "empty.tera": "",
                    },
                    target: {},
                    src: {
                        "test.md": "",
                    },
                    syntax_themes: {},
                };
                let mut page = new_page_with_tree(
                    &tree,
                    &tree.path().join("src/test.md"),
                    r#"+++
                            template_name = "empty.tera"

                            [meta]
                            test = "sample"
                            +++"#,
                )
                .unwrap();

                let meta = rhai_module::meta(&mut page, "test");
                assert_eq!(meta.into_string().unwrap().as_str(), "sample");
            }

            #[test]
            fn get_nonexistent_meta_item() {
                let tree = temptree! {
                    "rules.rhai": "",
                    templates: {
                        "default.tera": "",
                        "empty.tera": "",
                    },
                    target: {},
                    src: {
                        "test.md": "",
                    },
                    syntax_themes: {},
                };
                let mut page = new_page_with_tree(
                    &tree,
                    &tree.path().join("src/test.md"),
                    r#"+++
                template_name = "empty.tera"

                [meta]
                test = "sample"
                +++"#,
                )
                .unwrap();

                let meta = rhai_module::meta(&mut page, "nope");
                assert_eq!(meta.type_name(), "()");
            }
        }
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]
    use super::*;

    #[test]
    fn new_context_item() {
        fn value(v: usize) -> serde_json::Value {
            serde_json::to_value(v).unwrap()
        }
        let ctx_item = ContextItem::new("test", value(1));
        assert_eq!(ctx_item.identifier.as_str(), "test");
        assert_eq!(ctx_item.data, value(1));
    }

    #[test]
    fn raw_markdown_as_ref() {
        let markdown = RawMarkdown("test".into());
        assert_eq!(markdown.as_ref(), "test");
    }
}

#[cfg(test)]
pub mod test_page {
    #![allow(clippy::all)]
    #![allow(warnings, unused)]

    use std::io;
    use std::path::Path;

    use crate::core::library::SearchKey;
    use crate::core::Page;
    use crate::test::{default_test_paths, rel};
    use crate::SysPath;
    use tempfile::TempDir;
    use temptree::temptree;

    use crate::{render::template::TemplateName, Renderers, Result};

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

        let renderers = Renderers::new(paths.clone()).expect("Failed to create renderers");

        std::fs::write(&file_path, content)
            .unwrap_or_else(|e| panic!("failed to write doc @ {}: {}", &file_path.display(), e));

        // relative to src directory in project folder
        let relative_target_path = file_path
            .strip_prefix(tree.path())
            .unwrap()
            .strip_prefix(paths.content_dir())
            .unwrap();

        let sys_path = SysPath::new(
            paths.project_root(),
            paths.content_dir(),
            rel!(relative_target_path),
        );

        let checked_path = sys_path
            .confirm(pathmarker::MdFile)
            .expect("failed to confirm path");

        Page::from_file(paths, checked_path, &renderers)
    }

    pub fn new_page(doc: &str, file_name: &str) -> Result<Page> {
        let (paths, tree) = crate::test::simple_init();
        let renderers = Renderers::new(paths.clone()).expect("Failed to create renderers");

        let doc_path = tree.path().join("src").join(file_name);

        {
            let mut doc_path = doc_path.clone();
            if let true = doc_path.pop() {
                std::fs::create_dir_all(&doc_path).expect("failed to make subdirs for page");
            }
        }
        std::fs::write(&doc_path, doc).expect("failed to write doc");

        let sys_path = SysPath::new(paths.project_root(), paths.content_dir(), rel!(file_name));
        let checked_path = sys_path
            .confirm(pathmarker::MdFile)
            .expect("failed to confirm path");

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
        for key in page.search_keys() {
            if key != "/doc.md".into() && key != "/doc.html".into() {
                panic!("invalid search key: {}", key);
            }
        }
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
