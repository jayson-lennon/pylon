use std::path::Path;

use pipeworks::pipeline::{AutorunTrigger, Glob, Operation, Pipeline, ShellCommand};
use pipeworks::render::html::HtmlRenderer;
use pipeworks::Directories;

fn run_render_markdown(dirs: Directories) {
    let renderer = pipeworks::render::html::TeraRenderer::new("test/templates/**/*");
    let markdown_files =
        pipeworks::discover::get_all_paths(dirs.abs_src_dir(), &|path: &Path| -> bool {
            path.extension()
                .map(|ext| ext == "md")
                .unwrap_or_else(|| false)
        })
        .unwrap();
    for path in markdown_files.iter() {
        let doc = std::fs::read_to_string(path).unwrap();
        let (frontmatter, markdown) = pipeworks::render::split_document(doc).unwrap();
        let html_content = pipeworks::render::markdown::render(&markdown);
        let mut context = tera::Context::new();
        context.insert("content", &html_content);
        dbg!(renderer.render("blog/single.tera.html", &context));
    }
}

fn run_get_all_html_paths() {
    use pipeworks::discover::get_all_paths;

    dbg!(get_all_paths("test", &|path: &Path| -> bool {
        if let Some(ext) = path.extension() {
            ext == "html"
        } else {
            false
        }
    }));
}

fn run_find_assets() {
    let sample_html = r#"
        <!DOCTYPE html>
        <meta charset="utf-8">
        <head><title>Hello, world!</title></head>
        <body>
            <h1 class="foo">Hello, <i>world!</i></h1>
            <script src="sup.js"></script>
            <img src="some image.png">
            <link href="styles.css" />
            <audio src="audio.ogg"></audio>
            <video src="video.mkv"></video>
            <object data="maths.svg"></object>
            <source src="source.mp3"></source>
            <source srcset="sourceset.mp3"></source>
            <track src="subs.txt">
        </body>
        </html>
    "#;
    let assets = pipeworks::discover::find_assets(sample_html);
    dbg!(assets);
}

fn run_pipeline(dirs: Directories) {
    // let mut copy_pipeline = Pipeline::new(dirs, Glob("*.txt".into()), AutorunTrigger::TargetGlob);
    // copy_pipeline.push_op(Operation::Copy);
    // copy_pipeline.run("sample.txt");

    let mut sed_pipeline = Pipeline::new(dirs, Glob("*.txt".into()), AutorunTrigger::TargetGlob);
    sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
        "sed 's/hello/goodbye/g' $INPUT > $OUTPUT",
    )));
    sed_pipeline.push_op(Operation::Copy);
    sed_pipeline.run("sample.txt");
}

fn main() {
    let dirs = Directories::new("test/src", "test/public");

    // run_get_all_html_paths();
    run_render_markdown(dirs);
}
