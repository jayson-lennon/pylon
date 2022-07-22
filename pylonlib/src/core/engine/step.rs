use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::Path,
};

use eyre::WrapErr;
use itertools::Itertools;
use tracing::{debug, info, trace};
use typed_path::{AbsPath, ConfirmedPath, PathMarker, RelPath, SysPath};

use crate::{
    core::{
        page::{lint::LintResults, LintResult, RenderedPage, RenderedPageCollection},
        rules::{Mount, RuleProcessor, Rules},
        script_engine::{ScriptEngine, ScriptEngineConfig},
        Library, Page,
    },
    discover::{
        html_asset::{HtmlAsset, HtmlAssets},
        UrlType,
    },
    Renderers, Result, USER_LOG,
};

use super::{Engine, GlobalEnginePaths};

pub mod report {
    use std::collections::HashSet;

    use eyre::bail;
    use tracing::{error, warn};

    use crate::core::page::lint::LintResults;
    use crate::discover::html_asset::HtmlAsset;
    use crate::{Result, USER_LOG};

    pub fn lints(lints: &LintResults) -> Result<()> {
        use crate::core::page::LintLevel;

        let mut abort = false;
        for lint in lints {
            let file = lint.md_file.as_sys_path().display();
            match lint.level {
                LintLevel::Warn => {
                    warn!(target: USER_LOG, lint=%lint.msg, doc=%file);
                }
                LintLevel::Deny => {
                    error!(target: USER_LOG, lint=%lint.msg, doc=%file);
                    abort = true;
                }
            }
        }
        if abort {
            bail!("lint errors encountered while building site");
        }
        Ok(())
    }

    pub fn missing_assets(assets: &HashSet<&HtmlAsset>) -> Result<()> {
        for asset in assets {
            error!(asset = ?asset, "missing asset or no pipeline defined");
        }
        if !assets.is_empty() {
            bail!("one or more assets are missing");
        }
        Ok(())
    }
}

pub mod filter {
    use crate::discover::html_asset::HtmlAsset;

    pub fn not_on_disk(asset: &HtmlAsset) -> bool {
        !asset.asset_target_path().target().exists()
    }
}

pub fn find_unpipelined_assets<'a>(
    not_pipelined: &HashSet<&'a HtmlAsset>,
) -> HashSet<&'a HtmlAsset> {
    not_pipelined
        .iter()
        .copied()
        .filter(|asset| !asset.asset_target_path().target().exists())
        .collect::<HashSet<_>>()
}

pub fn run_lints<'a, P: IntoIterator<Item = &'a Page>>(
    engine: &Engine,
    pages: P,
) -> Result<LintResults> {
    info!(target: USER_LOG, "running lints");

    let lint_results: Vec<Vec<LintResult>> = pages
        .into_iter()
        .map(|page| crate::core::page::lint(engine.rule_processor(), engine.rules().lints(), page))
        .try_collect()
        .wrap_err("Failed building LintResult collection")?;

    let lint_results = lint_results.into_iter().flatten();

    Ok(lint_results.collect::<LintResults>())
}

pub fn render<'a, P: IntoIterator<Item = &'a Page>>(
    engine: &Engine,
    pages: P,
) -> Result<RenderedPageCollection> {
    info!(target: USER_LOG, "rendering docs");

    let rendered: Vec<RenderedPage> = pages
        .into_iter()
        .map(|page| crate::core::page::render(engine, page))
        .try_collect()
        .wrap_err("Failed building RenderedPage collection")?;

    Ok(RenderedPageCollection::from_vec(rendered))
}

pub fn mount_directories<'a, M: IntoIterator<Item = &'a Mount>>(mounts: M) -> Result<()> {
    use fs_extra::dir::CopyOptions;

    info!(target: USER_LOG, "mounting directories");

    for mount in mounts {
        debug!(
            target: USER_LOG,
            "mount {} -> {}",
            mount.src(),
            mount.target()
        );
        crate::util::make_parent_dirs(mount.target()).wrap_err_with(|| {
            format!(
                "Failed to create parent directories at '{}' while processing mounts",
                mount.target()
            )
        })?;
        let options = CopyOptions {
            copy_inside: true,
            skip_exist: true,
            content_only: true,
            ..CopyOptions::default()
        };
        fs_extra::dir::copy(mount.src(), mount.target(), &options).wrap_err_with(|| {
            format!("Failed mounting '{}' at '{}'", mount.src(), mount.target())
        })?;
    }
    Ok(())
}

