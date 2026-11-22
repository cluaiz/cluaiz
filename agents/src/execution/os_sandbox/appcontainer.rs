//! Windows backend of the OS sandbox: an AppContainer.
//!
//! Why AppContainer: its defaults already match the policy the Unix backends build by hand. A
//! container token can open only what an ACL grants to its SID (or to "ALL APPLICATION PACKAGES",
//! which Windows grants on `C:\Windows` and `C:\Program Files` but not on user profiles), so the
//! user's home folder is hidden without a rule; nothing is writable until we grant it; and there is
//! no network unless the spawn carries the `internetClient` capability. AppContainer also blocks
//! loopback (`localhost`), which bubblewrap and Seatbelt do not. Jan uses the same backend
//! (`competitor-repos/jan@da44e32`, `tauri-plugin-agent-tools/src/tools/appcontainer.rs`); Codex's
//! restricted-token backend with sandbox users and firewall rules was not taken: our policy has a few
//! roots and one network switch, which AppContainer expresses directly.
//!
//! There is no argument list to wrap: the container is a token attribute given to `CreateProcessW`,
//! and `std`/`tokio`/`portable-pty` commands have no hook for it. So, as Jan does, the running
//! binary is re-run as a helper (`HELPER_FLAG` first). The helper grants the ACLs, starts the real
//! command inside the container with its own std handles (the caller's pipes or terminal pass
//! through unchanged), waits, and exits with the command's code. `confine` therefore stays the same
//! shape on every platform: a program and arguments to spawn.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::SandboxPolicy;

/// The first argument of a helper run. A normal launch never has it.
pub const HELPER_FLAG: &str = "--cluaiz-os-sandbox-exec";

/// Exit code when the helper itself fails, so a setup failure is not read as the command's own
#[cfg(windows)]
const HELPER_FAILURE: i32 = 126;

/// Set when the binary's `main` called `run_helper_if_requested`. Only such a binary can be re-run
/// as the helper; any other (a test binary, a library user) gets no Windows backend instead of
/// re-running itself with arguments it does not understand.
static HELPER_READY: AtomicBool = AtomicBool::new(false);

/// FNV-1a: stable across Rust releases, unlike `DefaultHasher`
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// One container per workspace, so one project's grants never open another project
pub fn moniker(workspace: &Path) -> String {
    format!("Cluaiz.Sandbox.{:016x}", fnv1a(workspace.to_string_lossy().to_lowercase().as_bytes()))
}

/// Hidden paths that sit inside a granted root. AppContainer hides everything not granted, so only
/// these need an explicit deny (for example `.ssh` when the workspace is the home folder).
pub fn deny_paths(policy: &SandboxPolicy) -> Vec<PathBuf> {
    super::reopened(policy).into_iter().cloned().collect()
}

/// The helper's arguments: the policy, `--`, then the command
pub fn helper_args(policy: &SandboxPolicy, program: &str, args: &[String]) -> Vec<String> {
    let p = |path: &Path| path.to_string_lossy().into_owned();
    let mut out = vec![HELPER_FLAG.to_string(), if policy.allow_network { "--net" } else { "--no-net" }.to_string()];
    out.extend(["--workspace".to_string(), p(&policy.workspace)]);
    for root in policy.read_roots.iter().filter(|r| !policy.write_roots.contains(r)) {
        out.extend(["--read".to_string(), p(root)]);
    }
    for root in &policy.write_roots {
        out.extend(["--write".to_string(), p(root)]);
    }
    for hidden in deny_paths(policy) {
        out.extend(["--deny".to_string(), p(&hidden)]);
    }
    for path in &policy.read_only {
        out.extend(["--read-only".to_string(), p(path)]);
    }
    out.push("--".to_string());
    out.push(program.to_string());
    out.extend(args.iter().cloned());
    out
}

