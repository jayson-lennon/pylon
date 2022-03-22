use std::collections::HashMap;

use clap::{Parser, Subcommand};
use cmslib::{
    engine::{Engine, EngineConfig, FrontmatterHookResponse},
    page::{Page, PageStore},
    Renderers,
};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

// fn run_pipeline(dirs: Directories) {
//     // let mut copy_pipeline = Pipeline::new(dirs, Glob("*.txt".into()), AutorunTrigger::TargetGlob);
//     // copy_pipeline.push_op(Operation::Copy);
//     // copy_pipeline.run("sample.txt");

//     let mut sed_pipeline = Pipeline::new(dirs, Glob("*.txt".into()), AutorunTrigger::TargetGlob);
//     sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
//         "sed 's/hello/goodbye/g' $INPUT > $OUTPUT",
//     )));
//     sed_pipeline.push_op(Operation::Copy);
//     sed_pipeline.run("sample.txt");
// }

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long, default_value = "test/templates")]
    template_dir: std::path::PathBuf,

    #[clap(long, default_value = "test/public")]
    output_dir: std::path::PathBuf,

    #[clap(long, default_value = "test/src")]
    src_dir: std::path::PathBuf,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run dev server
    Serve,
    /// Build site
    Build,
}

pub fn add_copy_pipeline(engine: &mut Engine) -> Result<(), anyhow::Error> {
    use cmslib::pipeline::*;
    let mut copy_pipeline = Pipeline::new("**/*.png", AutorunTrigger::TargetGlob)?;
    copy_pipeline.push_op(Operation::Copy);
    engine.add_pipeline(copy_pipeline);
    Ok(())
}

pub fn add_sed_pipeline(engine: &mut Engine) -> Result<(), anyhow::Error> {
    use cmslib::pipeline::*;
    let mut sed_pipeline = Pipeline::new("sample.txt", AutorunTrigger::TargetGlob)?;
    sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
        r"sed 's/hello/goodbye/g' $INPUT > $OUTPUT",
    )));
    sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
        r"sed 's/bye/ day/g' $INPUT > $OUTPUT",
    )));
    sed_pipeline.push_op(Operation::Copy);

    engine.add_pipeline(sed_pipeline);
    Ok(())
}

pub fn add_frontmatter_hook(engine: &mut Engine) {
    let hook = Box::new(|page: &Page| -> FrontmatterHookResponse {
        if page.canonical_path.as_str().starts_with("/db") {
            if !page.frontmatter.meta.contains_key("section") {
                FrontmatterHookResponse::Error("require 'section' in metadata".to_owned())
            } else {
                FrontmatterHookResponse::Ok
            }
        } else {
            FrontmatterHookResponse::Ok
        }
    });
    engine.add_frontmatter_hook(hook);
}

#[actix_web::main]
async fn main() -> Result<(), anyhow::Error> {
    // use cmslib::pipeline::{AutorunTrigger, Glob, Operation, Pipeline};

    let args = Args::parse();

    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::TRACE)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let renderers = Renderers::new(&args.template_dir);

    let config = EngineConfig::new(&args.src_dir, &args.output_dir, &args.template_dir);

    let mut engine = Engine::new(config, renderers)?;

    engine.set_global_ctx(&|page_store: &PageStore| -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert(
            "globular".to_owned(),
            "haaaay db sample custom variable!".to_owned(),
        );
        map
    })?;

    add_copy_pipeline(&mut engine)?;
    add_sed_pipeline(&mut engine)?;

    add_frontmatter_hook(&mut engine);

    engine.process_frontmatter_hooks()?;

    let rendered = engine.render(Box::new(
        |page_store: &PageStore, page: &Page| -> HashMap<String, String> {
            let mut map = HashMap::new();
            map.insert(
                "dbsample".to_owned(),
                "haaaay db sample custom variable!".to_owned(),
            );
            map
        },
    ))?;
    rendered.write_to_disk()?;

    let assets = rendered.find_assets()?;

    engine.run_pipelines(&assets)?;

    match args.command {
        // Command::Serve => cmslib::serve().await?,
        // Command::Build => generator.build_all()?,
        Command::Serve => (),
        Command::Build => (),
    }

    Ok(())
}
