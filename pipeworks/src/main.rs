use std::path::Path;

use pipeworks::pipeline::{AutorunTrigger, Glob, Operation, Pipeline, ShellCommand};
use pipeworks::Directories;

fn main() {
    let dirs = Directories::new("test/src", "test/public");
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
