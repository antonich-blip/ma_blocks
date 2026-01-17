use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

/// Manages the absolute paths for application-specific storage, ensuring cross-platform compatibility.
pub struct AppPaths {
    pub sessions: PathBuf,
    pub images: PathBuf,
}

impl AppPaths {
    pub fn from_project_dirs() -> Option<Self> {
        ProjectDirs::from("com", "mablocks", "MaBlocks2").map(|dirs| {
            let base = dirs.data_dir().to_path_buf();
            let sessions = base.join("sessions");
            let images = base.join("images");

            Self { sessions, images }
        })
    }

    pub fn ensure_dirs_exist(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.sessions)?;
        fs::create_dir_all(&self.images)?;
        Ok(())
    }
}
