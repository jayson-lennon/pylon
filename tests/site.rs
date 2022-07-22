use pylonlib::core::engine::{step, Engine, EnginePaths, GlobalEnginePaths};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use temptree::temptree;
use typed_path::{AbsPath, RelPath};

pub fn engine_paths(tree: &TempDir) -> GlobalEnginePaths {
    Arc::new(EnginePaths {
        rule_script: RelPath::new("rules.rhai").unwrap(),
        content_dir: RelPath::new("src").unwrap(),
        syntax_theme_dir: RelPath::new("syntax_themes").unwrap(),
        output_dir: RelPath::new("target").unwrap(),
        template_dir: RelPath::new("templates").unwrap(),
        project_root: AbsPath::new(tree.path()).unwrap(),
    })
}

// TODO: double quotes are being stripped when using this function,
// but file renders have the proper quotes
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

fn setup() {
    static HOOKED: once_cell::sync::OnceCell<()> = once_cell::sync::OnceCell::new();
    HOOKED.get_or_init(|| {
        let (_, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();
        eyre_hook.install().unwrap();
    });
}

pub fn assert_exists<P>(path: P)
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    assert!(path.exists());
}

#[test]
fn sample() {
    setup();
    let sample_md = r#"+++
    published = true
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

    assert_content(tree.path().join("target/sample.html"), "<p>sample</p>");
}

#[test]
fn readme_copy() {
    setup();
    let sample_md = r#"+++
    published = true
    +++"#;
    let default_template = r#"<link href="assets/sample.png">"#;

    let rules = r#"
rules.add_pipeline(
  "",          // use the Markdown directory for the working directory
  "**/*.png",   // apply this pipeline to all .png files
  [
    OP_COPY     // run the OP_COPY builtin to copy the png file
  ]
);"#;

    let tree = temptree! {
        "rules.rhai": rules,
        src: {
            blog: {
                "page.md": sample_md,
                assets: {
                    "sample.png": "test",
                }
            }
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

    assert_content(tree.path().join("target/blog/assets/sample.png"), "test");
}

#[test]
fn readme_sass_example() {
    setup();
    let sample_md = r#"+++
    published = true
    +++"#;
    let default_template = r#"<link href="/style.css">"#;

    let rules = r#"
rules.add_pipeline(
  "/web/styles",                // working directory is <project root>/web/styles
  "/style.css",                 // only run this pipeline when this exact file is linked in the HTML
  [
    "cat main.scss > $TARGET"   // run `sass` on the `main.scss` file, and output the resulting
  ]                             // CSS code to the target file (<output root>/style.css)
);
    "#;

    let tree = temptree! {
        "rules.rhai": rules,
        src: {
            "sample.md": sample_md,
        },
        templates: {
            "default.tera": default_template,
        },
        web: {
            styles: {
                "main.scss": "sample",
            }
        },
        target: {},
        syntax_themes: {}
    };

    let engine_paths = engine_paths(&tree);
    let engine = Engine::new(engine_paths).unwrap();
    engine.build_site().unwrap();

    assert_content(tree.path().join("target/style.css"), "sample");
}

#[test]
fn readme_svg_example() {
    setup();
    let sample_md = r#"+++
    published = true
    +++
    sample"#;
    let default_template = r#"<img src="/static/img/logo.svg"><img src="/static/img/popup.svg">"#;

    let rules = r#"
rules.add_pipeline(
  "/img",                  // working directory is <project root>/img
  "/static/img/*.svg",     // only run this pipeline on SVG files requested from `/static/img`
  [
    "sed 's/#AABBCC/#123456/g' $SOURCE > $SCRATCH",  // run `sed` to replace the color in the SVG file,
                                                     // and redirect to a scratch file

    "cat $SCRATCH > $TARGET"     // minify the scratch file (which now has color #123456)
                                 // with `usvg` and output to target
  ]
);
    "#;

    let tree = temptree! {
        "rules.rhai": rules,
        src: {
            "sample.md": sample_md,
        },
        templates: {
            "default.tera": default_template,
        },
        img: {
            "logo.svg": "#AABBCC",
            "popup.svg": "#AABBCC",
        },
        target: {},
        syntax_themes: {}
    };

    let engine_paths = engine_paths(&tree);
    let engine = Engine::new(engine_paths).unwrap();
    engine.build_site().unwrap();

    assert_content(tree.path().join("target/static/img/logo.svg"), "#123456");
    assert_content(tree.path().join("target/static/img/popup.svg"), "#123456");
}

#[test]
fn exports_frontmatter() {
    setup();
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
            "use_breadcrumbs": false,
            "published": false,
            "searchable": true,
            "meta": {}
        })
        .to_string();
        assert_content(tree.path().join("test/dir_1/file_1.json"), expected);
    }
}

#[test]
fn renders_shortcodes() {
    setup();
    let sample_md = r#"+++
    published = true
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

    let expected = "<p>line1 shortcode: hello line2 line3</p>";
    assert_content(tree.path().join("target/sample.html"), expected);
}

