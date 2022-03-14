use pipeworks::pipeline::{AutorunTrigger, Glob, Operation, Pipeline, ShellCommand};
use pipeworks::Directories;

fn run_get_all_html_paths() {
    use pipeworks::discover::get_all_html_paths;

    dbg!(get_all_html_paths("test"));
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

    run_get_all_html_paths();
}
