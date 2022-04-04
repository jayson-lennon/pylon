use clap::Parser;
use cmslib::core::{config::EngineConfig, engine::Engine};
use std::net::SocketAddr;
use std::path::PathBuf;

use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(clap::Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long, default_value = "test/templates", env = "CMS_TEMPLATE_DIR")]
    template_dir: PathBuf,

    #[clap(long, default_value = "test/public", env = "CMS_OUTPUT_DIR")]
    output_dir: PathBuf,

    #[clap(long, default_value = "test/src", env = "CMS_SRC_DIR")]
    src_dir: PathBuf,

    #[clap(long, default_value = "test/site-rules.rhai", env = "CMS_SITE_RULES")]
    rule_script: PathBuf,

    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Run dev server
    Serve(ServeOptions),
    /// Build site
    Build,
}

#[derive(clap::Args, Debug)]
struct ServeOptions {
    #[clap(long, default_value = "100", env = "CMS_DEBOUNCE_MS")]
    debounce_ms: u64,

    #[clap(long, default_value = "127.0.0.1:8000", env = "CMS_BIND_ADDR")]
    bind: SocketAddr,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::TRACE)
        .with_line_number(true)
        .with_env_filter("cms=trace")
        .pretty()
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let config = EngineConfig::new(
        &args.src_dir,
        &args.output_dir,
        &args.template_dir,
        &args.rule_script,
    );

    match args.command {
        Command::Serve(opt) => {
            let (handle, _broker) = Engine::with_broker(config, opt.bind, opt.debounce_ms)?;
            println!("{:?}", handle.join());
        }
        Command::Build => {
            let engine = Engine::new(config)?;
            engine.build_site()?;
        }
    }

    Ok(())
}
