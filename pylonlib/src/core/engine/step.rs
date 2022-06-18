use std::{collections::HashSet, ffi::OsStr, path::Path};

use eyre::WrapErr;
use itertools::Itertools;
use tracing::trace;
use typed_path::{ConfirmedPath, RelPath, SysPath};

use crate::{
    core::{
        page::{lint::LintResults, LintResult, RenderedPage, RenderedPageCollection},
        rules::{Mount, RuleProcessor, Rules},
        script_engine::{ScriptEngine, ScriptEngineConfig},
        Library, Page,
    },
    discover::html_asset::{HtmlAsset, HtmlAssets},
    Renderers, Result,
};

use super::{Engine, GlobalEnginePaths};

pub mod report {
    use std::collections::HashSet;

    use eyre::bail;
    use tracing::{error, warn};

    use crate::core::page::lint::LintResults;
    use crate::discover::html_asset::HtmlAsset;
    use crate::Result;

    pub fn lints(lints: &LintResults) -> Result<()> {
        use crate::core::page::LintLevel;

        let mut abort = false;
        for lint in lints {
            match lint.level {
                LintLevel::Warn => warn!(%lint.msg),
                LintLevel::Deny => {
                    error!(%lint.msg);
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
        !asset.path().target().exists()
    }
}

pub fn find_unpipelined_assets<'a>(
    not_pipelined: &HashSet<&'a HtmlAsset>,
) -> HashSet<&'a HtmlAsset> {
    not_pipelined
        .iter()
        .copied()
        .filter(|asset| !asset.path().target().exists())
        .collect::<HashSet<_>>()
}

pub fn run_lints<'a, P: Iterator<Item = &'a Page>>(
    engine: &Engine,
    pages: P,
) -> Result<LintResults> {
    trace!("linting");
    let lint_results: Vec<Vec<LintResult>> = pages
        .map(|page| crate::core::page::lint(engine.rule_processor(), engine.rules().lints(), page))
        .try_collect()
        .wrap_err("Failed building LintResult collection")?;

    let lint_results = lint_results.into_iter().flatten();

    Ok(lint_results.collect::<LintResults>())
}

pub fn render<'a, P: Iterator<Item = &'a Page>>(
    engine: &Engine,
    pages: P,
) -> Result<RenderedPageCollection> {
    trace!("rendering");

    let rendered: Vec<RenderedPage> = pages
        .map(|page| crate::core::page::render(engine, page))
        .try_collect()
        .wrap_err("Failed building RenderedPage collection")?;

    Ok(RenderedPageCollection::from_vec(rendered))
}

pub fn mount_directories<'a, M: Iterator<Item = &'a Mount>>(mounts: M) -> Result<()> {
    use fs_extra::dir::CopyOptions;
    for mount in mounts {
        trace!(mount=?mount, "mounting");
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
        std::fs::read_to_string(engine_paths.absolute_rule_script()).wrap_err_with(|| {
            format!(
                "failed reading rule script at '{}'",
                engine_paths.absolute_rule_script().display()
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
    trace!("running pipelines");

    let mut unhandled_assets = HashSet::new();

    for asset in html_assets {
        // Ignore anchor links for now. Issue https://github.com/jayson-lennon/pylon/issues/75
        // to eventually make this work.
        if asset.tag() == "a" {
            continue;
        }

        // tracks which assets have no processing logic
        let mut asset_has_pipeline = false;

        for pipeline in engine.rules().pipelines() {
            if pipeline.is_match(asset.uri().as_str()) {
                // asset has an associate pipeline, so we won't report an error
                asset_has_pipeline = true;

                pipeline.run(asset.uri()).wrap_err_with(|| {
                    format!("Failed to run pipeline on asset '{}'", asset.uri())
                })?;
            }
        }
        if !asset_has_pipeline {
            unhandled_assets.insert(asset);
        }
    }
    Ok(unhandled_assets)
}

#[allow(clippy::needless_pass_by_value)]
pub fn build_library(engine_paths: GlobalEnginePaths, renderers: &Renderers) -> Result<Library> {
    let pages: Vec<_> =
        crate::discover::get_all_paths(&engine_paths.absolute_src_dir(), &|path: &Path| -> bool {
            path.extension() == Some(OsStr::new("md"))
        })
        .wrap_err("Failed to discover source pages while building page store")?
        .iter()
        .map(|abs_path| {
            let root = engine_paths.project_root();
            let base = engine_paths.src_dir();
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

pub fn build_required_asset_list<
    'a,
    F: Iterator<Item = &'a ConfirmedPath<pathmarker::HtmlFile>>,
>(
    engine: &Engine,
    files: F,
) -> Result<HtmlAssets> {
    use tap::prelude::*;

    trace!("locating HTML assets");
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
    html_assets.drop_offsite();
    Ok(html_assets)
}

pub fn get_all_html_output_files(
    engine: &Engine,
) -> Result<Vec<ConfirmedPath<pathmarker::HtmlFile>>> {
    crate::discover::get_all_paths(engine.paths().absolute_output_dir(), &|path| {
        path.extension() == Some(OsStr::new("html"))
    })
    .wrap_err_with(|| {
        format!(
            "Failed to discover HTML files during build step at '{}'",
            engine.paths().absolute_output_dir()
        )
    })?
    .iter()
    .map(|abs_path| {
        SysPath::from_abs_path(
            abs_path,
            engine.paths().project_root(),
            engine.paths().output_dir(),
        )
        .and_then(|sys_path| sys_path.confirm(pathmarker::HtmlFile))
    })
    .collect::<Result<Vec<_>>>()
}

pub fn export_frontmatter<'a, P>(_engine: &Engine, pages: P, target_dir: &RelPath) -> Result<()>
where
    P: Iterator<Item = &'a Page>,
{
    use crate::util::make_parent_dirs;

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
