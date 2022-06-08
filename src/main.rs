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

pub type Result<T> = eyre::Result<T>;

#[derive(clap::Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
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
    command: SubCommand,
}

#[derive(Debug, clap::Subcommand)]
enum SubCommand {
    /// Build site
    Build,
    /// Run dev server
    Serve(CmdServe),
    /// Generate CSS theme from thTheme file
    BuildSyntaxTheme { path: PathBuf },
    /// Build search indexes & populate index services
    Index(CmdIndex),
}

#[derive(Debug, clap::Args)]
struct CmdServe {
    #[clap(long, default_value = "100", env = "PYLON_DEBOUNCE_MS")]
    debounce_ms: u64,

    #[clap(long, default_value = "127.0.0.1:8000", env = "PYLON_BIND_ADDR")]
    bind: SocketAddr,

    #[clap(long, default_value = "write", env = "PYLON_RENDER_BEHAVIOR")]
    render_behavior: RenderBehavior,
}

#[derive(Debug, clap::Args)]
struct CmdIndex {
    #[clap(subcommand)]
    command: IndexSubCommand,
}

#[derive(Debug, clap::Subcommand)]
enum IndexSubCommand {
    /// Generate an index
    Generate,
    /// Publish index to Meilisearch
    Meilisearch(MeilisearchOptions),
}

#[derive(Debug, clap::Args)]
struct MeilisearchOptions {
    /// Address for Meilisearch service
    #[clap(env = "PYLON_MEILISEARCH_ADDR")]
    address: String,

    /// API key for connecting to Meilisearch
    #[clap(env = "PYLON_MEILISEARCH_API_KEY")]
    api_key: String,

    /// Document fields to search
    #[clap(short = 'a', long = "attributes", name = "ATTRIBUTES")]
    search_attributes: Vec<String>,

    /// Primary key to use for documents (will be inferred if not provided)
    #[clap(long, name = "KEY")]
    primary_key: Option<String>,

    /// Name to use for index
    #[clap(long, default_value = "pylon", name = "NAME")]
    index_name: String,

    /// Provide pre-generated JSON document
    #[clap(long, name = "JSON-FILE")]
    use_doc: Option<PathBuf>,

    /// Add documents to index instead of rebuilding from scratch
    #[clap(long)]
    append: bool,
}

fn install_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("pylon=trace"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();
}

fn main() -> Result<()> {
    install_tracing();
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();

    eyre_hook.install()?;

    std::panic::set_hook(Box::new(move |pi| {
        tracing::error!("{}", panic_hook.panic_report(pi));
    }));

    let args = Cli::parse();

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
        SubCommand::Serve(opt) => {
            let (handle, _broker) = Engine::with_broker(
                Arc::new(paths),
                opt.bind,
                opt.debounce_ms,
                opt.render_behavior,
            )
            .wrap_err("Failed to initialize engine broker")?;
            let _ = handle.join().map_err(|e| eyre!("{:?}", e))?;
        }
        SubCommand::Build => {
            let engine = Engine::new(Arc::new(paths)).wrap_err("Failed to create new engine")?;
            engine.build_site().wrap_err("Failed to build site")?;
        }
        SubCommand::BuildSyntaxTheme { path } => {
            let css_theme = SyntectHighlighter::generate_css_theme(&path).wrap_err_with(|| {
                format!(
                    "Failed to generate CSS output from theme file {}",
                    path.display()
                )
            })?;
            println!("{}", css_theme.css());
        }
        SubCommand::Index(CmdIndex { command }) => match command {
            IndexSubCommand::Generate => {
                let docs = generate_search_docs(paths)?;
                let docs = serde_json::to_string(&docs)?;
                println!("{}", docs);
            }

            IndexSubCommand::Meilisearch(options) => {
                use searchconnector::Meilisearch;

                let docs = {
                    if let Some(path) = options.use_doc {
                        use std::fs;
                        let docs = fs::read_to_string(&path).wrap_err_with(|| {
                            format!("Failed to read provided JSON docs at '{}'", path.display())
                        })?;
                        let docs: searchdoc::SearchDocs = serde_json::from_str(&docs).unwrap();
                        docs
                    } else {
                        generate_search_docs(paths)
                            .wrap_err("Failed to generate JSON docs for indexing")?
                    }
                };

                let client = Meilisearch::new(options.address, options.api_key);
                futures::executor::block_on(async move {
                    client
                        .populate(&options.index_name, &docs, options.primary_key.as_deref())
                        .await
                        .wrap_err("Failed to populate Meilisearch")?;
                    if !options.search_attributes.is_empty() {
                        client
                            .set_searchable_attributes(
                                options.index_name,
                                options.search_attributes.as_slice(),
                            )
                            .await
                            .wrap_err("Failed to set search attributes")?;
                    }
                    Ok::<(), eyre::Report>(())
                })
                .wrap_err("Error while processing Meilisearch index")?;
            }
        },
    }

    Ok(())
}

fn generate_search_docs(paths: EnginePaths) -> Result<searchdoc::SearchDocs> {
    use pylonlib::core::engine::step::generate_search_docs;
    let engine = Engine::new(Arc::new(paths)).wrap_err("Failed to create new engine")?;
    generate_search_docs(engine.library().iter().map(|(_, page)| page))
}
