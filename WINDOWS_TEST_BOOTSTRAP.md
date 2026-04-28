# Windows Test Environment Bootstrap

A from-zero guide to provisioning a Windows 10/11 machine for running and writing
tests across common stacks. Work top-down; each section is independent, so skip
the runtimes you don't need.

> Run an **elevated PowerShell** (Right-click PowerShell → *Run as Administrator*)
> for everything below unless noted. Verify with `whoami /groups | findstr S-1-16-12288`.

---

## 0. Prerequisites

| Item | Why |
|------|-----|
| Windows 10 21H2+ or Windows 11 | `winget` ships in-box, WSL2 works reliably |
| ~30 GB free disk | Toolchains, container images, browser binaries |
| Admin rights | Most installers and WSL features need it |
| Stable network | Several gigabytes of downloads |

Check Windows version:

```powershell
winver
```

Enable script execution for the current user (one-time):

```powershell
Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned
```

---

## 1. Package Managers

Pick **one** primary; `winget` is the safe default. Scoop is nice for
user-scope CLIs without UAC prompts.

### winget (built-in)

```powershell
winget --version
winget source update
```

If missing, install *App Installer* from the Microsoft Store.

### Scoop (optional, user-scope)

```powershell
Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned
Invoke-RestMethod -Uri https://get.scoop.sh | Invoke-Expression
scoop bucket add extras
scoop bucket add versions
```

### Chocolatey (optional, machine-scope)

```powershell
Set-ExecutionPolicy Bypass -Scope Process -Force
[System.Net.ServicePointManager]::SecurityProtocol = 3072
iwr https://community.chocolatey.org/install.ps1 -UseBasicParsing | iex
```

---

## 2. Core Tooling

```powershell
winget install --id Git.Git              -e --source winget
winget install --id GitHub.cli           -e --source winget
winget install --id Microsoft.VisualStudioCode -e --source winget
winget install --id Microsoft.PowerShell -e --source winget   # PowerShell 7
winget install --id Microsoft.WindowsTerminal  -e --source winget
```

Configure git identity and sane line-ending defaults:

```powershell
git config --global user.name  "Your Name"
git config --global user.email "you@example.com"
git config --global init.defaultBranch main
git config --global core.autocrlf input
git config --global pull.rebase true
```

---

## 3. WSL2 (recommended for cross-platform tests)

```powershell
wsl --install -d Ubuntu
wsl --set-default-version 2
wsl --status
```

Reboot when prompted. After first launch, set a UNIX username and run:

```bash
sudo apt-get update && sudo apt-get -y upgrade
sudo apt-get install -y build-essential curl git unzip
```

Use WSL for Linux-flavored test runs (CI parity) and the Windows side for
Windows-only suites.

---

## 4. Container Runtime

Docker Desktop is simplest; Podman + WSL works if you can't license Docker.

```powershell
winget install --id Docker.DockerDesktop -e
```

After install:

1. Launch Docker Desktop, accept the license.
2. Settings → *Use the WSL 2 based engine*.
3. Settings → *Resources → WSL Integration* → enable for your distro.

Smoke test:

```powershell
docker run --rm hello-world
```

---

## 5. Language Runtimes

Install only what your project uses. Where possible, prefer a **version manager**
over a single global install — tests often pin specific versions.

### Node.js (via fnm)

```powershell
winget install --id Schniz.fnm -e
fnm install 20
fnm install 22
fnm default 22
node -v ; npm -v
corepack enable    # pnpm / yarn shims
```

### Python (via pyenv-win)

```powershell
winget install --id pyenv-win.pyenv-win -e
# Restart shell, then:
pyenv install 3.12.7
pyenv install 3.11.9
pyenv global 3.12.7
python --version
python -m pip install --upgrade pip pipx
pipx ensurepath
```

### .NET SDK

```powershell
winget install --id Microsoft.DotNet.SDK.8 -e
winget install --id Microsoft.DotNet.SDK.9 -e
dotnet --list-sdks
```

### JDK (Temurin)

```powershell
winget install --id EclipseAdoptium.Temurin.21.JDK -e
java -version
```

### Go / Rust (optional)

```powershell
winget install --id GoLang.Go        -e
winget install --id Rustlang.Rustup  -e
rustup default stable
```

---

## 6. Browsers + Drivers (for E2E tests)

Install all three browser engines so Playwright/Selenium suites run end-to-end.

```powershell
winget install --id Google.Chrome             -e
winget install --id Mozilla.Firefox           -e
winget install --id Microsoft.Edge            -e   # usually pre-installed
```

### Playwright (preferred)

