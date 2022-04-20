use crate::core::RelSystemPath;
use crate::core::Uri;
use crate::render::template::TemplateName;
use crate::Renderers;
use crate::Result;
use anyhow::{anyhow, Context};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tracing::{instrument, trace_span};

use super::FrontMatter;
use super::{PageKey, RawMarkdown};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Page {
    pub src_path: RelSystemPath,
    pub target_path: RelSystemPath,

    pub raw_doc: String,
    pub page_key: PageKey,

    pub frontmatter: FrontMatter,
    pub raw_markdown: RawMarkdown,
    pub uri: Uri,
}

impl Page {
    #[instrument(skip(renderers), ret)]
    pub fn from_file<P>(
        src_root: P,
        target_root: P,
        file_path: P,
        renderers: &Renderers,
    ) -> Result<Self>
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
            .with_context(|| format!("failed opening source file {}", src_path))?;

        Self::from_reader(src_root, target_root, file_path, &mut file, renderers)
    }

    #[instrument(skip(renderers, reader), ret)]
    pub fn from_reader<P, R>(
        src_root: P,
        target_root: P,
        file_path: P,
        reader: &mut R,
        renderers: &Renderers,
    ) -> Result<Self>
    where
        P: AsRef<Path> + std::fmt::Debug,
        R: std::io::Read,
    {
        let src_root = src_root.as_ref();
        let target_root = target_root.as_ref();
        let file_path = file_path.as_ref();

        let src_path = src_path(src_root, file_path);

        let mut raw_doc = String::new();
        reader
            .read_to_string(&mut raw_doc)
            .with_context(|| format!("error reading document into string for path {}", src_path))?;

        let (mut frontmatter, raw_markdown) = split_raw_doc(&raw_doc)
            .with_context(|| format!("failed parsing raw document for {}", src_path))?;

        let target_path = target_path(&src_path, target_root, frontmatter.use_index);

        let uri = uri(&target_path, frontmatter.use_index);

        if frontmatter.template_name.is_none() {
            let all_templates = renderers.tera.get_template_names().collect::<HashSet<_>>();
            let template = get_template_name(&all_templates, &src_path)?;

            frontmatter.template_name = Some(template);
        }

        Ok(Self {
            src_path,
            target_path,

            raw_doc,
            page_key: PageKey::default(),

            frontmatter,
            raw_markdown,
            uri,
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

fn split_raw_doc<S: AsRef<str>>(raw: S) -> Result<(FrontMatter, RawMarkdown)> {
    let raw = raw.as_ref();

    let (raw_frontmatter, raw_markdown) = split_document(raw)
        .with_context(|| String::from("failed to split raw document into component parts"))?;

    let frontmatter: FrontMatter = toml::from_str(raw_frontmatter)
        .with_context(|| String::from("failed parsing frontmatter into TOML"))?;
    let raw_markdown = RawMarkdown(raw_markdown.to_string());
    Ok((frontmatter, raw_markdown))
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

fn uri(target_path: &RelSystemPath, _use_index: bool) -> Uri {
    let target = target_path.target();
    if target_path.file_stem() == "index" {
        debug_assert!(target.parent().is_some());
        Uri::from_path(target.parent().unwrap())
    } else {
        Uri::from_path(target)
    }
}

#[instrument(skip_all, fields(page=%src_path.to_string()))]
fn get_template_name(
    template_names: &HashSet<&str>,
    src_path: &RelSystemPath,
) -> Result<TemplateName> {
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

    for path in ancestors.by_ref() {
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

fn split_document(raw: &str) -> Result<(&str, &str)> {
    let re = crate::util::static_regex!(
        r#"^[[:space:]]*\+\+\+[[:space:]]*\n?((?s).*)\n[[:space:]]*\+\+\+[[:space:]]*((?s).*)"#
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
        None => Err(anyhow!("improperly formed document")),
    }
}

#[cfg(test)]
pub mod test {
    #![allow(clippy::all)]

    use std::io;

    use crate::{
        core::{RelSystemPath, Uri},
        render::template::TemplateName,
        Renderers, Result,
    };

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

    pub fn basic_page() -> Page {
        page_from_doc(doc::MINIMAL).unwrap()
    }

    pub fn page_from_doc_with_paths(
        doc: &str,
        src_root: &str,
        target_root: &str,
        doc_path: &str,
    ) -> Result<Page> {
        let renderers = Renderers::new("test/templates");
        let mut reader = io::Cursor::new(doc.as_bytes());
        Page::from_reader(src_root, target_root, doc_path, &mut reader, &renderers)
    }

    pub fn page_from_doc(doc: &str) -> Result<Page> {
        let renderers = Renderers::new("test/templates");
        let mut reader = io::Cursor::new(doc.as_bytes());
        Page::from_reader(
            "src_root",
            "target_root",
            "sample/stem.ext",
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

    new_page_ok!(ok_with_empty_frontmatter => doc::EMPTY_FRONTMATTER);
    new_page_ok!(ok_with_no_content => doc::NO_CONTENT);
    new_page_ok!(ok_with_newlines_in_frontmatter => doc::EMPTY_FRONTMATTER_WITH_NEWLINES);
    new_page_ok!(ok_with_newline_content => doc::EMPTY_LINE_CONTENT);

    #[test]
    fn make_new_happy_paths() {
        let page = basic_page();

        assert_eq!(
            page.src_path(),
            RelSystemPath::new("src_root", "sample/stem.ext")
        );

        assert_eq!(
            page.target_path(),
            RelSystemPath::new("target_root", "sample/stem/index.html")
        );

        assert_eq!(page.uri(), Uri::new("/sample/stem").unwrap());

        assert_eq!(page.template_name(), TemplateName::new("empty.tera"));
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
template_name = "empty.tera"
use_index = false
+++"#;
        let page = page_from_doc(doc).unwrap();
        assert_eq!(
            page.target_path(),
            RelSystemPath::new("target_root", "sample/stem.html")
        );
    }

    #[test]
    fn proper_uri_without_use_index() {
        let doc = r#"
+++
template_name = "empty.tera"
use_index = false
+++"#;

        let page = page_from_doc(doc).unwrap();

        assert_eq!(page.uri(), Uri::from_path("/sample/stem.html"));
    }
}