//! Execution Security Rules & Path Protection Catalog
//!
//! Categorized lists of prohibited destructive patterns, sensitive host credential
//! paths, and safe read-only inspection prefixes across Windows, Linux, and macOS.

/// Commands strictly prohibited from executing under ANY security mode.
/// Includes root filesystem destruction, disk formatting, raw block overwrites,
/// and forced system halts.
pub const HARD_DENIED_PATTERNS: &[&str] = &[
    // POSIX / Linux / macOS root wipe & disk destruction
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "rm -rf $home",
    "mkfs",
    "mkfs.ext4",
    "mkfs.xfs",
    "mkfs.btrfs",
    "mkfs.vfat",
    "dd if=/dev/zero",
    "dd if=/dev/urandom",
    "dd if=/dev/null",
    "dd of=/dev/sd",
    "dd of=/dev/nvme",
    "dd of=/dev/disk",
    "> /dev/sda",
    "> /dev/nvme0n1",
    ":(){ :|:& };:",
    ":(){ :|: & };:",
    "chmod -r 777 /",
    "chmod -r 000 /",
    "chown -r /",
    "shutdown -h",
    "shutdown -r",
    "shutdown /s",
    "shutdown /r",
    "init 0",
    "init 6",
    "poweroff",
    "reboot",
    "halt",

    // Windows disk, filesystem & system wipe
    "format c:",
    "format d:",
    "format /q",
    "rmdir /s /q c:\\",
    "rmdir /s /q c:/",
    "rd /s /q c:\\",
    "rd /s /q c:/",
    "del /f /s /q c:\\windows",
    "del /f /s /q c:/windows",
    "del /f /s /q c:\\*",
    "diskpart",
    "clear-disk",
    "initialize-disk",
    "remove-partition",
    "format-volume",
    "stop-computer",
    "restart-computer",

    // macOS disk wiping & firmware/SIP tampering
    "diskutil erasedisk",
    "diskutil reformat",
    "diskutil unmountdisk force",
    "nvram -c",
    "csrutil disable",
];

/// Host paths containing credentials, private keys, or OS security databases.
/// Access is blocked in Sandboxed and Strict modes.
pub const SENSITIVE_SYSTEM_PATHS: &[&str] = &[
    // SSH & Cryptographic Key Material (All OS)
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
    "id_xmss",
    ".ssh/authorized_keys",
    ".ssh\\authorized_keys",
    ".ssh/known_hosts",
    ".ssh\\known_hosts",
    ".ssh/config",
    ".ssh\\config",
    ".ppk",
    "putty.ppk",

    // Cloud & DevOps CLI Credentials
    ".aws/credentials",
    ".aws\\credentials",
    ".aws/config",
    ".aws\\config",
    "aws_access_key_id",
    ".azure",
    "accesstokens.json",
    ".kube/config",
    ".kube\\config",
    "kubeconfig",
    ".config/gcloud",
    ".config\\gcloud",
    ".docker/config.json",
    ".docker\\config.json",
    ".vault-token",

    // Version Control & Package Managers Auth
    ".git-credentials",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".cargo/credentials.toml",
    ".cargo\\credentials.toml",

    // Secrets & Environment Keys
    ".env",
    ".env.local",
    ".env.production",
    ".env.staging",
    "private_key.pem",
    "server.key",
    "secrets.yaml",
    "secrets.json",

    // Shell & Console History
    ".bash_history",
    ".zsh_history",
    ".sh_history",
    "consolehost_history.txt",

    // Linux OS Security Hives
    "/etc/shadow",
    "/etc/gshadow",
    "/etc/passwd",
    "/etc/sudoers",
    "/var/log/auth.log",

    // Windows OS Security Hives
    "\\windows\\system32\\config",
    "/windows/system32/config",
    "system32\\config\\sam",
    "system32/config/sam",
    "system32\\config\\security",
    "system32/config/security",
    "system32\\config\\system",
    "system32/config/system",
    "\\windows\\system32",
    "/windows/system32",
    "ntds.dit",

    // macOS Security Keychains
    "library/keychains",
    "library\\keychains",
    "login.keychain",
    "system.keychain",
    "library/safari",
    "library\\safari",
];

/// Commands safe for idempotent inspection.
/// Permitted without prompting in RequireApproval mode.
pub const READ_ONLY_COMMAND_PREFIXES: &[&str] = &[
    // Git Read Operations
    "git status", "git log", "git diff", "git show", "git branch",
    "git tag", "git rev-parse", "git describe", "git remote",
    "git config --get", "git ls-files", "git check-ignore",

    // POSIX & Windows Filesystem Inspection
    "ls", "dir", "cat", "type", "head", "tail", "more", "less",
    "pwd", "cd", "echo", "printf", "find", "where", "which",
    "file", "stat", "wc", "du", "df",
    "grep", "rg", "findstr", "awk", "sed -n",

    // PowerShell Cmdlets & Aliases
    "get-childitem", "gci", "get-content", "gc", "get-item",
    "get-location", "gl", "select-string", "sls", "test-path",

    // Build, Test & Static Analysis (No Side Effects)
    "cargo check", "cargo test", "cargo clippy", "cargo metadata", "cargo tree",
    "npm test", "npm run lint", "npm run test", "npm list", "npm view", "npx tsc --noemit",
    "yarn test", "pnpm test",
    "pytest", "python -m unittest", "python -m pytest", "flake8", "mypy", "ruff check",
    "go test", "go vet",

    // System Diagnostics & Environment Inspection
    "uname", "whoami", "hostname", "uptime", "date", "env", "printenv",
    "free", "top -b -n 1", "ps", "netstat", "ss",
    "systeminfo", "wmic", "get-process", "get-service",
    "sw_vers", "system_profiler",
];

