use pylonlib::core::engine::{step, Engine, EnginePaths};
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
    let actual: String = fs::read_to_string(path.as_ref())
        .unwrap_or_else(|e| format!("missing file at path '{}': {}", path.as_ref().display(), e));
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
fn exports_frontmatter() {
    let file_1 = r#"+++
+++
sample one"#;

    let file_2 = r#"+++
+++
sample two"#;
    let default_template = r#"{{ content | safe }}"#;

    let tree = temptree! {
        "rules.rhai": "",
        src: {
            dir_1: {
                "file_1.md": file_1,
                dir_2: {
                    "file_2.md": file_2,
                }
            }
        },
        templates: {
            "default.tera": default_template,
        },
        target: {},
        syntax_themes: {},
        test: {}
    };

    let engine_paths = engine_paths(&tree);

    let engine = Engine::new(engine_paths).unwrap();
    let pages = engine.library().iter().map(|(_, page)| page);
    let target = RelPath::from_relative("test");

    step::export_frontmatter(&engine, pages, &target).expect("failed to export frontmatter");

    // file 1
    {
        use serde_json::json;

        let expected = json! ({
            "template_name": "default.tera",
            "keywords": [],
            "searchable": true,
            "meta": {}
        })
        .to_string();
        assert_content(tree.path().join("test/dir_1/file_1.json"), expected);
    }
}

#[test]
fn renders_shortcodes() {
    let sample_md = r#"+++
    +++
    line1
    {{ test_shortcode(arg="hello") }} line2
    line3"#;

    let test_shortcode = r#"shortcode: {{ arg }}"#;

    let tree = temptree! {
        "rules.rhai": "",
        src: {
            "sample.md": sample_md,
        },
        templates: {
            shortcodes: {
                "test_shortcode.tera": test_shortcode,
            },
            "default.tera": "{{ content | safe }}"
        },
        target: {},
        syntax_themes: {}
    };

    let engine_paths = engine_paths(&tree);
    let engine = Engine::new(engine_paths).unwrap();
    engine.build_site().unwrap();

    let expected = "<p>line1\nshortcode: hello line2\nline3</p>\n";
    assert_content(tree.path().join("target/sample.html"), expected);
}

