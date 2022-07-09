use pylonlib::{
    core::engine::{Engine, EnginePaths, GlobalEnginePaths},
    devserver::broker::RenderBehavior,
};
use serial_test::serial;
use std::net::{SocketAddr, SocketAddrV4};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tempfile::TempDir;
use temptree::temptree;
use typed_path::{AbsPath, RelPath};

fn setup() {
    static HOOKED: once_cell::sync::OnceCell<()> = once_cell::sync::OnceCell::new();
    HOOKED.get_or_init(|| {
        let (_, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();
        eyre_hook.install().unwrap();
    });
}

pub fn assert_err_body<A, E>(actual: A, expected_msg: E)
where
    A: AsRef<str>,
    E: AsRef<str>,
{
    use pylonlib::devserver::error_page_with_msg;

    let actual = actual.as_ref();
    let expected_msg = expected_msg.as_ref();

    let expected = error_page_with_msg(expected_msg);
    assert_eq!(actual, expected);
}

pub fn assert_ok_body<A, E>(actual: A, expected_html_content: E)
where
    A: AsRef<str>,
    E: AsRef<str>,
{
    use pylonlib::devserver::html_with_live_reload_script;

    let actual = actual.as_ref();
    let expected_html_content = expected_html_content.as_ref();

    let expected = html_with_live_reload_script(expected_html_content);
    assert_eq!(actual, expected);
}

pub fn assert_404(response: reqwest::Response) {
    use reqwest::StatusCode;
    let actual = response.status();
    assert_eq!(actual, StatusCode::NOT_FOUND);
}

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
#[serial]
fn sample() {
    setup();
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

    let addr = SocketAddrV4::from_str("127.0.0.1:38383").unwrap();
    let addr: SocketAddr = addr.into();

    let (_handle, _broker) =
        Engine::with_broker(engine_paths, addr, 0, RenderBehavior::Write).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let response = reqwest::blocking::get("http://127.0.0.1:38383/sample.html").unwrap();

    assert!(response.status().is_success());

    let body = response.text().unwrap();

    assert_ok_body(body, "<p>sample</p>\n");
}
