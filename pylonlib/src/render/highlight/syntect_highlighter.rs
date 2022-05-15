use itertools::Itertools;
use syntect::highlighting::ThemeSet;
use syntect::html::ClassStyle;
use syntect::html::{css_for_theme_with_class_style, line_tokens_to_classed_spans};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::Result;

use std::path::{Path, PathBuf};

pub const THEME_CLASS_PREFIX: &str = "syn-";
const THEME_CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed {
    prefix: THEME_CLASS_PREFIX,
};

#[derive(Debug, Clone)]
pub struct CssTheme {
    path: PathBuf,
    css: String,
}

impl CssTheme {
    pub fn css(&self) -> &str {
        self.css.as_str()
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

#[derive(Debug)]
pub struct SyntectHighlighter {
    syntax_set: SyntaxSet,
}

impl SyntectHighlighter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
        })
    }

    pub fn syntaxes(&self) -> &[SyntaxReference] {
        self.syntax_set.syntaxes()
    }

    pub fn get_syntax_by_token<S: AsRef<str>>(&self, token: S) -> Option<&SyntaxReference> {
        self.syntax_set.find_syntax_by_token(token.as_ref())
    }

    pub fn generate_css_theme<P: AsRef<Path>>(path: P) -> Result<CssTheme> {
        let theme = ThemeSet::get_theme(path.as_ref())?;
        let theme = CssTheme {
            path: path.as_ref().to_path_buf(),
            css: css_for_theme_with_class_style(&theme, THEME_CLASS_STYLE)?,
        };
        Ok(theme)
    }

    pub fn highlight<S: AsRef<str>>(
        &self,
        syntax: &SyntaxReference,
        code: S,
    ) -> Result<Vec<String>> {
        let mut highlighter = ClassHighlighter::new(syntax, &self.syntax_set);

        let lines = LinesWithEndings::from(code.as_ref());
        lines
            .into_iter()
            .map(|line| highlighter.highlight_line(line))
            .try_collect()
    }
}

// Highlighter taken from Zola https://github.com/getzola/zola/blob/master/components/rendering/src/codeblock/highlight.rs#L21
#[derive(Debug)]
pub struct ClassHighlighter<'s> {
    syntax_set: &'s SyntaxSet,
    open_spans: isize,
    parse_state: ParseState,
    scope_stack: ScopeStack,
}

impl<'s> ClassHighlighter<'s> {
    pub fn new(syntax: &SyntaxReference, syntax_set: &'s SyntaxSet) -> Self {
        let parse_state = ParseState::new(syntax);
        Self {
            syntax_set,
            open_spans: 0,
            parse_state,
            scope_stack: ScopeStack::new(),
        }
    }

    /// Parse the line of code and update the internal HTML buffer with tagged HTML
    ///
    /// *Note:* This function requires `line` to include a newline at the end and
    /// also use of the `load_defaults_newlines` version of the syntaxes.
    pub fn highlight_line(&mut self, line: &str) -> Result<String> {
        debug_assert!(line.ends_with('\n'));
        let parsed_line = self.parse_state.parse_line(line, self.syntax_set)?;
        let (formatted_line, delta) = line_tokens_to_classed_spans(
            line,
            parsed_line.as_slice(),
            THEME_CLASS_STYLE,
            &mut self.scope_stack,
        )?;
        self.open_spans += delta;

        Ok(formatted_line)
    }

