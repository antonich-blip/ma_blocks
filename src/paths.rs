use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

pub struct AppPaths {
    pub _base: PathBuf,
    pub sessions: PathBuf,
    pub images: PathBuf,
    pub data: PathBuf,
}

impl AppPaths {
    pub fn from_project_dirs() -> Option<Self> {
        ProjectDirs::from("com", "mablocks", "MaBlocks2").map(|dirs| {
            let _base = dirs.data_dir().to_path_buf();
            let sessions = _base.join("sessions");
            let images = _base.join("images");
            let data = _base.join("data");

            Self {
                _base,
                sessions,
                images,
                data,
            }
        })
    }

    pub fn ensure_dirs_exist(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.sessions)?;
        fs::create_dir_all(&self.images)?;
        fs::create_dir_all(&self.data)?;
        Ok(())
    }
}
