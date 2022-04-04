use crate::core::Uri;
use crate::render::template::TemplateName;
use crate::Renderers;
use crate::{core::RelSystemPath, frontmatter::FrontMatter};
use anyhow::{anyhow, Context};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tracing::{instrument, trace, trace_span};

slotmap::new_key_type! {
    pub struct PageKey;
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Markdown(String);

#[derive(Clone, Debug, Default, Serialize)]
pub struct Page {
    pub src_path: RelSystemPath,
    pub target_path: RelSystemPath,

    pub raw_doc: String,
    pub page_key: PageKey,

    pub frontmatter: FrontMatter,
    pub markdown: Markdown,
    pub uri: Uri,
}

impl Page {
    #[instrument(skip(renderers), ret)]
    pub fn from_file<P>(
        src_root: P,
        target_root: P,
        file_path: P,
        renderers: &Renderers,
    ) -> Result<Self, anyhow::Error>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        let src_root = src_root.as_ref();
        let target_root = target_root.as_ref();
        let file_path = file_path.as_ref();
        let file_path = if let Ok(path) = file_path.strip_prefix(src_root) {
            path
        } else {
            file_path
        };

        let src_path = src_path(src_root, file_path);

        let mut file = std::fs::File::open(&PathBuf::from(&src_path))
            .with_context(|| format!("failed opening source file {}", src_path.to_string()))?;

        Self::from_reader(src_root, target_root, file_path, &mut file, renderers)
    }

    #[instrument(skip(renderers, reader), ret)]
    pub fn from_reader<P, R>(
        src_root: P,
        target_root: P,
        file_path: P,
        reader: &mut R,
        renderers: &Renderers,
    ) -> Result<Self, anyhow::Error>
    where
        P: AsRef<Path> + std::fmt::Debug,
        R: std::io::Read,
    {
        let src_root = src_root.as_ref();
        let target_root = target_root.as_ref();
        let file_path = file_path.as_ref();

        let src_path = src_path(src_root, file_path);

        let mut raw_doc = String::new();
        reader.read_to_string(&mut raw_doc).with_context(|| {
            format!(
                "error reading document into string for path {}",
                src_path.to_string()
            )
        })?;

        let (mut frontmatter, markdown) = parsed_raw_document(&raw_doc, renderers)
            .with_context(|| format!("failed parsing raw document for {}", src_path.to_string()))?;

        let target_path = target_path(&src_path, target_root, frontmatter.use_index);

        let uri = uri(&target_path);

        let all_templates = renderers.tera.get_template_names().collect::<HashSet<_>>();
        let template = get_template_name(&all_templates, &src_path)?;

        frontmatter.template_name = Some(template);

        Ok(Self {
            src_path,
            raw_doc,
            frontmatter,
            markdown,
            uri,
            target_path,

            page_key: PageKey::default(),
        })
    }

    pub fn set_page_key(&mut self, key: PageKey) {
        self.page_key = key;
    }

    #[instrument(ret)]
    pub fn uri(&self) -> Uri {
        self.uri.clone()
    }
    #[instrument(ret)]
    pub fn src_path(&self) -> RelSystemPath {
        self.src_path.clone()
    }
    #[instrument(ret)]
    pub fn target_path(&self) -> RelSystemPath {
        self.target_path.clone()
    }

    #[instrument(ret)]
    pub fn template_name(&self) -> TemplateName {
        debug_assert!(self.frontmatter.template_name.is_some());
        self.frontmatter.template_name.as_ref().cloned().unwrap()
    }
}

fn src_path<P: AsRef<Path>>(src_root: P, file_path: P) -> RelSystemPath {
    RelSystemPath::new(src_root.as_ref(), file_path.as_ref())
}

fn raw_document<P: AsRef<Path>>(path: P) -> Result<String, anyhow::Error> {
    let path = path.as_ref();
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed reading raw document data from {}", path.display()))
}

fn parsed_raw_document<S: AsRef<str>>(
    raw: S,
    renderers: &Renderers,
) -> Result<(FrontMatter, Markdown), anyhow::Error> {
    let raw = raw.as_ref();

    let (raw_frontmatter, raw_markdown) = split_document(&raw)
        .with_context(|| String::from("failed to split raw document into component parts"))?;

    let frontmatter: FrontMatter = toml::from_str(raw_frontmatter)
        .with_context(|| String::from("failed parsing frontmatter into TOML"))?;
    let markdown = Markdown(renderers.markdown.render(raw_markdown));
    Ok((frontmatter, markdown))
}