/// What a helper run was asked to do
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub allow_network: bool,
    pub workspace: PathBuf,
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub deny: Vec<PathBuf>,
    /// Inside the workspace, but no write access (git hooks and config)
    pub read_only: Vec<PathBuf>,
    pub program: String,
    pub args: Vec<String>,
}

/// Parses the helper's own arguments (without the binary name); `None` for a normal launch
pub fn parse_request<I: IntoIterator<Item = String>>(argv: I) -> Option<Request> {
    let mut it = argv.into_iter();
    if it.next()? != HELPER_FLAG {
        return None;
    }
    let mut req = Request::default();
    loop {
        match it.next()?.as_str() {
            "--net" => req.allow_network = true,
            "--no-net" => req.allow_network = false,
            "--workspace" => req.workspace = PathBuf::from(it.next()?),
            "--read" => req.read.push(PathBuf::from(it.next()?)),
            "--write" => req.write.push(PathBuf::from(it.next()?)),
            "--deny" => req.deny.push(PathBuf::from(it.next()?)),
            "--read-only" => req.read_only.push(PathBuf::from(it.next()?)),
            "--" => break,
            _ => return None,
        }
    }
    if req.workspace.as_os_str().is_empty() {
        return None;
    }
    req.program = it.next()?;
    req.args = it.collect();
    Some(req)
}

