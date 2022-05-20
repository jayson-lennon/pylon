use pylonlib::core::engine::{Engine, EnginePaths};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use temptree::temptree;
use typed_path::{AbsPath, RelPath};

pub fn engine_paths(tree: &TempDir) -> Arc<EnginePaths> {
    Arc::new(EnginePaths {
        rule_script: RelPath::new("rules.rhai").unwrap(),
        src_dir: RelPath::new("src").unwrap(),
        syntax_theme_dir: RelPath::new("syntax_themes").unwrap(),
        output_dir: RelPath::new("target").unwrap(),
        template_dir: RelPath::new("templates").unwrap(),
        project_root: AbsPath::new(tree.path()).unwrap(),
    })
}

pub fn assert_content<P, S>(path: P, content: S)
where
    P: AsRef<Path>,
    S: AsRef<str>,
{
    use std::fs;
    let actual: String = fs::read_to_string(path).expect("missing file");
    assert_eq!(actual, content.as_ref());
}

#[test]
fn sample() {
    let sample_md = r#"+++
    +++
    sample"#;
    let default_template = r#"{{ content | safe }}"#;

    let tree = temptree! {
        "rules.rhai": "",
        src: {
            "sample.md": sample_md,
        },
        templates: {
            "default.tera": default_template,
        },
        target: {},
        syntax_themes: {}
    };

    let engine_paths = engine_paths(&tree);
    let engine = Engine::new(engine_paths).unwrap();
    engine.build_site().unwrap();

    assert_content(tree.path().join("target/sample.html"), "<p>sample</p>\n");
}

#[test]
fn renders_macros_in_markdown() {
    let sample_md = r#"+++
    +++
    {{ test_macro::sample(arg="hello") }}"#;

    let default_template = r#"
    {% import "test_macro.tera" as test_macro %}
    {{ content | safe }}
    "#;

    let test_macro = r#"
    {% macro sample(arg) %}
        {{ arg }}
    {% endmacro sample %}"#;

    let tree = temptree! {
        "rules.rhai": "",
        src: {
            "sample.md": sample_md,
        },
        templates: {
            "default.tera": default_template,
            "test_macro.tera": test_macro,
        },
        target: {},
        syntax_themes: {}
    };

    let engine_paths = engine_paths(&tree);
    let engine = Engine::new(engine_paths).unwrap();
    engine.build_site().unwrap();

    assert_content(tree.path().join("target/sample.html"), "hello");
}