fn target_path<P: AsRef<Path>>(
    src_path: &RelSystemPath,
    target_root: P,
    use_index: bool,
) -> RelSystemPath {
    let target = src_path.with_base(target_root.as_ref());
    if use_index && src_path.file_stem() != "index" {
        target
            .add_parent(target.with_extension("").file_name())
            .with_file_name("index.html")
    } else {
        target.with_extension("html")
    }
}

fn uri(target_path: &RelSystemPath) -> Uri {
    let target = target_path.target();
    let uri = Uri::new(format!("/{}", target.to_string_lossy()));
    debug_assert!(uri.is_ok());
    uri.unwrap()
}

#[instrument(skip_all, fields(page=%src_path.to_string()))]
fn get_template_name(
    template_names: &HashSet<&str>,
    src_path: &RelSystemPath,
) -> Result<TemplateName, anyhow::Error> {
    let _span = trace_span!("no template specified").entered();
    match get_default_template_name(template_names, src_path.clone()) {
        Some(template) => Ok(template),
        None => {
            return Err(anyhow!(
                "no template provided and unable to find a default template for page {}",
                src_path
            ))
        }
    }
}

#[instrument(ret)]
fn get_default_template_name(
    default_template_names: &HashSet<&str>,
    rel_system_path: RelSystemPath,
) -> Option<TemplateName> {
    // This function chomps the page path until no more components are remaining.
    let mut ancestors = rel_system_path.target().ancestors();

    while let Some(path) = ancestors.next() {
        let template_name = {
            // Add the default page name ("page.tera") to the new path.
            let mut template_name = PathBuf::from(path);
            template_name.push("page.tera");
            template_name.to_string_lossy().to_string()
        };

        if default_template_names.contains(&template_name.as_str()) {
            return Some(TemplateName::new(template_name));
        }
    }
    None
}

fn split_document(raw: &str) -> Result<(&str, &str), anyhow::Error> {
    let re = crate::util::static_regex!(
        r#"^[[:space:]]*\+\+\+[[:space:]]*\n((?s).*)\n[[:space:]]*\+\+\+[[:space:]]*((?s).*)"#
    );
    match re.captures(raw) {
        Some(captures) => {
            let frontmatter = captures
                .get(1)
                .map(|m| m.as_str())
                .ok_or_else(|| anyhow!("unable to read frontmatter"))?;

            let markdown = captures
                .get(2)
                .map(|m| m.as_str())
                .ok_or_else(|| anyhow!("unable to read markdown"))?;
            Ok((frontmatter, markdown))
        }
        None => Err(anyhow!("improperly formed document"))?,
    }
}

pub mod script {
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::rules::gctx::{ContextItem, Generators, Matcher};
        use crate::core::rules::Rules;
        use crate::core::Page;
        use crate::frontmatter::FrontMatter;
        use rhai::serde::to_dynamic;
        use rhai::FnPtr;
        use tracing::{instrument, trace};

        #[rhai_fn(name = "uri")]
        pub fn uri(page: &mut Page) -> String {
            page.uri().to_string()
        }

        #[rhai_fn(get = "frontmatter")]
        pub fn frontmatter(page: &mut Page) -> FrontMatter {
            page.frontmatter.clone()
        }

        /// Returns all attached metadata.
        #[rhai_fn(get = "meta", return_raw)]
        pub fn all_meta(page: &mut Page) -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            to_dynamic(page.frontmatter.meta.clone())
        }

        /// Returns the value found at the provided key. Returns `()` if the key wasn't found.
        #[rhai_fn()]
        pub fn meta(page: &mut Page, key: &str) -> rhai::Dynamic {
            page.frontmatter
                .meta
                .get(key)
                .map(|v| to_dynamic(v).ok())
                .flatten()
                .unwrap_or_default()
        }

        /// Generates a new context for use within the page template.
        #[instrument(ret)]
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
    }
}

#[cfg(test)]
pub mod test {

    use std::io;

    use crate::{
        core::{RelSystemPath, Uri},
        render::template::TemplateName,
        Renderers,
    };

    use super::Page;

    pub mod doc {
        pub mod broken {
            pub const MALFORMED_FRONTMATTER: &'static str = r#"
                +++
                whoops = 
                +++
                content"#;
            pub const MISSING_OPENING_DELIMITER: &'static str = r#"
                whoops = 
                +++
                content"#;

            pub const MISSING_CLOSING_DELIMITER: &'static str = r#"
                +++
                whoops = 
                content"#;

