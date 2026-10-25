//! Disposable home for one current-thread integration test per test binary.
use anyhow::{ensure, Result};
use std::{ffi::OsString, path::Path};

pub struct IsolatedHome {
    home: tempfile::TempDir,
    previous: Option<OsString>,
}

impl IsolatedHome {
    pub fn new() -> Result<Self> {
        let home = tempfile::tempdir()?;
        let previous = std::env::var_os("CLUAIZ_HOME");
        std::env::set_var("CLUAIZ_HOME", home.path());
        let guard = Self { home, previous };
        ensure!(
            cluaiz_core::EnvironmentManager::current()
                .local_dir
                .canonicalize()?
                == guard.path().canonicalize()?,
            "The test must resolve to its fresh disposable home"
        );
        Ok(guard)
    }

    pub fn path(&self) -> &Path {
        self.home.path()
    }
}

impl Drop for IsolatedHome {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("CLUAIZ_HOME", value),
            None => std::env::remove_var("CLUAIZ_HOME"),
        }
    }
}