#[test]
fn builds_site_no_lint_errors() {
    let doc1 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

    let doc2 = r#"+++
            template_name = "test.tera"
            [meta]
            author = "test"
            +++
        "#;

    let tree = temptree! {
      "rules.rhai": "",
      templates: {
          "test.tera": r#"<img src="blank.png">"#,
          "empty.tera": "",
          "default.tera": "",
      },
      target: {},
      src: {
          "doc1.md": doc1,
          "doc2.md": doc2,
          "blank.png": "test",
      },
      syntax_themes: {}
    };

    let rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_pipeline(".", "**/*.png", ["[COPY]"]);
            "#;

    let rule_script = tree.path().join("rules.rhai");
    std::fs::write(&rule_script, &rules).unwrap();

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();
    engine.build_site().expect("failed to build site");

    let target_doc1 = tree.path().join("target").join("doc1.html");

    let target_doc2 = tree.path().join("target").join("doc2.html");

    let target_img = tree.path().join("target").join("blank.png");

    assert!(target_doc1.exists());
    assert!(target_doc2.exists());
    assert!(target_img.exists());
}
#[test]
fn aborts_site_build_with_deny_lint_error() {
    let rules = r#"
            rules.add_lint(DENY, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_pipeline("base", "**/*.png", ["[COPY]"]);
        "#;

    let doc1 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

    let doc2 = r#"+++
            template_name = "test.tera"
            [meta]
            author = "test"
            +++
        "#;

    let tree = temptree! {
      "rules.rhai": rules,
      templates: {
          "test.tera": r#"<img src="blank.png">"#,
          "empty.tera": ""
      },
      target: {},
      src: {
          "doc1.md": doc1,
          "doc2.md": doc2,
          "blank.png": "",
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();
    assert!(engine.build_site().is_err());
}

#[test]
fn copies_mounts() {
    let tree = temptree! {
      "rules.rhai": "",
      templates: {},
      target: {},
      src: {},
      wwwroot: {
          "file_1": "data",
          inner: {
              "file_2": "data"
          }
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let rules = r#"
                rules.mount("wwwroot");
            "#;

    let rule_script = tree.path().join("rules.rhai");
    std::fs::write(&rule_script, rules).unwrap();

    let engine = Engine::new(paths).unwrap();
    engine.build_site().expect("failed to build site");

    {
        let mut wwwroot = tree.path().join("target");
        wwwroot.push("wwwroot");
        assert!(!wwwroot.exists());

        let mut file_1 = tree.path().join("target");
        file_1.push("file_1");
        assert!(file_1.exists());

        let mut file_2 = tree.path().join("target");
        file_2.push("inner");
        file_2.push("file_2");
        assert!(file_2.exists());
    }
}

#[test]
fn copies_mounts_inner() {
    let tree = temptree! {
      "rules.rhai": "",
      templates: {},
      target: {},
      src: {},
      wwwroot: {
          "file_1": "data",
          inner: {
              "file_2": "data"
          }
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let rules = r#"
                rules.mount("wwwroot", "inner");
            "#;

    let rule_script = tree.path().join("rules.rhai");
    std::fs::write(&rule_script, rules).unwrap();

    let engine = Engine::new(paths).unwrap();
    engine.build_site().expect("failed to build site");

    {
        let mut wwwroot = tree.path().join("target/inner");
        wwwroot.push("wwwroot");
        assert!(!wwwroot.exists());

        let mut file_1 = tree.path().join("target/inner");
        file_1.push("file_1");
        assert!(file_1.exists());

        let mut file_2 = tree.path().join("target/inner");
        file_2.push("inner");
        file_2.push("file_2");
        assert!(file_2.exists());
    }
}
#[test]
fn doesnt_reprocess_existing_assets() {
    let doc = r#"+++
            template_name = "test.tera"
            +++"#;

    let tree = temptree! {
      "rules.rhai": "",
      templates: {
          "test.tera": r#"<img src="/found_it.png">"#,
      },
      target: {},
      src: {
          "doc.md": doc,
      },
      wwwroot: {
          "found_it.png": "",
      },
      syntax_themes: {}
    };

    let rules = r#"
                rules.mount("wwwroot", "target");
                rules.add_pipeline(".", "**/*.png", ["[COPY]"]);"#;

    let rule_script = tree.path().join("rules.rhai");
    std::fs::write(&rule_script, rules).unwrap();

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();

    // Here we go through the site building process manually in order to arrive
    // at the point where the pipelines are being processed. If the pipeline
    // processing returns an empty `LinkedAssets` structure, then this test
    // was successful. Since the file under test was copied via `mount`, the
    // pipeline should skip processing. If the pipeline returns the asset,
    // then this indicates a test failure because the asset should have been
    // located before running the pipeline.
    {
        let pages = engine.library().iter().map(|(_, page)| page);
        step::render(&engine, pages).expect("failed to render");

        step::mount_directories(engine.rules().mounts()).expect("failed to process mounts");

        let html_assets =
            pylonlib::discover::html_asset::find_all(engine.paths(), engine.paths().output_dir())
                .expect("failed to discover html assets");

        let unhandled_assets =
            step::run_pipelines(&engine, &html_assets).expect("failed to run pipelines");

        assert!(unhandled_assets.is_empty());
    }
}

#[test]
fn renders_properly_when_assets_are_available() {
    let doc = r#"+++
            template_name = "test.tera"
            +++"#;

    let tree = temptree! {
      "rules.rhai": "",
      templates: {
          "test.tera": r#"<img src="found_it.png">"#,
      },
      target: {},
      src: {
          "doc.md": doc,
          "found_it.png": "",
      },
      syntax_themes: {}
    };

    let rules = r#"rules.add_pipeline(".", "**/*.png", ["[COPY]"]);"#;

    let rule_script = tree.path().join("rules.rhai");
    std::fs::write(&rule_script, rules).unwrap();

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();

    engine.build_site().expect("failed to build site");
}

#[test]
fn aborts_render_when_assets_are_missing() {
    let doc = r#"+++
            template_name = "test.tera"
            +++"#;

    let tree = temptree! {
      "rules.rhai": "",
      templates: {
          "test.tera": r#"<img src="missing.png">"#,
      },
      target: {},
      src: {
          "doc.md": doc,
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();

    assert!(engine.build_site().is_err());
}

#[test]
fn does_render() {
    let doc1 = r#"+++
            template_name = "test.tera"
            +++
doc1"#;

    let doc2 = r#"+++
            template_name = "test.tera"
            +++
doc2"#;

    let tree = temptree! {
      "rules.rhai": "",
      templates: {
          "test.tera": "content: {{content}}"
      },
      target: {},
      src: {
          "doc1.md": doc1,
          "doc2.md": doc2,
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();

    let rendered = step::render(&engine, engine.library().iter().map(|(_, page)| page))
        .expect("failed to render pages");

    assert_eq!(rendered.iter().count(), 2);
}

#[test]
fn does_lint() {
    let rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_lint(WARN, "Missing author 2", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
        "#;

    let doc = r#"+++
            template_name = "empty.tera"
            +++
        "#;

    let tree = temptree! {
      "rules.rhai": rules,
      templates: {},
      target: {},
      src: {
          "sample.md": doc,
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();

    let lints = step::run_lints(&engine, engine.library().iter().map(|(_, page)| page))
        .expect("linting failed");
    assert_eq!(lints.into_iter().count(), 2);
}
