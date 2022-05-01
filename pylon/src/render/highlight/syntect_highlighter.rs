use anyhow::Context;
use itertools::Itertools;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{css_for_theme_with_class_style, line_tokens_to_classed_spans};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::Result;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

pub const THEME_CLASS_PREFIX: &str = "syn-";
const THEME_CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed {
    prefix: THEME_CLASS_PREFIX,
};

#[derive(Debug, Clone)]
pub struct CssTheme {
    name: String,
    css: String,
}

impl CssTheme {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn css(&self) -> &str {
        self.css.as_str()
    }
}

#[derive(Debug)]
pub struct SyntectHighlighter {
    theme_set: ThemeSet,
    syntax_set: SyntaxSet,
}

impl SyntectHighlighter {
    pub fn new<P: AsRef<Path>>(theme_root: P) -> Result<Self> {
        let theme_set = ThemeSet::load_from_folder(theme_root)?;
        Ok(Self {
            theme_set,
            syntax_set: SyntaxSet::load_defaults_newlines(),
        })
    }

    pub fn syntaxes(&self) -> &[SyntaxReference] {
        self.syntax_set.syntaxes()
    }

    pub fn get_syntax_by_token<S: AsRef<str>>(&self, token: S) -> Option<&SyntaxReference> {
        self.syntax_set.find_syntax_by_token(token.as_ref())
    }

    pub fn get_theme<S: AsRef<str>>(&self, name: S) -> Option<&Theme> {
        self.theme_set.themes.get(name.as_ref())
    }

    pub fn generate_css_themes(&self) -> Result<Vec<CssTheme>> {
        let mut css_themes = vec![];
        for (key, theme) in &self.theme_set.themes {
            let theme = CssTheme {
                name: theme
                    .name
                    .clone()
                    .with_context(|| format!("theme file '{key}' must have a name key"))?,
                css: css_for_theme_with_class_style(theme, THEME_CLASS_STYLE),
            };
            css_themes.push(theme);
        }
        Ok(css_themes)
    }

    pub fn highlight<S: AsRef<str>>(&self, syntax: &SyntaxReference, code: S) -> Vec<String> {
        let mut highlighter = ClassHighlighter::new(syntax, &self.syntax_set);

        let lines = LinesWithEndings::from(code.as_ref());
        lines
            .into_iter()
            .map(|line| highlighter.highlight_line(line))
            .collect()
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
    pub fn highlight_line(&mut self, line: &str) -> String {
        dbg!(line);
        debug_assert!(line.ends_with('\n'));
        let parsed_line = self.parse_state.parse_line(line, self.syntax_set);
        let (formatted_line, delta) = line_tokens_to_classed_spans(
            line,
            parsed_line.as_slice(),
            THEME_CLASS_STYLE,
            &mut self.scope_stack,
        );
        self.open_spans += delta;
        formatted_line
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
    use std::path::PathBuf;

    use super::*;
    use temptree::temptree;

    impl SyntectHighlighter {
        pub fn default_test() -> Self {
            Self::new("").unwrap()
        }
    }

    #[test]
    fn creates_new_highlighter() {
        let test_data_dir = PathBuf::from("src/render/highlight/test");
        let highlighter =
            SyntectHighlighter::new(&test_data_dir).expect("failed to create syntect highlighter");
        assert_eq!(highlighter.theme_set.themes.len(), 2);
    }

    #[test]
    fn gets_syntaxes() {
        let test_data_dir = PathBuf::from("src/render/highlight/test");
        let highlighter =
            SyntectHighlighter::new(&test_data_dir).expect("failed to create syntect highlighter");

        assert!(!highlighter.syntaxes().is_empty());
    }

    #[test]
    fn gets_syntax_by_name() {
        let test_data_dir = PathBuf::from("src/render/highlight/test");
        let highlighter =
            SyntectHighlighter::new(&test_data_dir).expect("failed to create syntect highlighter");

        assert!(highlighter.get_syntax_by_token("rs").is_some());
    }

    #[test]
    fn doesnt_find_nonexistent_syntax() {
        let test_data_dir = PathBuf::from("src/render/highlight/test");
        let highlighter =
            SyntectHighlighter::new(&test_data_dir).expect("failed to create syntect highlighter");

        assert!(highlighter.get_syntax_by_token("NOT_A_SYNTAX").is_none());
    }

    #[test]
    fn generates_themes() {
        let test_data_dir = PathBuf::from("src/render/highlight/test");
        let highlighter =
            SyntectHighlighter::new(&test_data_dir).expect("failed to create syntect highlighter");

        let css_themes = highlighter
            .generate_css_themes()
            .expect("failed to generate themes");
        assert_eq!(css_themes.len(), 2);
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