```powershell
npm init -y
npm install -D @playwright/test
npx playwright install --with-deps
npx playwright --version
```

### Selenium (only if your suite uses it)

Drivers are auto-managed by Selenium 4+; no manual `chromedriver.exe` needed.

---

## 7. Common Test Frameworks (per stack)

| Stack       | Install                                                 | Run             |
|-------------|---------------------------------------------------------|-----------------|
| Node/TS     | `npm i -D vitest jest @types/jest ts-node typescript`   | `npx vitest`    |
| Python      | `pipx install poetry` then `poetry add -G dev pytest`   | `poetry run pytest` |
| .NET        | `dotnet new xunit -n MyApp.Tests`                       | `dotnet test`   |
| Java        | Use Maven/Gradle; JUnit 5 via dependency                | `mvn test`      |
| Go          | Built-in                                                | `go test ./...` |
| Rust        | Built-in                                                | `cargo test`    |

---

## 8. Networking & Ports

Many test suites bind to localhost ports. Pre-clear and allow them:

```powershell
# Show what owns a port (e.g., 3000)
Get-NetTCPConnection -LocalPort 3000 -ErrorAction SilentlyContinue |
  Select-Object LocalPort, OwningProcess

# Allow a port through Windows Firewall (e.g., 5173 for Vite tests)
New-NetFirewallRule -DisplayName "Test 5173" -Direction Inbound `
  -Protocol TCP -LocalPort 5173 -Action Allow
```

If corporate proxy / TLS interception is present:

```powershell
[Environment]::SetEnvironmentVariable("HTTPS_PROXY", "http://proxy:8080", "User")
[Environment]::SetEnvironmentVariable("NODE_EXTRA_CA_CERTS", "C:\certs\corp-root.pem", "User")
```

---

## 9. Environment Hygiene

Long `PATH` is the #1 cause of "tests pass on CI, fail locally".

```powershell
# Inspect resolved PATH
$env:Path -split ';'

# Confirm which binary actually runs
Get-Command node, python, dotnet, docker
```

Recommended user env vars:

```powershell
[Environment]::SetEnvironmentVariable("CI", "false", "User")
[Environment]::SetEnvironmentVariable("PYTHONUTF8", "1", "User")
[Environment]::SetEnvironmentVariable("DOTNET_CLI_TELEMETRY_OPTOUT", "1", "User")
```

Disable Windows Defender real-time scanning on your code/tooling cache dirs to
cut test runtime dramatically (admin only, use your judgment):

```powershell
Add-MpPreference -ExclusionPath "$env:USERPROFILE\source"
Add-MpPreference -ExclusionPath "$env:USERPROFILE\.cache"
Add-MpPreference -ExclusionProcess "node.exe","python.exe","dotnet.exe"
```

---

## 10. Verification

Run the full smoke check after install. Anything that prints a version is good.

```powershell
git --version
gh --version
node -v ; npm -v
python --version
dotnet --list-sdks
java -version
docker run --rm hello-world
wsl --status
npx playwright --version
```

If all of the above succeed, the box is ready for tests.

---

## 11. Optional: One-Shot Bootstrap Script

Save as `bootstrap.ps1` and run elevated. Edit the `$packages` list to taste.

```powershell
#requires -RunAsAdministrator
$ErrorActionPreference = "Stop"

$packages = @(
  "Git.Git",
  "GitHub.cli",
  "Microsoft.PowerShell",
  "Microsoft.WindowsTerminal",
  "Microsoft.VisualStudioCode",
  "Schniz.fnm",
  "pyenv-win.pyenv-win",
  "Microsoft.DotNet.SDK.8",
  "EclipseAdoptium.Temurin.21.JDK",
  "Docker.DockerDesktop",
  "Google.Chrome",
  "Mozilla.Firefox"
)

foreach ($id in $packages) {
  Write-Host "==> $id"
  winget install --id $id -e --silent --accept-package-agreements --accept-source-agreements
}

wsl --install -d Ubuntu --no-launch
Write-Host "Reboot, then re-run the verification block in section 10."
```

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `winget` not found | Install *App Installer* from Microsoft Store |
| `wsl --install` hangs | Enable virtualization in BIOS; `bcdedit /set hypervisorlaunchtype auto` |
| Docker engine won't start | Toggle WSL2 backend off/on; restart `vmcompute` service |
| Long-path errors on `npm install` | `git config --system core.longpaths true` and enable Win32 long paths via Group Policy |
| Tests hit antivirus throttling | Add Defender exclusions (section 9) |
| `playwright install` fails behind proxy | Set `HTTPS_PROXY` and `PLAYWRIGHT_DOWNLOAD_HOST` |
