use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};

use eyre::WrapErr;
use itertools::Itertools;
use tracing::{error, trace, warn};
use typed_path::{pathmarker, AbsPath, CheckedFile, CheckedFilePath, RelPath, SysPath};

use crate::{
    core::{
        page::{lint::LintResults, LintResult, RenderedPage, RenderedPageCollection},
        rules::{Mount, RuleProcessor, Rules},
        script_engine::{ScriptEngine, ScriptEngineConfig},
        Page, PageStore,
    },
    discover::html_asset::{HtmlAsset, HtmlAssets},
    Renderers, Result,
};

use super::{Engine, EnginePaths, PipelineBehavior};

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

    pub fn unhandled_assets(assets: HashSet<&HtmlAsset>) -> Result<()> {
        for asset in &assets {
            error!(asset = ?asset, "missing asset or no pipeline defined");
        }
        if !assets.is_empty() {
            bail!("one or more assets are missing");
        }
        Ok(())
    }
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

    Ok(LintResults::from_iter(lint_results))
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
        std::fs::create_dir_all(mount.target()).wrap_err_with(|| {
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
    engine_paths: Arc<EnginePaths>,
    page_store: &PageStore,
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
        .build_rules(engine_paths, page_store, rule_script)
        .wrap_err("failed to build Rules structure")?;

    Ok((script_engine, rule_processor, rules))
}

pub fn run_pipelines<'a>(
    engine: &Engine,
    html_assets: &'a HtmlAssets,
    behavior: PipelineBehavior,
) -> Result<HashSet<&'a HtmlAsset>> {
    trace!("running pipelines");

    let mut unhandled_assets = HashSet::new();

    for asset in html_assets {
        // Ignore anchor links for now. Issue https://github.com/jayson-lennon/pylon/issues/75
        // to eventually make this work.
        if asset.tag() == "a" {
            continue;
        }

        // Ignore any assets that already exist in the target directory.
        {
            if behavior == PipelineBehavior::NoOverwrite && asset.path().target().exists() {
                continue;
            }
        }

        // tracks which assets have no processing logic
        let mut asset_has_pipeline = false;

        for pipeline in engine.rules().pipelines() {
            if pipeline.is_match(asset.uri().as_str()) {
                // asset has an associate pipeline, so we won't report an error
                asset_has_pipeline = true;

                // create parent directories
                {
                    let asset_uri = asset.uri();
                    let relative_asset = &asset_uri.as_str()[1..];
                    // Make a new target in order to create directories for the asset.
                    let mut target_dir = PathBuf::from(engine.paths().output_dir());
                    target_dir.push(relative_asset);

                    let target_dir = target_dir.parent().expect("should have parent directory");
                    let target_dir = AbsPath::new(
                        engine
                            .paths()
                            .absolute_output_dir()
                            .join(&RelPath::new(target_dir)?),
                    )
                    .wrap_err("Failed to create target directory prior to pipeline processing")?;
                    crate::util::make_parent_dirs(&target_dir).wrap_err_with(|| {
                        format!(
                            "Failed creating parent directories at '{}' prior to running pipeline",
                            target_dir
                        )
                    })?;
                }

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

pub fn build_page_store(
    engine_paths: Arc<EnginePaths>,
    renderers: &Renderers,
) -> Result<PageStore> {
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
                .to_checked_file()
                .wrap_err("Failed to make CheckedFile while building page store")?;
            Page::from_file(engine_paths.clone(), checked_file_path, renderers)
        })
        .try_collect()
        .wrap_err("Failed building page collection while building page store")?;

    let mut page_store = PageStore::new();
    page_store.insert_batch(pages);

    Ok(page_store)
}

pub fn discover_html_assets<'a, F: Iterator<Item = &'a CheckedFilePath<pathmarker::Html>>>(
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

pub fn discover_html_output_files(
    engine: &Engine,
) -> Result<Vec<CheckedFilePath<pathmarker::Html>>> {
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
        .and_then(|sys_path| sys_path.to_checked_file())
    })
    .collect::<Result<Vec<_>>>()
}
