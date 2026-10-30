# Local CI/CD Testing Tools

Scripts to test the GitHub Actions workflows on the local machine before pushing them, which saves CI time and
keeps the commit history clean.

## 1. `run-actionlint.ps1`

**What it does:** scans every `.github/workflows/*.yml` file in about a second and reports typos, syntax errors
and invalid workflow structure. It does not need Docker.

**Run (PowerShell):**

```powershell
.\test\workflows\run-actionlint.ps1
```

"0 errors" or "passed" in the output means the workflow files are valid.

---

## 2. `run-act.ps1`

**What it does:** uses `act` and Docker to create a GitHub-like Ubuntu / Windows environment on the local machine
and runs the CI/CD pipeline in it, to confirm that the code really compiles there.

**Run:** start Docker Desktop, then in PowerShell:

```powershell
.\test\workflows\run-act.ps1
```

The first run can download several GB of Docker images, so it takes a while.

## Installer banners and live progress

`python test/workflows/test_installer_banner.py` checks Cog spacing, Windows/shell parity and the existing CLUAIZ wordmark.
`python test/workflows/test_installer_progress.py` exercises the actual downloader against a tiny local HTTP fixture: live percentage, unknown size, redirects, HTTP errors, interrupted bodies and plain logs. It uses available Windows PowerShell/PowerShell7/POSIX shells; `CLUAIZ_TEST_SH` can select a shell. No real Cluaiz installation, GPU pack or user PATH change occurs. Logs go to `.agent/installer-logo-proof`.
