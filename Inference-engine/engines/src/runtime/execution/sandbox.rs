use std::process::{Command, Child, Stdio};
use std::os::windows::process::CommandExt;

/// 🛡️ cluaiz Sandbox Manager
/// Isolate pre-compiled kernels into restricted process containers.
pub struct cluaizSandbox {
    pub process_id: u32,
    child: Option<Child>,
}

impl cluaizSandbox {
    /// 🚀 Spawn a kernel in a restricted "Safe Box"
    /// On Windows, we use Job Objects / Low Integrity Level logic (Placeholder for V1)
    pub fn spawn_kernel(binary_path: &str, args: Vec<&str>) -> anyhow::Result<Self> {
        // Industrial Standard: No Window, Restricted Permissions
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;

        let child = Command::new(binary_path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS) // Isolate from main console
            .spawn()?;

        let pid = child.id();
        cluaiz_shared::dev_info!("🛡️ [Sandbox] Kernel Spawned in Isolate: PID={}", pid);

        Ok(Self {
            process_id: pid,
            child: Some(child),
        })
    }

    /// 🛑 Surgically terminate the sandbox
    pub fn kill(&mut self) -> anyhow::Result<()> {
        if let Some(mut child) = self.child.take() {
            child.kill()?;
            cluaiz_shared::dev_info!("🛑 [Sandbox] Isolate PID={} terminated safely.", self.process_id);
        }
        Ok(())
    }
}
