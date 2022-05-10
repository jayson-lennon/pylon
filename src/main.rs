use clap::Parser;
use pylonlib::core::engine::{Engine, EnginePaths};
use pylonlib::devserver::broker::RenderBehavior;
use pylonlib::render::highlight::SyntectHighlighter;
use pylonlib::{AbsPath, RelPath};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(clap::Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long, default_value = "site-rules.rhai", env = "PYLON_RULES")]
    rule_script: PathBuf,

    #[clap(long, default_value = "public", env = "PYLON_OUTPUT")]
    output_dir: PathBuf,

    #[clap(long, default_value = "content", env = "PYLON_CONTENT")]
    content_dir: PathBuf,

    #[clap(long, default_value = "syntax_themes", env = "PYLON_SYNTAX_THEMES")]
    syntax_themes_dir: PathBuf,

    #[clap(long, default_value = "templates", env = "PYLON_TEMPLATES")]
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
    /// Generate CSS theme from thTheme file
    BuildSyntaxTheme { path: PathBuf },
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

fn main() -> Result<(), eyre::Report> {
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

    let paths = EnginePaths {
        rule_script: RelPath::new(&args.rule_script)?,
        src_dir: RelPath::new(&args.content_dir)?,
        syntax_theme_dir: RelPath::new(&args.syntax_themes_dir)?,
        output_dir: RelPath::new(&args.output_dir)?,
        template_dir: RelPath::new(&args.template_dir)?,
        project_root: AbsPath::new(
            args.rule_script
                .canonicalize()
                .expect("failed to discover project root"),
        )?,
    };
    match args.command {
        Command::Serve(opt) => {
            let (handle, _broker) = Engine::with_broker(
                Arc::new(paths),
                opt.bind,
                opt.debounce_ms,
                opt.render_behavior,
            )?;
            println!("{:?}", handle.join());
        }
        Command::Build => {
            let engine = Engine::new(Arc::new(paths))?;
            engine.build_site()?;
        }
        Command::BuildSyntaxTheme { path } => {
            let css_theme = SyntectHighlighter::generate_css_theme(path)?;
            println!("{}", css_theme.css());
        }
    }

    Ok(())
}