pub fn load_rules(
    engine_paths: GlobalEnginePaths,
    library: &Library,
) -> Result<(ScriptEngine, RuleProcessor, Rules)> {
    let script_engine_config = ScriptEngineConfig::new();
    let script_engine = ScriptEngine::new(&script_engine_config.modules());

    let _project_root = engine_paths.project_root();

    let rule_script =
        std::fs::read_to_string(engine_paths.abs_rule_script()).wrap_err_with(|| {
            format!(
                "failed reading rule script at '{}'",
                engine_paths.abs_rule_script().display()
            )
        })?;

    let (rule_processor, rules) = script_engine
        .build_rules(engine_paths, library, rule_script)
        .wrap_err("failed to build Rules structure")?;

    Ok((script_engine, rule_processor, rules))
}

pub fn run_pipelines<'a>(
    engine: &Engine,
    html_assets: &'a HtmlAssets,
) -> Result<HashSet<&'a HtmlAsset>> {
    info!(target: USER_LOG, "running pipelines");

    let mut missing_assets: HashMap<&AbsPath, Vec<&HtmlAsset>> = HashMap::new();

    // first pass: try to run user-defined pipelines
    {
        for (target_asset, html_files) in html_assets {
            // we only need to run the pipeline one time per target asset, so
            // we just grab the first entry
            let html = html_files.get(0).unwrap();

            // TODO: https://github.com/jayson-lennon/pylon/issues/144
            if html.url_type() == &UrlType::Offsite
                || html.asset_target_uri().uri_fragment().ends_with('/')
                || html.asset_target_uri().uri_fragment().ends_with("html")
            {
                continue;
            }

            let mut asset_processed = false;

            for pipeline in engine.rules().pipelines() {
                if pipeline.is_match(html.asset_target_uri().as_str()) {
                    debug!(target: USER_LOG, pipeline=%pipeline.glob(), asset=%html.asset_target_path().uri().as_str(), "run pipeline");
                    // asset has an associated pipeline, so we won't report an error
                    asset_processed = true;

                    pipeline.run(html.asset_target_uri()).wrap_err_with(|| {
                        format!(
                            "Failed to run pipeline on asset '{}'",
                            html.asset_target_uri()
                        )
                    })?;
                }
            }
            // all assets that weren't processed need to be reported later.
            if !asset_processed {
                let entry = missing_assets.entry(target_asset).or_default();
                entry.push(html);
            }
        }
    }

    // second pass: try to copy colocated assets
    // This works by brute-force checking every page for the colocated asset. If any are found,
    // then they are removed from the `missing_assets` collection.
    {
        let copy_pipeline = {
            let paths = engine.paths();
            let paths = pipeworks::Paths::new(
                paths.project_root(),
                paths.output_dir(),
                paths.content_dir(),
            );
            let basedir = pipeworks::BaseDir::RelativeToDoc(RelPath::from_relative("."));

            pipeworks::Pipeline::with_ops(paths, &basedir, &[pipeworks::Operation::Copy])
                .wrap_err("Failed to build default copy pipeline")?
        };

        for (target_asset, html_files) in html_assets {
            // Continue to the next asset if it already exists. This happens when a pipeline
            // has already processed the asset.
            if target_asset.exists() {
                continue;
            }

            for html in html_files {
                // TODO: proper error handling -- this might fail to create directories
                // while attempting to make a copy.

                // A successful pipeline means that we copied over the asset.
                if copy_pipeline.run(html.asset_target_uri()).is_ok() {
                    missing_assets.remove(&target_asset);
                    // all pages link to this same asset, so we can bail once the asset exists
                    break;
                }
            }
        }
    }

    // Anything missing after running pipelines and colocation copies need to get reported
    Ok(missing_assets
        .into_iter()
        // The target asset is included within each HtmlAsset structure, so we flatten
        // here so the reporting mechanism can report on each individual page that references
        // the asset.
        .flat_map(|(_, assets)| assets)
        .collect())
}

#[allow(clippy::needless_pass_by_value)]
pub fn build_library(engine_paths: GlobalEnginePaths, renderers: &Renderers) -> Result<Library> {
    debug!(target: USER_LOG, "discovering documents");

    let pages: Vec<_> =
        crate::discover::get_all_paths(&engine_paths.abs_content_dir(), &|path: &Path| -> bool {
            path.extension() == Some(OsStr::new("md"))
        })
        .wrap_err("Failed to discover source pages while building page store")?
        .iter()
        .map(|abs_path| {
            let root = engine_paths.project_root();
            let base = engine_paths.content_dir();
            let target = abs_path
                .strip_prefix(root.join(base))
                .wrap_err("Failed to strip root+base from abs path while building page store")?;
            let checked_file_path = SysPath::new(root, base, &target)
                .confirm(pathmarker::MdFile)
                .wrap_err("Failed to confirm path while building page store")?;
            Page::from_file(engine_paths.clone(), checked_file_path, renderers)
        })
        .try_collect()
        .wrap_err("Failed building page collection while building page store")?;

    let mut library = Library::new();
    library.insert_batch(pages);

    Ok(library)
}

