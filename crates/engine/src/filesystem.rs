use std::path::PathBuf;

use directories::{BaseDirs, ProjectDirs};
use once_cell::sync::Lazy;

pub struct Directories {
    pub base: BaseDirs,
    pub project: ProjectDirs,
    pub asset: PathBuf,
}

pub static DIRS: Lazy<Directories> = Lazy::new(Directories::new);

impl Directories {
    fn new() -> Directories {
        let base = BaseDirs::new().expect("Failed to get base directories");
        let app_name = std::option_env!("APP_NAME").unwrap_or("test");
        let org = std::option_env!("ORG").unwrap_or("org");
        let organization = std::option_env!("ORGANIZATION").unwrap_or("dragonfire");
        let project = ProjectDirs::from(org, organization, app_name)
            .expect("Failed to get project directories");
        let exe_dir = std::env::current_exe()
            .map(|it| it.parent().unwrap().to_path_buf())
            .unwrap_or_else(|_| std::env::current_dir().expect("Could not get current dir"));
        let asset = exe_dir.join("asset");
        Directories {
            base,
            project,
            asset,
        }
    }
}
