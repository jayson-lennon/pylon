#[allow(clippy::wildcard_imports)]
use dyn_clonable::*;
use std::fmt;

use crate::Result;

#[derive(Clone)]
pub struct PostProcessors {
    html_minifier: Box<dyn Processor>,
    css_minifier: Box<dyn Processor>,
}

impl PostProcessors {
    pub fn new() -> Self {
        Self {
            html_minifier: Box::new(HtmlMinifier::new()),
            css_minifier: Box::new(CssMinifier::new()),
        }
    }

    pub fn html_minifier(&self) -> &dyn Processor {
        self.html_minifier.as_ref()
    }

    pub fn css_minifier(&self) -> &dyn Processor {
        self.css_minifier.as_ref()
    }
}

impl Default for PostProcessors {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for PostProcessors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<PostProcessors>")
    }
}

#[clonable]
pub trait Processor: Clone + Send + Sync + 'static {
    fn execute(&self, input: &[u8]) -> Result<String>;
}

pub struct HtmlMinifier {
    config: minify_html::Cfg,
}

impl Clone for HtmlMinifier {
    #[allow(clippy::needless_update)]
    fn clone(&self) -> Self {
        // :(
        let config = minify_html::Cfg {
            do_not_minify_doctype: self.config.do_not_minify_doctype,
            ensure_spec_compliant_unquoted_attribute_values: self
                .config
                .ensure_spec_compliant_unquoted_attribute_values,
            keep_closing_tags: self.config.keep_closing_tags,
            keep_html_and_head_opening_tags: self.config.keep_html_and_head_opening_tags,
            keep_spaces_between_attributes: self.config.keep_spaces_between_attributes,
            keep_comments: self.config.keep_comments,
            minify_css: self.config.minify_css,
            minify_js: self.config.minify_js,
            remove_bangs: self.config.remove_bangs,
            remove_processing_instructions: self.config.remove_processing_instructions,
            ..Default::default()
        };
        Self { config }
    }
}

impl HtmlMinifier {
    #[allow(clippy::needless_update)]
    pub fn new() -> Self {
        let config = minify_html::Cfg {
            do_not_minify_doctype: true,
            ensure_spec_compliant_unquoted_attribute_values: true,
            keep_closing_tags: true,
            keep_html_and_head_opening_tags: true,
            keep_spaces_between_attributes: true,
            keep_comments: false,
            minify_css: true,
            minify_js: true,
            remove_bangs: true,
            remove_processing_instructions: true,
            ..Default::default()
        };
        Self { config }
    }
}

impl fmt::Debug for HtmlMinifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<HtmlMinify>")
    }
}

impl Default for HtmlMinifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for HtmlMinifier {
    fn execute(&self, input: &[u8]) -> Result<String> {
        let minified = minify_html::minify(input, &self.config);
        let minified_str = std::str::from_utf8(&minified)?;
        Ok(minified_str.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct CssMinifier;

impl CssMinifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CssMinifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for CssMinifier {
    fn execute(&self, input: &[u8]) -> Result<String> {
        let input = std::str::from_utf8(input)?;
        let minified = minifier::css::minify(input).map_err(|e| eyre::eyre!(e))?;
        Ok(minified.to_string())
    }
}
