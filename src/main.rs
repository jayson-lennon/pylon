use clap::Parser;
use color_eyre::Section;
use eyre::{eyre, WrapErr};
use pylonlib::core::engine::{Engine, EnginePaths};
use pylonlib::devserver::broker::RenderBehavior;
use pylonlib::render::highlight::SyntectHighlighter;
use pylonlib::{AbsPath, RelPath};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::{error, info, trace, warn, Level};
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
fn install_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();
}

fn main() -> Result<(), eyre::Report> {
    install_tracing();
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();

    eyre_hook.install()?;

    std::panic::set_hook(Box::new(move |pi| {
        tracing::error!("{}", panic_hook.panic_report(pi));
    }));

    let args = Args::parse();

    let paths = EnginePaths {
        rule_script: RelPath::new(&args.rule_script).wrap_err_with(|| {
            format!(
                "Failed to locate rule script at {}",
                &args.rule_script.display()
            )
        })?,
        src_dir: RelPath::new(&args.content_dir).wrap_err_with(|| {
            format!(
                "Failed to locate content dir at {}",
                &args.content_dir.display()
            )
        })?,
        syntax_theme_dir: RelPath::new(&args.syntax_themes_dir).wrap_err_with(|| {
            format!(
                "Failed to locate syntax theme dir at {}",
                &args.syntax_themes_dir.display()
            )
        })?,
        output_dir: RelPath::new(&args.output_dir).wrap_err_with(|| {
            format!(
                "Failed to locate output dir at {}",
                &args.output_dir.display()
            )
        })?,
        template_dir: RelPath::new(&args.template_dir).wrap_err_with(|| {
            format!(
                "Failed to locate template dir at {}",
                &args.template_dir.display()
            )
        })?,
        project_root: AbsPath::new(
            args.rule_script
                .canonicalize()
                .wrap_err_with(||format!("Failed to canonicalize rule script path at {}", &args.rule_script.display()))
                .suggestion(
                    "Make sure 'site-rules.rhai' exists, or set the path manually with --rule-script, or set PYLON_RULES env",
                )?
                .parent().ok_or_else(|| eyre!("Unable to determine project root from rule script"))?,
        )?,
    };
    match args.command {
        Command::Serve(opt) => {
            let (handle, _broker) = Engine::with_broker(
                Arc::new(paths),
                opt.bind,
                opt.debounce_ms,
                opt.render_behavior,
            )
            .wrap_err("Failed to initialize engine broker")?;
            println!("{:?}", handle.join());
        }
        Command::Build => {
            let engine = Engine::new(Arc::new(paths)).wrap_err("Failed to create new engine")?;
            engine.build_site().wrap_err("Failed to build site")?;
        }
        Command::BuildSyntaxTheme { path } => {
            let css_theme = SyntectHighlighter::generate_css_theme(&path).wrap_err_with(|| {
                format!(
                    "Failed to generate CSS output from theme file {}",
                    path.display()
                )
            })?;
            println!("{}", css_theme.css());
        }
    }

    Ok(())
}
