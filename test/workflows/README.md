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