pub fn build_required_asset_list<'a, F>(engine: &Engine, files: F) -> Result<HtmlAssets>
where
    F: IntoIterator<Item = &'a ConfirmedPath<pathmarker::HtmlFile>>,
{
    use tap::prelude::*;

    debug!(target: USER_LOG, "discovering linked assets");

    let mut html_assets = HtmlAssets::new();
    for file in files {
        std::fs::read_to_string(file.as_sys_path().to_absolute_path())
            .wrap_err_with(|| {
                format!(
                    "Failed reading HTML from '{}' while discoering html assets",
                    file
                )
            })
            .and_then(|raw_html| crate::discover::html_asset::find(engine.paths(), file, &raw_html))
            .wrap_err_with(|| {
                format!("Failed to discover linked assets from HTML file '{}'", file)
            })?
            .pipe(|assets| html_assets.extend(assets));
    }
    Ok(html_assets)
}

pub fn get_all_output_files<M: PathMarker>(
    engine: &Engine,
    marker: M,
) -> Result<Vec<ConfirmedPath<M>>> {
    trace!("discovering all output HTML files");

    crate::discover::get_all_paths(engine.paths().abs_output_dir(), &|path| {
        marker
            .confirm(path)
            .expect("Failed to run PathMarker confirmation function. This is a bug.")
    })
    .wrap_err_with(|| {
        format!(
            "Failed to discover HTML files during build step at '{}'",
            engine.paths().abs_output_dir()
        )
    })?
    .iter()
    .map(|abs_path| {
        SysPath::from_abs_path(
            abs_path,
            engine.paths().project_root(),
            engine.paths().output_dir(),
        )
        .and_then(|sys_path| sys_path.confirm(marker))
    })
    .collect::<Result<Vec<_>>>()
}

pub fn export_frontmatter<'a, P>(_engine: &Engine, pages: P, target_dir: &RelPath) -> Result<()>
where
    P: IntoIterator<Item = &'a Page>,
{
    use crate::util::make_parent_dirs;

    info!(target: USER_LOG, "exporting frontmatter");

    for page in pages {
        let parent = page
            .target()
            .with_base(target_dir)
            .without_file_name()
            .to_absolute_path();
        make_parent_dirs(&parent)?;

        let file_name = page.target().with_extension("json");
        let file_name = file_name.file_name();

        let target_file = parent.join(&RelPath::from_relative(file_name));

        let frontmatter_json = serde_json::to_string(page.frontmatter())?;
        std::fs::write(target_file, &frontmatter_json)
            .wrap_err("Failed to write frontmatter to disk")?;
    }

    Ok(())
}

pub fn minify_html_files<'a, F>(engine: &Engine, html_files: F) -> Result<()>
where
    F: IntoIterator<Item = &'a ConfirmedPath<pathmarker::HtmlFile>>,
{
    let processor = engine.rules.post_processors().html_minifier();

    for file in html_files {
        let path = &file.as_sys_path().to_absolute_path();

        let content = std::fs::read_to_string(&path).wrap_err(format!(
            "Failed to read file during HTML minification process at '{}'",
            &path.display()
        ))?;

        let minified = processor
            .execute(content.as_bytes())
            .wrap_err("HTML minification failed")?;

        std::fs::write(&path, &minified).wrap_err(format!(
            "Failed to write minified HTML during minification process at '{}'",
            &path.display()
        ))?;
    }
    Ok(())
}

