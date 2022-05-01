use clap::Parser;
use pylon::core::{config::EngineConfig, engine::Engine};
use pylon::devserver::broker::RenderBehavior;
use pylon::render::highlight::SyntectHighlighter;
use std::net::SocketAddr;
use std::path::PathBuf;

use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(clap::Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long, default_value = "site-rules.rhai", env = "CMS_SITE_RULES")]
    rule_script: PathBuf,

    #[clap(long, default_value = "public", env = "CMS_OUTPUT_DIR")]
    output_dir: PathBuf,

    #[clap(long, default_value = "src", env = "CMS_SRC_DIR")]
    src_dir: PathBuf,

    #[clap(long, default_value = "syntax_themes", env = "CMS_SYNTAX_THEME_DIR")]
    syntax_themes_dir: PathBuf,

    #[clap(long, default_value = "templates", env = "CMS_TEMPLATE_DIR")]
    template_dir: PathBuf,

    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Build site
    Build,
    /// Run dev server
    Serve(ServeOptions),
    /// Syntax theme options
    BuildSyntax {
        /// thTheme directory
        theme_dir: PathBuf,
        /// output directory
        output_dir: PathBuf,
    },
}

#[derive(clap::Args, Debug)]
struct ServeOptions {
    #[clap(long, default_value = "100", env = "CMS_DEBOUNCE_MS")]
    debounce_ms: u64,

    #[clap(long, default_value = "127.0.0.1:8000", env = "CMS_BIND_ADDR")]
    bind: SocketAddr,

    #[clap(long, default_value = "write", env = "CMS_RENDER_BEHAVIOR")]
    render_behavior: RenderBehavior,
}

#[derive(clap::Subcommand, Debug)]
enum SyntaxCommand {
    /// Generates CSS from tmThemes
    Generate,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::TRACE)
        .with_line_number(true)
        .with_env_filter("pylon=trace")
        .pretty()
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let config = EngineConfig {
        rule_script: args.rule_script.clone(),
        src_root: args.src_dir.clone(),
        syntax_theme_root: args.syntax_themes_dir.clone(),
        target_root: args.output_dir.clone(),
        template_root: args.template_dir.clone(),
    };
    match args.command {
        Command::Serve(opt) => {
            let (handle, _broker) =
                Engine::with_broker(config, opt.bind, opt.debounce_ms, opt.render_behavior)?;
            println!("{:?}", handle.join());
        }
        Command::Build => {
            let engine = Engine::new(config)?;
            engine.build_site()?;
        }
        Command::BuildSyntax {
            theme_dir,
            output_dir,
        } => {
            let highlighter = SyntectHighlighter::new(theme_dir)?;
            let themes = highlighter.generate_css_themes()?;
            for theme in themes {
                let mut output_path = output_dir.clone();
                output_path.push(theme.name());
                std::fs::write(&output_path, theme.css())?;
            }
        }
    }

    Ok(())
}