/// Quotes one argument the way `CommandLineToArgvW` and the C runtime split it: backslashes are
/// literal except before a quote, where they are doubled
fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
                continue;
            }
            _ => {}
        }
        if c != '\\' {
            out.push_str(&"\\".repeat(backslashes));
            backslashes = 0;
            out.push(c);
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// The `CreateProcessW` command line. `cmd.exe` does not split its line with `CommandLineToArgvW`:
/// after `/c` it takes the rest as typed and strips one pair of outer quotes, so its command goes in
/// as `"<command>"` (Jan's `cmd_payload` does the same).
pub fn command_line(program: &str, args: &[String]) -> String {
    let is_cmd = Path::new(program).file_stem().is_some_and(|s| s.eq_ignore_ascii_case("cmd"));
    let mut line = quote_arg(program);
    for (i, arg) in args.iter().enumerate() {
        line.push(' ');
        if is_cmd && i + 1 == args.len() && i > 0 && args[i - 1].eq_ignore_ascii_case("/c") {
            line.push('"');
            line.push_str(arg);
            line.push('"');
        } else {
            line.push_str(&quote_arg(arg));
        }
    }
    line
}

/// Whether this process can confine with AppContainer: Windows 8 or later (the isolation API exists)
/// and a binary that handles the helper flag
pub fn available() -> bool {
    HELPER_READY.load(Ordering::Relaxed) && probe()
}

#[cfg(windows)]
fn probe() -> bool {
    win::derive_sid("Cluaiz.Sandbox.Probe").is_ok()
}

#[cfg(not(windows))]
fn probe() -> bool {
    false
}

/// Call first in `main`. On a helper run it confines, runs and exits with the command's code; on a
/// normal launch it only records that this binary can serve as the helper.
pub fn run_helper_if_requested() {
    HELPER_READY.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    if let Some(req) = parse_request(std::env::args().skip(1)) {
        match win::run(&req) {
            Ok(code) => std::process::exit(code),
            Err(message) => {
                eprintln!("ERROR: OS sandbox setup failed: {message}");
                std::process::exit(HELPER_FAILURE);
            }
        }
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::core::{HRESULT, PWSTR};
    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, SetHandleInformation, ERROR_SUCCESS, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, WAIT_FAILED,
    };
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, ACCESS_MODE, DENY_ACCESS, EXPLICIT_ACCESS_W, GRANT_ACCESS,
        SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::Isolation::{CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName};
    use windows_sys::Win32::Security::{
        AddAccessAllowedAceEx, AddAce, CreateWellKnownSid, EqualSid, FreeSid, GetAce, GetLengthSid, InitializeAcl, WinCapabilityInternetClientSid, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
        ACL_REVISION, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, INHERITED_ACE, OBJECT_INHERIT_ACE, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SECURITY_CAPABILITIES, SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ};
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess, GetExitCodeProcess, InitializeProcThreadAttributeList,
        UpdateProcThreadAttribute, WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT, INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST,
        PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    };

    use super::{command_line, moniker, Request};

    /// `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)`: the profile exists from an earlier command
    const PROFILE_EXISTS: HRESULT = 0x800700B7u32 as HRESULT;

    /// `ACE_HEADER.AceType` values (winnt.h), here to avoid the large SystemServices feature
    const ACCESS_ALLOWED_ACE_TYPE: u32 = 0;
    const ACCESS_DENIED_ACE_TYPE: u32 = 1;
    /// `SE_GROUP_ENABLED` (winnt.h), same reason
    const SE_GROUP_ENABLED: u32 = 0x4;

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn os_error(code: u32) -> std::io::Error {
        std::io::Error::from_raw_os_error(code as i32)
    }

    /// A SID allocated by the isolation API
    pub struct ContainerSid(PSID);

    impl Drop for ContainerSid {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { FreeSid(self.0) };
            }
        }
    }

    pub fn derive_sid(moniker: &str) -> Result<ContainerSid, String> {
        let name = wide(OsStr::new(moniker));
        let mut sid: PSID = std::ptr::null_mut();
        let hr = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
        if hr < 0 || sid.is_null() {
            return Err(format!("could not derive an AppContainer SID (hr 0x{hr:08x})"));
        }
        Ok(ContainerSid(sid))
    }

    /// Registers the container on first use and returns its SID. Capabilities (network) are given per
    /// spawn, so the SID does not depend on them.
    fn ensure_profile(moniker: &str) -> Result<ContainerSid, String> {
        let name = wide(OsStr::new(moniker));
        let display = wide(OsStr::new("Cluaiz sandbox"));
        let description = wide(OsStr::new("Confines commands that Cluaiz agents and tools run"));
        let mut sid: PSID = std::ptr::null_mut();
        let hr = unsafe { CreateAppContainerProfile(name.as_ptr(), display.as_ptr(), description.as_ptr(), std::ptr::null(), 0, &mut sid) };
        if hr >= 0 && !sid.is_null() {
            return Ok(ContainerSid(sid));
        }
        if hr == PROFILE_EXISTS {
            return derive_sid(moniker);
        }
        Err(format!("could not create the AppContainer profile (hr 0x{hr:08x})"))
    }

    /// True when `dacl` already holds an explicit ACE of `ace_type` for `sid` covering `mask`.
    /// Checked first because applying an inheritable ACE walks the whole folder tree, which takes
    /// seconds on a project with `target/` or `node_modules/`; it should happen once, not per command.
    fn has_ace(dacl: *const ACL, sid: PSID, ace_type: u32, mask: u32) -> bool {
        if dacl.is_null() {
            return false;
        }
        let count = unsafe { (*dacl).AceCount };
        (0..count as u32).any(|i| {
            let mut ace: *mut c_void = std::ptr::null_mut();
            if unsafe { GetAce(dacl, i, &mut ace) } == 0 || ace.is_null() {
                return false;
            }
            // Allowed and denied ACEs share this layout
            let ace = ace as *const ACCESS_ALLOWED_ACE;
            let header = unsafe { &(*ace).Header };
            header.AceType as u32 == ace_type
                && header.AceFlags as u32 & INHERITED_ACE == 0
                && unsafe { (*ace).Mask } & mask == mask
                && unsafe { EqualSid(std::ptr::addr_of!((*ace).SidStart) as PSID, sid) } != 0
        })
    }

    /// Adds an inheritable ACE for the container on `path`, merged into its DACL so the user keeps
    /// their own access. Used for grants; a path inside a granted root gets less through [`limit`].
    fn set_ace(path: &Path, sid: PSID, mode: ACCESS_MODE, mask: u32) -> Result<(), String> {
        let object = wide(path.as_os_str());
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(object.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, std::ptr::null_mut(), std::ptr::null_mut(), &mut dacl, std::ptr::null_mut(), &mut descriptor)
        };
        if status != ERROR_SUCCESS {
            return Err(format!("could not read the ACL of {}: {}", path.display(), os_error(status)));
        }
        let ace_type = if mode == DENY_ACCESS { ACCESS_DENIED_ACE_TYPE } else { ACCESS_ALLOWED_ACE_TYPE };
        if has_ace(dacl, sid, ace_type, mask) {
            unsafe { LocalFree(descriptor) };
            return Ok(());
        }
        let access = EXPLICIT_ACCESS_W {
            grfAccessPermissions: mask,
            grfAccessMode: mode,
            grfInheritance: CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: 0,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: sid as PWSTR,
            },
        };
        let mut merged: *mut ACL = std::ptr::null_mut();
        let status = unsafe { SetEntriesInAclW(1, &access, dacl, &mut merged) };
        if status != ERROR_SUCCESS {
            unsafe { LocalFree(descriptor) };
            return Err(format!("could not build the ACL of {}: {}", path.display(), os_error(status)));
        }
        let status = unsafe { SetNamedSecurityInfoW(object.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, std::ptr::null_mut(), std::ptr::null_mut(), merged, std::ptr::null()) };
        unsafe {
            LocalFree(merged as *mut c_void);
            LocalFree(descriptor);
        }
        if status != ERROR_SUCCESS {
            return Err(format!("could not set the ACL of {}: {}", path.display(), os_error(status)));
        }
        Ok(())
    }

    /// Less access for the container on a path inside a root it was granted: `access` only (0: none).
    /// A deny ACE does not do it: in an AppContainer a write through the inherited workspace grant
    /// still succeeded with an explicit deny for the container on the path (measured 2026-11-22,
    /// Windows 11). So the path stops inheriting: the ACEs it inherited are copied as its own (nobody
    /// else's access changes), the container's ACEs on it are dropped, and `access` is granted.
    fn limit(path: &Path, sid: PSID, access: u32) -> Result<(), String> {
        let object = wide(path.as_os_str());
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(object.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, std::ptr::null_mut(), std::ptr::null_mut(), &mut dacl, std::ptr::null_mut(), &mut descriptor)
        };
        if status != ERROR_SUCCESS {
            return Err(format!("could not read the ACL of {}: {}", path.display(), os_error(status)));
        }
        let mut kept: Vec<Vec<u8>> = Vec::new();
        let count = if dacl.is_null() { 0 } else { unsafe { (*dacl).AceCount } };
        for i in 0..count as u32 {
            let mut ace: *mut c_void = std::ptr::null_mut();
            if unsafe { GetAce(dacl, i, &mut ace) } == 0 || ace.is_null() {
                continue;
            }
            let header = unsafe { &*(ace as *const ACE_HEADER) };
            let simple = matches!(header.AceType as u32, ACCESS_ALLOWED_ACE_TYPE | ACCESS_DENIED_ACE_TYPE);
            if simple && unsafe { EqualSid(std::ptr::addr_of!((*(ace as *const ACCESS_ALLOWED_ACE)).SidStart) as PSID, sid) } != 0 {
                continue;
            }
            let mut bytes = unsafe { std::slice::from_raw_parts(ace as *const u8, header.AceSize as usize) }.to_vec();
            bytes[1] &= !(INHERITED_ACE as u8);
            kept.push(bytes);
        }
        unsafe { LocalFree(descriptor) };
        let allow_size = std::mem::size_of::<ACCESS_ALLOWED_ACE>() - std::mem::size_of::<u32>() + unsafe { GetLengthSid(sid) } as usize;
        let size = std::mem::size_of::<ACL>() + kept.iter().map(Vec::len).sum::<usize>() + allow_size;
        // u32 items keep the ACL aligned as the API expects
        let mut buffer = vec![0u32; size.div_ceil(4)];
        let acl = buffer.as_mut_ptr() as *mut ACL;
        if unsafe { InitializeAcl(acl, (buffer.len() * 4) as u32, ACL_REVISION) } == 0 {
            return Err(format!("could not build the ACL of {}: {}", path.display(), std::io::Error::last_os_error()));
        }
        for ace in &kept {
            if unsafe { AddAce(acl, ACL_REVISION, u32::MAX, ace.as_ptr() as *const c_void, ace.len() as u32) } == 0 {
                return Err(format!("could not build the ACL of {}: {}", path.display(), std::io::Error::last_os_error()));
            }
        }
        let inherit = if path.is_dir() { CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE } else { 0 };
        if access != 0 && unsafe { AddAccessAllowedAceEx(acl, ACL_REVISION, inherit, access, sid) } == 0 {
            return Err(format!("could not build the ACL of {}: {}", path.display(), std::io::Error::last_os_error()));
        }
        let what = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
        let status = unsafe { SetNamedSecurityInfoW(object.as_ptr(), SE_FILE_OBJECT, what, std::ptr::null_mut(), std::ptr::null_mut(), acl, std::ptr::null()) };
        if status != ERROR_SUCCESS {
            return Err(format!("could not set the ACL of {}: {}", path.display(), os_error(status)));
        }
        Ok(())
    }

    /// The `internetClient` capability: outbound network for the container
    fn internet_capability(buffer: &mut Vec<u8>) -> Result<SID_AND_ATTRIBUTES, String> {
        buffer.resize(SECURITY_MAX_SID_SIZE as usize, 0);
        let mut len = buffer.len() as u32;
        if unsafe { CreateWellKnownSid(WinCapabilityInternetClientSid, std::ptr::null_mut(), buffer.as_mut_ptr() as PSID, &mut len) } == 0 {
            return Err(format!("could not build the internetClient capability: {}", std::io::Error::last_os_error()));
        }
        // Enabled: a capability with attribute 0 is present but not in force, and the container then has no
        // network at all (measured 2026-11-20: curl to 1.1.1.1 failed with attribute 0)
        Ok(SID_AND_ATTRIBUTES { Sid: buffer.as_mut_ptr() as PSID, Attributes: SE_GROUP_ENABLED })
    }

    /// This process's std handles, made inheritable so the command gets the caller's pipes or
    /// terminal. An inherited handle keeps the access it was opened with, so the container needs no
    /// rights on it.
    fn inheritable_std_handles() -> [HANDLE; 3] {
        [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|id| {
            let handle = unsafe { GetStdHandle(id) };
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) };
            }
            handle
        })
    }

    /// Puts the helper in a kill-on-close job. The command, created inside it, then ends whenever the
    /// helper ends: a timeout or cancel kills the helper, the only process the caller knows about.
    /// Best effort; without it the command can outlive a kill.
    fn end_children_with_this_process() {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return;
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        if unsafe { SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits as *const _ as *const c_void, size) } == 0 {
            unsafe { CloseHandle(job) };
            return;
        }
        // The handle stays open on purpose: it closes when this process exits, which fires the kill
        unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) };
    }

    /// The long form of the workspace without the `\\?\` prefix `canonicalize` adds (cmd refuses it).
    /// PowerShell 5.1 expands an 8.3 short name by listing each parent, which the container may not
    /// read, so the long form matters.
    fn working_dir(workspace: &Path) -> std::path::PathBuf {
        match workspace.canonicalize() {
            Ok(long) => {
                let text = long.to_string_lossy().into_owned();
                match text.strip_prefix(r"\\?\") {
                    Some(rest) if !rest.starts_with("UNC\\") => rest.into(),
                    _ => workspace.to_path_buf(),
                }
            }
            Err(_) => workspace.to_path_buf(),
        }
    }

    /// Grants, then starts the command in the container and waits. Returns the command's exit code.
    pub fn run(req: &Request) -> Result<i32, String> {
        if !req.workspace.is_dir() {
            return Err(format!("the workspace does not exist: {}", req.workspace.display()));
        }
        end_children_with_this_process();
        let sid = ensure_profile(&moniker(&req.workspace))?;
        set_ace(&req.workspace, sid.0, GRANT_ACCESS, FILE_ALL_ACCESS)?;
        for root in req.write.iter().filter(|r| r.exists()) {
            set_ace(root, sid.0, GRANT_ACCESS, FILE_ALL_ACCESS)?;
        }
        for root in req.read.iter().filter(|r| r.exists()) {
            set_ace(root, sid.0, GRANT_ACCESS, FILE_GENERIC_READ | FILE_GENERIC_EXECUTE)?;
        }
        for hidden in req.deny.iter().filter(|h| h.exists()) {
            limit(hidden, sid.0, 0)?;
        }
        for path in req.read_only.iter().filter(|p| p.exists()) {
            limit(path, sid.0, FILE_GENERIC_READ | FILE_GENERIC_EXECUTE)?;
        }

        let mut capability_sid = Vec::new();
        let mut capabilities = Vec::new();
        if req.allow_network {
            capabilities.push(internet_capability(&mut capability_sid)?);
        }
        let mut security = SECURITY_CAPABILITIES {
            AppContainerSid: sid.0,
            Capabilities: if capabilities.is_empty() { std::ptr::null_mut() } else { capabilities.as_mut_ptr() },
            CapabilityCount: capabilities.len() as u32,
            Reserved: 0,
        };

        // First call reports the size, second initialises the buffer
        let mut size: usize = 0;
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size) };
        let mut buffer = vec![0u8; size];
        let attributes = buffer.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        if unsafe { InitializeProcThreadAttributeList(attributes, 1, 0, &mut size) } == 0 {
            return Err(format!("could not prepare the spawn attributes: {}", std::io::Error::last_os_error()));
        }
        let applied = unsafe {
            UpdateProcThreadAttribute(attributes, 0, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize, &mut security as *mut _ as *const c_void, std::mem::size_of::<SECURITY_CAPABILITIES>(), std::ptr::null_mut(), std::ptr::null())
        };
        if applied == 0 {
            let error = std::io::Error::last_os_error();
            unsafe { DeleteProcThreadAttributeList(attributes) };
            return Err(format!("could not attach the AppContainer token: {error}"));
        }

        let [stdin, stdout, stderr] = inheritable_std_handles();
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin;
        startup.StartupInfo.hStdOutput = stdout;
        startup.StartupInfo.hStdError = stderr;
        startup.lpAttributeList = attributes;

        let mut line = wide(OsStr::new(&command_line(&req.program, &req.args)));
        let cwd = wide(working_dir(&req.workspace).as_os_str());
        let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // Environment: inherited from the helper, which the runner already reduced and scrubbed
        let spawned = unsafe {
            CreateProcessW(std::ptr::null(), line.as_mut_ptr(), std::ptr::null(), std::ptr::null(), 1, EXTENDED_STARTUPINFO_PRESENT, std::ptr::null(), cwd.as_ptr(), &startup.StartupInfo, &mut process)
        };
        let spawn_error = (spawned == 0).then(std::io::Error::last_os_error);
        unsafe { DeleteProcThreadAttributeList(attributes) };
        if let Some(error) = spawn_error {
            let hint = if error.raw_os_error() == Some(5) { " (a program installed under your user folder cannot be read inside the sandbox; one under Program Files can)" } else { "" };
            return Err(format!("could not start {} inside the sandbox: {error}{hint}", req.program));
        }

        let code = wait_for(process.hProcess);
        unsafe {
            CloseHandle(process.hThread);
            CloseHandle(process.hProcess);
        }
        code
    }

    fn wait_for(process: HANDLE) -> Result<i32, String> {
        if unsafe { WaitForSingleObject(process, INFINITE) } == WAIT_FAILED {
            return Err(format!("could not wait for the command: {}", std::io::Error::last_os_error()));
        }
        let mut code: u32 = 0;
        if unsafe { GetExitCodeProcess(process, &mut code) } == 0 {
            return Err(format!("could not read the exit code: {}", std::io::Error::last_os_error()));
        }
        Ok(code as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> SandboxPolicy {
        SandboxPolicy {
            workspace: PathBuf::from(r"C:\Users\u\project"),
            hidden: vec![PathBuf::from(r"C:\Users\u"), PathBuf::from(r"C:\Users\u\.ssh"), PathBuf::from(r"C:\Users\u\.cluaiz")],
            read_roots: vec![PathBuf::from(r"C:\Users\u\.cluaiz\tools\x")],
            write_roots: vec![],
            allow_network: false,
            ..Default::default()
        }
    }

    #[test]
    fn helper_args_round_trip() {
        let args = helper_args(&policy(), "cmd.exe", &["/c".into(), "echo hi".into()]);
        let req = parse_request(args).expect("a helper request");
        assert_eq!(req.workspace, PathBuf::from(r"C:\Users\u\project"));
        assert_eq!(req.read, vec![PathBuf::from(r"C:\Users\u\.cluaiz\tools\x")]);
        assert!(!req.allow_network);
        assert!(req.deny.is_empty(), "nothing hidden sits inside a granted root");
        assert_eq!((req.program.as_str(), req.args), ("cmd.exe", vec!["/c".to_string(), "echo hi".to_string()]));
    }

    #[test]
    fn git_hooks_and_config_reach_the_helper_as_read_only() {
        let mut p = policy();
        p.read_only = vec![PathBuf::from(r"C:\work\p\.git\hooks"), PathBuf::from(r"C:\work\p\.git\config")];
        let req = parse_request(helper_args(&p, "cmd.exe", &[])).unwrap();
        assert_eq!(req.read_only, p.read_only);
        assert!(req.write.is_empty(), "they never get a write grant");
    }

    #[test]
    fn a_normal_launch_is_not_a_helper_run() {
        assert!(parse_request(["serve".to_string()]).is_none());
        assert!(parse_request(Vec::<String>::new()).is_none());
        assert!(parse_request([HELPER_FLAG.to_string(), "--net".into()]).is_none(), "no command");
    }

    #[test]
    fn secrets_inside_a_granted_root_are_denied() {
        // Joined paths, not `C:\` literals, so the path logic is checked on every platform (run 28)
        let home = PathBuf::from("users").join("u");
        let p = SandboxPolicy { workspace: home.clone(), hidden: vec![home.clone(), home.join(".ssh")], ..Default::default() };
        let deny = deny_paths(&p);
        assert!(deny.contains(&home.join(".ssh")));
        assert!(!deny.contains(&home), "the workspace itself is not denied");
    }

    #[test]
    fn one_container_per_workspace() {
        assert_eq!(moniker(Path::new(r"C:\A\b")), moniker(Path::new(r"c:\a\B")));
        assert_ne!(moniker(Path::new(r"C:\A\b")), moniker(Path::new(r"C:\A\c")));
        assert!(moniker(Path::new(r"C:\A\b")).len() <= 64);
    }

    #[test]
    fn quoting_follows_the_c_runtime_rules() {
        assert_eq!(quote_arg("plain"), "plain");
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote_arg(r"C:\dir with space\"), r#""C:\dir with space\\""#);
    }

    #[test]
    fn cmd_gets_its_command_in_its_own_form() {
        let line = command_line("cmd.exe", &["/c".into(), r#"echo "a b" & dir"#.into()]);
        assert_eq!(line, r#"cmd.exe /c "echo "a b" & dir""#);
        let ps = command_line("powershell.exe", &["-Command".into(), "Get-Item 'a b'".into()]);
        assert_eq!(ps, "powershell.exe -Command \"Get-Item 'a b'\"");
    }
}