pub fn minify_css_files<'a, F>(engine: &Engine, css_files: F) -> Result<()>
where
    F: IntoIterator<Item = &'a ConfirmedPath<pathmarker::CssFile>>,
{
    let processor = engine.rules.post_processors().css_minifier();

    for file in css_files {
        let path = &file.as_sys_path().to_absolute_path();

        let content = std::fs::read_to_string(&path).wrap_err(format!(
            "Failed to read file during CSS minification process at '{}'",
            &path.display()
        ))?;

        let minified = processor
            .execute(content.as_bytes())
            .wrap_err("CSS minification failed")?;

        std::fs::write(&path, &minified).wrap_err(format!(
            "Failed to write minified CSS during minification process at '{}'",
            &path.display()
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::core::engine::Engine;
    use std::path::Path;
    use temptree::temptree;

    fn setup() {
        static HOOKED: once_cell::sync::OnceCell<()> = once_cell::sync::OnceCell::new();
        HOOKED.get_or_init(|| {
            let (_, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();
            eyre_hook.install().unwrap();
        });
    }

    pub fn assert_exists<P>(path: P)
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        assert!(path.exists());
    }

    #[test]
    fn pipeline_copies_colocated_assets() {
        setup();
        let sample_md = r#"+++
    published = true
    +++
    sample"#;

        let tree = temptree! {
            "rules.rhai": "",
            src: {
                "sample.md": sample_md,
                "data.png": "",
            },
            templates: {
                "default.tera": r#"<img src="data.png">"#,
            },
            target: {},
            syntax_themes: {}
        };

        let engine_paths = crate::test::default_test_paths(&tree);
        let engine = Engine::new(engine_paths).unwrap();
        engine.build_site().unwrap();

        assert_exists(tree.path().join("target/data.png"));
    }

    #[test]
    fn pipeline_copies_colocated_assets_from_another_doc() {
        setup();
        let md_relative = r#"+++
    published = true
    template_name = "relative.tera"
    +++
    sample"#;

        let md_absolute = r#"+++
    published = true
    template_name = "absolute.tera"
    +++
    sample"#;

        let tree = temptree! {
            "rules.rhai": "",
            src: {
                inner: {
                    "sample.md": md_relative,
                    "data.png": "",
                },
                "a.md": md_absolute,
                "b.md": md_absolute,
                "c.md": md_absolute,
                "d.md": md_absolute,
            },
            templates: {
                "relative.tera": r#"<img src="data.png">"#,
                "absolute.tera": r#"<img src="/inner/data.png">"#,
            },
            target: {},
            syntax_themes: {}
        };

        let engine_paths = crate::test::default_test_paths(&tree);
        let engine = Engine::new(engine_paths).unwrap();
        engine.build_site().unwrap();

        assert_exists(tree.path().join("target/inner/data.png"));
    }

    #[test]
    fn pipeline_copies_colocated_assets_from_another_doc_with_relative_path() {
        setup();
        let md_relative = r#"+++
    published = true
    template_name = "relative.tera"
    +++
    sample"#;

        let md_absolute = r#"+++
    published = true
    template_name = "absolute.tera"
    +++
    sample"#;

        let tree = temptree! {
            "rules.rhai": "",
            src: {
                inner: {
                    "sample.md": md_relative,
                    "data.png": "",
                },
                "a.md": md_absolute,
                "b.md": md_absolute,
                "c.md": md_absolute,
                "d.md": md_absolute,
            },
            templates: {
                "relative.tera": r#"<img src="data.png">"#,
                "absolute.tera": r#"<img src="inner/data.png">"#,
            },
            target: {},
            syntax_themes: {}
        };

        let engine_paths = crate::test::default_test_paths(&tree);
        let engine = Engine::new(engine_paths).unwrap();
        engine.build_site().unwrap();

        assert_exists(tree.path().join("target/inner/data.png"));

        assert_exists(tree.path().join("target/a.html"));
        assert_exists(tree.path().join("target/b.html"));
        assert_exists(tree.path().join("target/c.html"));
        assert_exists(tree.path().join("target/d.html"));
    }

    #[test]
    fn pipeline_reports_errors_on_missing_asset() {
        setup();
        let md_relative = r#"+++
    published = true
    template_name = "relative.tera"
    +++
    sample"#;

        let md_absolute = r#"+++
    published = true
    template_name = "absolute.tera"
    +++
    sample"#;

        let tree = temptree! {
            "rules.rhai": "",
            src: {
                inner: {
                    "sample.md": md_relative,
                },
                "a.md": md_absolute,
                "b.md": md_absolute,
                "c.md": md_absolute,
                "d.md": md_absolute,
            },
            templates: {
                "relative.tera": r#"<img src="data.png">"#,
                "absolute.tera": r#"<img src="/inner/data.png">"#,
            },
            target: {},
            syntax_themes: {}
        };

        let engine_paths = crate::test::default_test_paths(&tree);
        let engine = Engine::new(engine_paths).unwrap();
        assert!(engine.build_site().is_err());
    }
}