#[test]
fn builds_site_no_lint_errors() {
    setup();
    let doc1 = r#"+++
            template_name = "empty.tera"
            published = true
            +++
        "#;

    let doc2 = r#"+++
            template_name = "test.tera"
            published = true
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
            rules.add_lint(WARN, "Missing author", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
            });
            rules.add_pipeline(".", "**/*.png", ["_COPY_"]);
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
fn aborts_site_build_with_deny_lint_error_on_unpublished_page() {
    setup();
    let rules = r#"
            rules.add_lint(DENY, "Missing author", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
            });
            rules.add_pipeline(".", "**/*.png", ["_COPY_"]);
        "#;

    let doc1 = r#"+++
            template_name = "empty.tera"
            published = false
            +++
        "#;

    let doc2 = r#"+++
            template_name = "test.tera"
            published = true
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
fn aborts_site_build_with_deny_lint_error() {
    setup();
    let rules = r#"
            rules.add_lint(DENY, "Missing author", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
            });
            rules.add_pipeline(".", "**/*.png", ["_COPY_"]);
        "#;

    let doc1 = r#"+++
            template_name = "empty.tera"
            published = true
            +++
        "#;

    let doc2 = r#"+++
            template_name = "test.tera"
            published = true
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
    setup();
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
    setup();
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
    setup();
    let doc = r#"+++
            template_name = "test.tera"
            published = true
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
                rules.add_pipeline(".", "**/*.png", ["_COPY_"]);"#;

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
    setup();
    let doc = r#"+++
            template_name = "test.tera"
            published = true
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

    let rules = r#"rules.add_pipeline(".", "**/*.png", ["_COPY_"]);"#;

    let rule_script = tree.path().join("rules.rhai");
    std::fs::write(&rule_script, rules).unwrap();

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();

    engine.build_site().expect("failed to build site");
}

#[test]
fn aborts_render_when_assets_are_missing() {
    setup();
    let doc = r#"+++
            template_name = "test.tera"
            published = true
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
    setup();
    let doc1 = r#"+++
            template_name = "test.tera"
            published = true
            +++
doc1"#;

    let doc2 = r#"+++
            template_name = "test.tera"
            published = true
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
fn skips_unpublished_docs() {
    setup();
    let doc1 = r#"+++
            template_name = "test.tera"
            published = false
            +++
doc1"#;

    let doc2 = r#"+++
            template_name = "test.tera"
            published = true
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

    engine.build_site().expect("failed to build site");

    if tree.path().join("target/doc1.html").exists() {
        panic!("html page should not exist when published == false");
    }
}

#[test]
fn does_lint() {
    setup();
    let rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
            });
            rules.add_lint(WARN, "Missing author 2", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
            });
        "#;

    let doc = r#"+++
            template_name = "empty.tera"
            published = true
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

#[test]
fn minifies_html() {
    setup();
    let sample_md = r#"+++
published = true
+++
sample"#;
    let default_template = r#"<div>{{ content | safe }}    </div>    
       <h1>test</h1>"#;

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

    assert_content(
        tree.path().join("target/sample.html"),
        "<div><p>sample</p></div><h1>test</h1>",
    );
}

#[test]
fn minifies_css() {
    setup();
    let sample_md = r#"+++
published = true
+++
sample"#;
    let default_template = "";
    let css = r#"
.test {
    color:           red;



}
"#;

    let tree = temptree! {
        "rules.rhai": "",
        src: {
            "sample.md": sample_md,
        },
        templates: {
            "default.tera": default_template,
        },
        target: {},
        syntax_themes: {},
        target: {
            "sample.css": css,
        }
    };

    let engine_paths = engine_paths(&tree);
    let engine = Engine::new(engine_paths).unwrap();
    engine.build_site().unwrap();

    assert_content(tree.path().join("target/sample.css"), ".test{color:red;}");
}

#[test]
fn local_anchors_render_without_errors() {
    setup();
    let doc1 = r#"+++
            template_name = "test.tera"
            published = true
            +++
# anchor
[test](#anchor)"#;

    let tree = temptree! {
      "rules.rhai": "",
      templates: {
          "test.tera": "content: {{content}}"
      },
      target: {},
      src: {
          "doc1.md": doc1,
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();
    engine.build_site().unwrap();

    assert_content(
        tree.path().join("target/doc1.html"),
        "content: <h1 id=anchor>anchor</h1><p><a href=#anchor>test</a></p>",
    );
}

#[test]
fn internal_doc_link_anchor_renders_without_errors() {
    setup();
    let doc1 = r#"+++
            template_name = "test.tera"
            published = true
            +++
# anchor 1"#;

    let doc2 = r#"+++
            template_name = "test.tera"
            published = true
            +++
[doc1](@/doc1.md#anchor-1)"#;

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
fn offsite_anchors_work() {
    setup();
    let doc1 = r#"+++
            template_name = "test.tera"
            published = true
            +++
[test](https://example.com/#anchor)"#;

    let tree = temptree! {
      "rules.rhai": "",
      templates: {
          "test.tera": "content: {{content}}"
      },
      target: {},
      src: {
          "doc1.md": doc1,
      },
      syntax_themes: {}
    };

    let paths = engine_paths(&tree);

    let engine = Engine::new(paths).unwrap();

    step::render(&engine, engine.library().iter().map(|(_, page)| page))
        .expect("failed to render pages");
}