            pub const WRONG_DELIMETERS: &'static str = r#"
                ++
                whoops = 
                content
                ++"#;
            pub const INVALID_STARTING_CHARACTERS: &'static str = r#"
                whoops
                +++
                whoops = 
                +++
                content"#;
        }
        pub const MINIMAL: &'static str = r#"
            +++
            title = "test"
            template_name = "test"
            +++
            content"#;
        pub const NO_CONTENT: &'static str = r#"
            +++
            title = "test"
            template_name = "test"
            +++"#;

        pub const EMPTY_LINE_CONTENT: &'static str = r#"
            +++
            title = "test"
            template_name = "test"
            +++"#;

        pub const EMPTY_FRONTMATTER_WITH_NEWLINES: &'static str = r#"
                    
            +++

            +++
            content"#;

        pub const EMPTY_FRONTMATTER: &'static str = r#"
            +++
            +++
            content"#;
    }

    pub fn basic_page() -> Page {
        page_from_doc(doc::MINIMAL).unwrap()
    }

    pub fn page_from_doc_with_paths(
        doc: &str,
        src: &str,
        target: &str,
        path: &str,
    ) -> Result<Page, anyhow::Error> {
        let renderers = Renderers::new("test/templates/**/*");
        let mut reader = io::Cursor::new(doc.as_bytes());
        Page::from_reader(src, target, path, &mut reader, &renderers)
    }

    pub fn page_from_doc(doc: &str) -> Result<Page, anyhow::Error> {
        let renderers = Renderers::new("test/templates/**/*");
        let mut reader = io::Cursor::new(doc.as_bytes());
        Page::from_reader(
            "src_root",
            "target_root",
            "file_path/is/test.ext",
            &mut reader,
            &renderers,
        )
    }

    macro_rules! new_page_ok {
        ($name:ident => $doc:path) => {
            #[test]
            fn $name() {
                let page = page_from_doc($doc);
                assert!(page.is_ok());
            }
        };
    }

    macro_rules! new_page_err {
        ($name:ident => $doc:path) => {
            #[test]
            fn $name() {
                let page = page_from_doc($doc);
                assert!(page.is_err());
            }
        };
    }

    new_page_err!(err_on_missing_closing_delimeter => doc::broken::MISSING_CLOSING_DELIMITER);
    new_page_err!(err_on_missing_opening_delimeter => doc::broken::MISSING_OPENING_DELIMITER);
    new_page_err!(err_on_wrong_delimeters => doc::broken::WRONG_DELIMETERS);
    new_page_err!(err_on_malformed_frontmatter => doc::broken::MALFORMED_FRONTMATTER);
    new_page_err!(err_on_extra_characters => doc::broken::INVALID_STARTING_CHARACTERS);
    new_page_err!(err_with_empty_frontmatter => doc::EMPTY_FRONTMATTER);

    new_page_ok!(ok_with_newlines_in_frontmatter => doc::EMPTY_FRONTMATTER_WITH_NEWLINES);
    new_page_ok!(ok_with_no_content => doc::NO_CONTENT);
    new_page_ok!(ok_with_newline_content => doc::EMPTY_LINE_CONTENT);

    #[test]
    fn make_new_happy_paths() {
        let page = basic_page();

        assert_eq!(
            page.src_path(),
            RelSystemPath::new("src_root", "file_path/is/test.ext")
        );

        assert_eq!(
            page.target_path(),
            RelSystemPath::new("target_root", "file_path/is/test/index.html")
        );

        assert_eq!(
            page.uri(),
            Uri::new("/file_path/is/test/index.html").unwrap()
        );

        assert_eq!(page.template_name(), TemplateName::new("test"));
    }

    #[test]
    fn sets_page_key() {
        use crate::core::page::PageKey;
        use slotmap::SlotMap;

        let mut map: SlotMap<PageKey, _> = SlotMap::with_key();
        let mut page = basic_page();
        map.insert(page.clone());
        let new_key = map.insert(page.clone());

        page.set_page_key(new_key);
        assert_eq!(page.page_key, new_key);
    }

    #[test]
    fn proper_target_without_use_index() {
        let doc = r#"
+++
title = "test"
template_name = "template"
use_index = false
+++"#;
        let page = page_from_doc(doc).unwrap();
        assert_eq!(
            page.target_path(),
            RelSystemPath::new("target_root", "file_path/is/test.html")
        );
    }

    #[test]
    fn proper_uri_without_use_index() {
        let doc = r#"
+++
title = "test"
template_name = "template"
use_index = false
+++"#;

        let page = page_from_doc(doc).unwrap();

        assert_eq!(page.uri(), Uri::from_path("/file_path/is/test.html"));
    }
}
