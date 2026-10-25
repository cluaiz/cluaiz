//! HTTP/1 request/stream semantics over an explicit local OS endpoint, never TCP.
mod connection;

use anyhow::{bail, Context, Result};
use axum::Router;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinSet};

pub const MAX_NATIVE_CONNECTIONS: usize = 32;

pub struct NativeListener {
    #[cfg(windows)]
    pending: tokio::net::windows::named_pipe::NamedPipeServer,
    #[cfg(windows)]
    endpoint: String,
    #[cfg(unix)]
    listener: tokio::net::UnixListener,
    #[cfg(unix)]
    _owner: UnixSocketOwner,
}

impl NativeListener {
    pub fn bind(endpoint: &str) -> Result<Self> {
        #[cfg(windows)]
        {
            const PREFIX: &str = r"\\.\pipe\";
            let name = endpoint
                .strip_prefix(PREFIX)
                .context("Native endpoint must be a local Windows pipe path")?;
            if name.is_empty()
                || name.len() > 180
                || name
                    .chars()
                    .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            {
                bail!("Native pipe name must be an explicit local alphanumeric identifier");
            }
            let pending = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create(endpoint)
                .context("Cannot own the native pipe endpoint")?;
            Ok(Self {
                pending,
                endpoint: endpoint.to_owned(),
            })
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
            let path = std::path::PathBuf::from(endpoint);
            if !path.is_absolute() {
                bail!("Native socket endpoint must be absolute");
            }
            let parent = path
                .parent()
                .context("Native socket needs a parent directory")?;
            if !parent.exists() {
                std::fs::DirBuilder::new().mode(0o700).create(parent)?;
            }
            let metadata = std::fs::symlink_metadata(parent)?;
            // Never alter another directory's permissions or remove a pre-existing endpoint.
            // SAFETY: geteuid has no pointer arguments or memory ownership contract.
            let effective_user = unsafe { libc::geteuid() };
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || metadata.mode() & 0o077 != 0
                || metadata.uid() != effective_user
            {
                bail!("Native socket parent must be an owned private directory (0700)");
            }
            let listener = tokio::net::UnixListener::bind(&path)
                .context("Cannot own the native socket endpoint")?;
            let metadata = std::fs::symlink_metadata(&path)?;
            if !metadata.file_type().is_socket() {
                bail!("Native endpoint is not a socket");
            }
            let owner = UnixSocketOwner {
                path: path.clone(),
                device: metadata.dev(),
                inode: metadata.ino(),
            };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self {
                listener,
                _owner: owner,
            })
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = endpoint;
            bail!("Local native transport is unsupported on this target")
        }
    }

    #[cfg(windows)]
    async fn accept(&mut self) -> Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        self.pending.connect().await?;
        // Keep a pending instance available before handing the connected instance to its task.
        let next = tokio::net::windows::named_pipe::ServerOptions::new()
            .reject_remote_clients(true)
            .create(&self.endpoint)?;
        Ok(std::mem::replace(&mut self.pending, next))
    }
    #[cfg(unix)]
    async fn accept(&mut self) -> Result<tokio::net::UnixStream> {
        Ok(self.listener.accept().await?.0)
    }

    pub async fn serve(
        mut self,
        router: Router,
        session_token: Arc<String>,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<()> {
        if session_token.is_empty() {
            bail!("Native listener requires a nonempty session token");
        }
        let (stop, receiver) = watch::channel(false);
        let mut tasks = JoinSet::new();
        tokio::pin!(shutdown);
        let outcome = loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => break Ok(()),
                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    if let Err(error) = result { tracing::debug!("Native connection task stopped: {error}"); }
                }
                accepted = self.accept(), if tasks.len() < MAX_NATIVE_CONNECTIONS => {
                    match accepted {
                        Ok(stream) => { tasks.spawn(connection::serve(stream, router.clone(), session_token.clone(), receiver.clone())); }
                        Err(error) => break Err(error),
                    }
                }
            }
        };
        let _ = stop.send(true);
        if tokio::time::timeout(Duration::from_secs(3), async {
            while tasks.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }
        outcome
    }
}

#[cfg(unix)]
struct UnixSocketOwner {
    path: std::path::PathBuf,
    device: u64,
    inode: u64,
}
#[cfg(unix)]
impl Drop for UnixSocketOwner {
    fn drop(&mut self) {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path) {
            if metadata.file_type().is_socket()
                && metadata.dev() == self.device
                && metadata.ino() == self.inode
            {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}