    /// Close all open `<span>` tags and return the finished HTML string
    pub fn finalize(&mut self) -> String {
        let mut html = String::with_capacity((self.open_spans * 7) as usize);
        for _ in 0..self.open_spans {
            html.push_str("</span>");
        }
        html
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use super::*;

    #[test]
    fn creates_new_highlighter() {
        SyntectHighlighter::new().expect("failed to create syntect highlighter");
    }

    #[test]
    fn gets_syntaxes() {
        let highlighter = SyntectHighlighter::new().expect("failed to create syntect highlighter");

        assert!(!highlighter.syntaxes().is_empty());
    }

    #[test]
    fn gets_syntax_by_name() {
        let highlighter = SyntectHighlighter::new().expect("failed to create syntect highlighter");

        assert!(highlighter.get_syntax_by_token("rs").is_some());
    }

    #[test]
    fn doesnt_find_nonexistent_syntax() {
        let highlighter = SyntectHighlighter::new().expect("failed to create syntect highlighter");

        assert!(highlighter.get_syntax_by_token("NOT_A_SYNTAX").is_none());
    }

    #[test]
    fn generates_theme() {
        let css_theme = SyntectHighlighter::generate_css_theme(
            "src/render/highlight/test/material-dark.tmTheme",
        )
        .expect("failed to generate css theme");
        assert_eq!(css_theme.css, "/*\n * theme \"Material\" generated by syntect\n */\n\n.syn-code {\n color: #f8f8f2;\n background-color: #263238;\n}\n\n.syn-comment {\n color: #616161;\n}\n.syn-string {\n color: #ffd54f;\n}\n.syn-constant.syn-numeric {\n color: #7e57c2;\n}\n.syn-constant.syn-language {\n color: #7e57c2;\n}\n.syn-constant.syn-character, .syn-constant.syn-other {\n color: #7e57c2;\n}\n.syn-variable {\n color: #607d8b;\n}\n.syn-keyword {\n color: #ff5722;\n}\n.syn-storage {\n color: #e91e63;\n}\n.syn-storage.syn-type {\n color: #259b24;\n}\n.syn-entity.syn-name.syn-class {\n color: #8bc34a;\n}\n.syn-entity.syn-other.syn-inherited-class {\n color: #8bc34a;\n}\n.syn-entity.syn-name.syn-function {\n color: #009688;\n}\n.syn-variable.syn-parameter {\n color: #fd971f;\n}\n.syn-entity.syn-name.syn-tag {\n color: #26a69a;\n}\n.syn-entity.syn-other.syn-attribute-name {\n color: #ff5722;\n}\n.syn-support.syn-function {\n color: #03a9f4;\n}\n.syn-support.syn-constant {\n color: #03a9f4;\n}\n.syn-support.syn-type, .syn-support.syn-class {\n color: #607d8b;\n}\n.syn-support.syn-other.syn-variable {\n}\n.syn-invalid {\n color: #f8f8f0;\n background-color: #f92672;\n}\n.syn-invalid.syn-deprecated {\n color: #f8f8f0;\n background-color: #ae81ff;\n}\n.syn-text.syn-html.syn-markdown .syn-markup.syn-raw.syn-inline {\n color: #6a3db5;\n}\n.syn-text.syn-html.syn-markdown .syn-meta.syn-dummy.syn-line-break {\n color: #e10050;\n}\n.syn-markdown.syn-heading, .syn-markup.syn-heading, .syn-markup.syn-heading .syn-entity.syn-name, .syn-markup.syn-heading.syn-markdown, .syn-punctuation.syn-definition.syn-heading.syn-markdown {\n color: #228d1b;\nfont-weight: bold;\n}\n.syn-markup.syn-italic {\n color: #fc3e1b;\nfont-style: italic;\n}\n.syn-markup.syn-bold {\n color: #fc3e1b;\nfont-weight: bold;\n}\n.syn-markup.syn-underline {\n color: #fc3e1b;\nfont-style: underline;\n}\n.syn-markup.syn-quote, .syn-punctuation.syn-definition.syn-blockquote.syn-markdown {\n color: #fece3f;\nfont-style: italic;\n}\n.syn-markup.syn-quote {\n color: #fece3f;\nfont-style: italic;\n}\n.syn-string.syn-other.syn-link.syn-title.syn-markdown {\n color: #fb8419;\nfont-style: underline;\n}\n.syn-markup.syn-raw.syn-block {\n color: #228d1b;\n}\n.syn-punctuation.syn-definition.syn-fenced.syn-markdown, .syn-variable.syn-language.syn-fenced.syn-markdown, .syn-markup.syn-raw.syn-block.syn-fenced.syn-markdown {\n color: #228d1b;\n}\n.syn-variable.syn-language.syn-fenced.syn-markdown {\n color: #7aba3a;\n}\n.syn-punctuation.syn-definition.syn-list_item.syn-markdown, .syn-meta.syn-paragraph.syn-list.syn-markdown {\n color: #1397f1;\n}\n.syn-meta.syn-separator {\n color: #1397f1;\n background-color: #118675;\nfont-weight: bold;\n}\n.syn-markup.syn-table {\n color: #fb8419;\n}\n.syn-meta.syn-diff, .syn-meta.syn-diff.syn-header {\n color: #616161;\nfont-style: italic;\n}\n.syn-markup.syn-deleted {\n color: #e10050;\n}\n.syn-markup.syn-inserted {\n color: #7aba3a;\n}\n.syn-markup.syn-changed {\n color: #fb8419;\n}\n.syn-meta.syn-diff, .syn-meta.syn-diff.syn-range {\n color: #1397f1;\n}\n.syn-sublimelinter.syn-gutter-mark {\n color: #ffffff;\n}\n.syn-sublimelinter.syn-mark.syn-error {\n color: #d02000;\n}\n.syn-sublimelinter.syn-mark.syn-warning {\n color: #ddb700;\n}\n.syn-comment.syn-block.syn-attribute.syn-rust {\n color: #ff9800;\n}\n.syn-meta.syn-preprocessor.syn-rust {\n color: #795548;\n}\n.syn-meta.syn-namespace-block.syn-rust {\n color: #ffccbc;\n}\n.syn-support {\n color: #d1c4e9;\n}\n.syn-source.syn-json .syn-meta .syn-meta.syn-structure.syn-dictionary .syn-string {\n color: #ff5722;\n}\n.syn-source.syn-json .syn-meta .syn-meta .syn-meta.syn-structure.syn-dictionary .syn-string {\n color: #228d1b;\n}\n.syn-source.syn-json .syn-meta .syn-meta .syn-meta .syn-meta.syn-structure.syn-dictionary .syn-string {\n color: #ff5722;\n}\n.syn-source.syn-json .syn-meta .syn-meta .syn-meta .syn-meta .syn-meta.syn-structure.syn-dictionary .syn-string {\n color: #03a9f4;\n}\n.syn-source.syn-json .syn-meta .syn-meta .syn-meta .syn-meta .syn-meta .syn-meta.syn-structure.syn-dictionary .syn-string {\n color: #ffd54f;\n}\n");
    }
}

// pub fn highlight() -> Result<(), std::io::Error> {
//     // ---------------------------------------------------------------------------------------------
//     // generate html
//     let ss = SyntaxSet::load_defaults_newlines();

//     let html_file = File::create(Path::new("synhtml-css-classes.html"))?;
//     let mut html = BufWriter::new(&html_file);

//     // write html header
//     writeln!(html, "<!DOCTYPE html>")?;
//     writeln!(html, "<html>")?;
//     writeln!(html, "  <head>")?;
//     writeln!(html, "    <title>synhtml-css-classes.rs</title>")?;
//     writeln!(
//         html,
//         "    <link rel=\"stylesheet\" href=\"synhtml-css-classes.css\">"
//     )?;
//     writeln!(html, "  </head>")?;
//     writeln!(html, "  <body>")?;

//     // Rust
//     let code_rs = "// Rust source
// fn main() {
//     println!(\"Hello World!\");
// }";

//     let sr_rs = ss.find_syntax_by_extension("rs").unwrap();
//     let mut rs_html_generator =
//         ClassedHTMLGenerator::new_with_class_style(sr_rs, &ss, ClassStyle::Spaced);
//     for line in LinesWithEndings::from(code_rs) {
//         rs_html_generator.parse_html_for_line_which_includes_newline(line);
//     }
//     let html_rs = rs_html_generator.finalize();

//     writeln!(html, "<pre class=\"code\">")?;
//     writeln!(html, "{}", html_rs)?;
//     writeln!(html, "</pre>")?;

//     // C++
//     let code_cpp = "/* C++ source */
// #include <iostream>
// int main() {
//     std::cout << \"Hello World!\" << std::endl;
// }";

//     let sr_cpp = ss.find_syntax_by_extension("cpp").unwrap();
//     let mut cpp_html_generator = ClassedHTMLGenerator::new_with_class_style(
//         sr_cpp,
//         &ss,
//         ClassStyle::SpacedPrefixed { prefix: "syn-" },
//     );
//     let mut lc = 1;
//     for line in LinesWithEndings::from(code_cpp) {
//         cpp_html_generator.parse_html_for_line_which_includes_newline(line);
//         eprintln!("lc = {}", lc);
//         lc += 1;
//     }
//     let html_cpp = cpp_html_generator.finalize();

//     writeln!(html, "<pre class=\"code\">")?;
//     writeln!(html, "{}", html_cpp)?;
//     writeln!(html, "</pre>")?;

//     // write html end
//     writeln!(html, "  </body>")?;
//     writeln!(html, "</html>")?;

//     // ---------------------------------------------------------------------------------------------
//     // generate css
//     let css = "@import url(\"theme-light.css\") (prefers-color-scheme: light);
//     @import url(\"theme-dark.css\") (prefers-color-scheme: dark);
//     @media (prefers-color-scheme: dark) {
//       body {
//         background-color: gray;
//       }
//     }
//     @media (prefers-color-scheme: light) {
//       body {
//         background-color: lightgray;
//       }
//     }";

//     let css_file = File::create(Path::new("synhtml-css-classes.css"))?;
//     let mut css_writer = BufWriter::new(&css_file);

//     writeln!(css_writer, "{}", css)?;

//     // ---------------------------------------------------------------------------------------------
//     // generate css files for themes
//     let ts = ThemeSet::load_defaults();

//     // create dark color scheme css
//     let dark_theme = &ts.themes["Solarized (dark)"];
//     let css_dark_file = File::create(Path::new("theme-dark.css"))?;
//     let mut css_dark_writer = BufWriter::new(&css_dark_file);

//     let css_dark = css_for_theme_with_class_style(dark_theme, ClassStyle::Spaced);
//     writeln!(css_dark_writer, "{}", css_dark)?;

//     // create light color scheme css
//     let light_theme = &ts.themes["Solarized (light)"];
//     let css_light_file = File::create(Path::new("theme-light.css"))?;
//     let mut css_light_writer = BufWriter::new(&css_light_file);

//     let css_light = css_for_theme_with_class_style(light_theme, ClassStyle::Spaced);
//     writeln!(css_light_writer, "{}", css_light)?;

//     Ok(())
// }
