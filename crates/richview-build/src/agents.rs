//! The public agent entry points, generated from the installed vocabulary (FEA-SF-46/48).
use crate::Result;
use std::{fs, path::Path};

pub fn stage(root: &Path, public: &Path, install_enabled: bool) -> Result<()> {
    let mut text = richview_app::reference::llms_text();
    if install_enabled {
        let marker = "## Authoring\n";
        if text.matches(marker).count() != 1 {
            return Err("agent reference is missing its authoring section".into());
        }
        let install = fs::read_to_string(root.join("assets/install/llms.md"))?;
        text = text.replacen(marker, &format!("{install}{marker}"), 1);
    }
    fs::write(public.join("llms.txt"), text)?;
    let skill = public.join("skill");
    fs::create_dir_all(&skill)?;
    fs::copy(root.join("skill/SKILL.md"), skill.join("SKILL.md"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_reference_follows_the_launch_switch() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        fs::create_dir_all(root.join("assets/install")).unwrap();
        fs::create_dir(root.join("skill")).unwrap();
        fs::create_dir(root.join("web")).unwrap();
        fs::write(
            root.join("assets/install/llms.md"),
            "## Install\n\nInstall here.\n\n",
        )
        .unwrap();
        fs::write(root.join("skill/SKILL.md"), "Skill").unwrap();
        stage(root, &root.join("web"), false).unwrap();
        assert!(
            !fs::read_to_string(root.join("web/llms.txt"))
                .unwrap()
                .contains("## Install")
        );
        stage(root, &root.join("web"), true).unwrap();
        let text = fs::read_to_string(root.join("web/llms.txt")).unwrap();
        assert!(text.contains("## Install\n\nInstall here.\n\n## Authoring"));
    }
}
