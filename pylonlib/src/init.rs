use eyre::{eyre, WrapErr};
use typed_path::{AbsPath};

use crate::Result;

#[rustfmt::skip]
pub fn at_target(target: &AbsPath) -> Result<()> {
    use std::fs;

    let contents = fs::create_dir_all(target).and_then(|_| fs::read_dir(target)).wrap_err("Failed to make dir: target")?;

    if contents.count() > 0 {
        Err(eyre!("target directory must be empty"))
    } else {
        let target = target.as_path_buf();
        fs::create_dir_all(target.join("content")).wrap_err("Failed to make dir: content")?;
        fs::create_dir_all(target.join("web/templates/content")).wrap_err("Failed to make dir: web/templates/content")?;
        fs::create_dir_all(target.join("web/templates/macros")).wrap_err("Failed to make dir: web/templates/macros")?;
        fs::create_dir_all(target.join("web/templates/partials")).wrap_err("Failed to make dir: web/templates/partials")?;
        fs::create_dir_all(target.join("web/templates/shortcodes")).wrap_err("Failed to make dir: web/templates/shortcodes")?;
        fs::create_dir_all(target.join("web/wwwroot")).wrap_err("Failed to make dir: wwwroot")?;

        fs::write(target.join("content/index.md"), include_str!("init-resource/content/index.md")).wrap_err("Failed to make file: content/index.md")?;
        fs::write(target.join("web/templates/index.tera"), include_str!("init-resource/web/templates/index.tera")).wrap_err("Failed to make file: web/templates/index.tera")?;
        fs::write(target.join("web/templates/content/default.tera"), include_str!("init-resource/web/templates/content/default.tera")).wrap_err("Failed to make file: web/templates/content/default.tera")?;
        fs::write(target.join("web/templates/partials/head.tera"), include_str!("init-resource/web/templates/partials/head.tera")).wrap_err("Failed to make file: web/templates/partials/head.tera")?;
        fs::write(target.join("web/templates/macros/text.tera"), include_str!("init-resource/web/templates/macros/text.tera")).wrap_err("Failed to make file: web/templates/macros/text.tera")?;
        fs::write(target.join("web/templates/shortcodes/big.tera"), include_str!("init-resource/web/templates/shortcodes/big.tera")).wrap_err("Failed to make file: web/templates/shortcodes/big.tera")?;
        fs::write(target.join("site-rules.rhai"), include_str!("init-resource/site-rules.rhai")).wrap_err("Failed to make file: site-rules.rhai")?;
        fs::write(target.join(".gitignore"), include_str!("init-resource/.gitignore")).wrap_err("Failed to make file: .gitignore")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    
    use temptree::temptree;
    use typed_path::AbsPath;

    use super::at_target;

    fn assert_exists(path: &Path) {
        assert!(path.exists(), "Missing path: {}", path.display());
    }

    #[rustfmt::skip]
    fn ensure_all_paths_exist(target: &Path) {
        // dirs
        assert_exists(&target.join("content"));
        assert_exists(&target.join("web/templates/content"));
        assert_exists(&target.join("web/templates/macros"));
        assert_exists(&target.join("web/templates/partials"));
        assert_exists(&target.join("web/templates/shortcodes"));
        assert_exists(&target.join("web/wwwroot"));

        // files
        assert_exists(&target.join("content/index.md"));
        assert_exists(&target.join("web/templates/index.tera"));
        assert_exists(&target.join("web/templates/content/default.tera"));
        assert_exists(&target.join("web/templates/partials/head.tera"));
        assert_exists(&target.join("web/templates/macros/text.tera"));
        assert_exists(&target.join("web/templates/shortcodes/big.tera"));
        assert_exists(&target.join("site-rules.rhai"));
        assert_exists(&target.join(".gitignore"));
    }

    #[test]
    fn inits_when_empty() {
        let tree = temptree! {
            target: {},
        };
        at_target(&AbsPath::from_absolute(tree.path().join("target")))
            .expect("failed to initialize new project");
        ensure_all_paths_exist(&tree.path().join("target"));
    }

    #[test]
    fn aborts_when_entries_exist() {
        let tree = temptree! {
            target: {
                "stuff": "",
            },
        };
        let created = at_target(&AbsPath::from_absolute(tree.path().join("target")));
        assert!(created.is_err());
    }

    #[test]
    fn creates_target_dir_if_needed() {
        let tree = temptree! {
            target: { }
        };
        at_target(&AbsPath::from_absolute(tree.path().join("target/inner")))
            .expect("failed to initialize new project");
        ensure_all_paths_exist(&tree.path().join("target/inner"));
    }
}
