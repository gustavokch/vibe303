# Windows x86_64 VST3 + CLAP Build Targets — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Destination:** copy this file to `docs/superpowers/plans/2026-09-22-windows-x86_64-build-targets.md` as the first action of Task 1 (plan mode only allowed writing it under `~/.claude/plans/`).

**Goal:** Ship `TB303.vst3` + `TB303.clap` for `x86_64-pc-windows-msvc` in two CPU variants (v3/AVX2 and v2/compat), built on a GitHub-hosted `windows-latest` runner.

**Architecture:** The plugin code is already fully platform-agnostic — zero `cfg(target_os)`, zero SIMD intrinsics, and `vendor/baseview` carries a complete, unpatched Windows backend (`src/win/`, `src/gl/win.rs`). `nih_plug_xtask` already knows the Windows VST3 bundle layout. So this is almost entirely CI + packaging work: one new workflow, one local install script, docs. A local `cargo check --target x86_64-pc-windows-msvc` gate proves the code compiles before any CI minute is spent.

**Tech Stack:** Rust stable (edition 2021), nih-plug (pinned rev `f36931f7af4646065488a9845d8f8c2f95252c23`), nih_plug_egui + vendored baseview, GitHub Actions, PowerShell (`pwsh`).

**Spec:** No separate spec document. The requirements are the four decisions recorded below plus the Context section.

## Context

Why this change: `README.md:7` lists **Platforms: macOS (Apple Silicon), Linux (x86_64 AVX2)**. Windows — the largest DAW platform — has no build at all. There is no `x86_64-pc-windows` string anywhere in the repo (verified by grep across `*.rs`/`*.toml`/`*.yml`/`*.sh`).

Why it is cheap: the blockers you would normally expect are absent.

| Expected blocker | Actual state |
|---|---|
| Platform `cfg` gates in DSP | None. `grep -rn 'cfg(target_os\|cfg(windows\|cfg(unix\|cfg(target_arch' plugin303/src/` → empty |
| Hand-written SIMD / NEON intrinsics | None. `grep -rn 'std::arch\|core::arch\|target_feature\|aarch64' plugin303/src/` → empty |
| GUI backend missing on Windows | Present: `vendor/baseview/src/win/{mod,window,keyboard,drop_target,cursor}.rs`, `vendor/baseview/src/gl/win.rs`, and `vendor/baseview/Cargo.toml:30-32` declares the `winapi` dep |
| Vendored baseview patched away from Windows | No. The patch is macOS-only (`vendor/baseview/src/macos/window.rs:252-258`, for the screenshot harness) |
| Lockfile churn | None. `Cargo.lock` already resolves `winapi`, `windows 0.44.0`, `widestring` |
| Bundle layout logic | Lives upstream in `nih_plug_xtask`; `xtask/src/main.rs` is 3 lines of passthrough |

Intended outcome: `workflow_dispatch` on the new workflow produces two zips (`TB303-windows-x86_64-v3-<sha8>.zip`, `TB303-windows-x86_64-v2-<sha8>.zip`), each containing a `TB303.vst3` bundle folder and a `TB303.clap` DLL, optionally attached to a GitHub Release.

## Global Constraints

- Target triple: **`x86_64-pc-windows-msvc`** (MSVC ABI, native toolchain — not mingw/GNU, not cargo-xwin).
- Runner: **`windows-latest`** (GitHub-hosted). The repo is public (`https://github.com/gustavokch/vibe303`), so hosted minutes are free.
- Cargo features: **`--no-default-features --features gui`**. GUI yes, `standalone` binary no.
- CPU variants: **two**, `x86-64-v3` (AVX2/FMA/BMI) and `x86-64-v2` (SSE4.2/POPCNT, compatibility).
- `RUSTFLAGS` must be set as the **plain `RUSTFLAGS` env var**, never `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS`. Reason below.
- Bundle name is `TB303` (from `bundler.toml`). Do not rename.
- Package is `plugin303`. Artifact naming follows the existing linux convention: `TB303-windows-x86_64-<variant>`.
- Do not touch `plugin303/src/`, `Cargo.toml`, `Cargo.lock`, or `vendor/` unless Task 1 proves a real compile error.

### The `RUSTFLAGS` trap (read before writing any YAML)

`.cargo/config.toml` (tracked at HEAD) contains:

```toml
[target.'cfg(target_arch = "x86_64")']
rustflags = ["-C", "target-cpu=x86-64-v3"]
```

That `cfg` is **arch-scoped, not OS-scoped**, so it matches `x86_64-pc-windows-msvc`. Cargo picks exactly one rustflags source, first match wins:

1. `CARGO_ENCODED_RUSTFLAGS`
2. `RUSTFLAGS`
3. `target.<triple>.rustflags` **unioned with** `target.<cfg>.rustflags`
4. `build.rustflags`

Setting `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS` lands in bucket 3 **alongside** the `cfg` block, so the v2 job would emit both `-C target-cpu=x86-64-v2` and `-C target-cpu=x86-64-v3` in unspecified order — the "compat" build could silently be AVX2. Plain `RUSTFLAGS` (bucket 2) replaces bucket 3 wholesale. This is why `.github/workflows/build.yml:53-54` already uses plain `RUSTFLAGS` for linux while the macOS workflows use the per-triple form (they target aarch64, which the `cfg` block does not match, so it is safe there).

### Known dirty-worktree hazard

`git status --short .cargo` currently reports ` D .cargo/config.toml` — the file is tracked at HEAD but **deleted in the working tree**. Without it `cargo xtask` fails with `no such subcommand: xtask`. CI checks out HEAD and is unaffected; local verification in Task 1 is not. Restore it before starting:

```bash
git checkout -- .cargo/config.toml
```

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `.github/workflows/build-windows.yml` | Create | Windows CI: 2-variant matrix build + package + upload + release. Mirrors `build.yml` structure so the repo's one-workflow-per-platform convention holds. |
| `scripts/build-windows-x86_64.ps1` | Create | Local Windows build + install to system VST3/CLAP dirs. PowerShell counterpart to `scripts/build-linux-x86_64-v3.sh`. |
| `README.md:7`, `README.md:135-138` | Modify | Add Windows to the platform list and the CI section. |
| `CHANGELOG.md` | Modify | One `### Added` entry. |

Nothing under `plugin303/`, `xtask/`, `vendor/`, or `Cargo.lock` is expected to change. If Task 1 forces a change there, stop and report before proceeding to Task 2.

---

## Task 1: Prove the crate compiles for `x86_64-pc-windows-msvc`

This is the gate. It runs entirely on the local macOS host — `cargo check` type-checks the full dependency graph (including `winapi`, `windows 0.44.0`, `widestring`, and `vendor/baseview/src/win/`) **without invoking a linker**, so no MSVC toolchain is needed. If this is green, the CI job in Task 2 is near-certain to be green too.

**Files:**
- Create: `docs/superpowers/plans/2026-09-22-windows-x86_64-build-targets.md` (copy of this plan)
- Modify: none expected

**Interfaces:**
- Consumes: nothing
- Produces: a verified-compiling `x86_64-pc-windows-msvc` target for package `plugin303` with feature set `gui` (no `standalone`, no `default`). Task 2 relies on that exact feature string.

- [ ] **Step 1: Copy this plan into the repo and restore the deleted cargo config**

```bash
cd /Users/gus/Git/vibe303
mkdir -p docs/superpowers/plans
cp ~/.claude/plans/write-a-plan-to-imperative-possum.md \
   docs/superpowers/plans/2026-09-22-windows-x86_64-build-targets.md
git checkout -- .cargo/config.toml
git status --short .cargo
```

Expected: `git status --short .cargo` prints nothing (deletion reverted).

- [ ] **Step 2: Confirm the Windows target is not yet installed (the failing state)**

```bash
rustup target list --installed
```

Expected: the list does **not** contain `x86_64-pc-windows-msvc`. Then confirm the check actually fails without it:

```bash
cargo check -p plugin303 --target x86_64-pc-windows-msvc --no-default-features --features gui
```

Expected: FAIL with `error[E0463]: can't find crate for 'core'` / `the 'x86_64-pc-windows-msvc' target may not be installed`.

(If the target is already installed, this step is a no-op — record that and move on.)

- [ ] **Step 3: Install the target**

```bash
rustup target add x86_64-pc-windows-msvc
```

- [ ] **Step 4: Run the library check — the real gate**

```bash
cargo check -p plugin303 --target x86_64-pc-windows-msvc --no-default-features --features gui
```

Expected: PASS (`Finished` line, warnings tolerated).

If it fails, the failure will be in one of exactly three places. Diagnose before editing anything:
- `vendor/baseview/src/win/*` — a genuine upstream-baseview Windows bug. Report and stop; do not patch blind.
- `egui-baseview` (git rev `ec70c3fe6b2f070dcacbc22924431edbe24bd1c0`) OpenGL context wiring — note that `vendor/baseview/Cargo.toml:19` declares `opengl = ["uuid", "x11/glx"]`, and `x11` is a `cfg(target_os="linux")`-only dep, so the `x11/glx` part resolves to a no-op off Linux. A failure here is a feature-unification bug, not a missing dep.
- `plugin303/src/gui/*` — unexpected, since there are no platform `cfg`s. Would most likely be a path or font-loading assumption.

- [ ] **Step 5: Run the all-targets check (catches test + bin breakage)**

```bash
cargo check -p plugin303 --target x86_64-pc-windows-msvc --no-default-features --features gui --all-targets
```

Expected: PASS. This pulls in the 9 `test = false` bins (`debug_note84`, `eval_clipper`, `gen_acid`, `measure_swing`, `render303`, `profile_voice`, `scratch_pitch`, `scratch_voice`, `vco_pitch_sweep`) plus the `plugin303/tests/*.rs` integration tests and the `assert_no_alloc` dev-dep. The `standalone` bin is correctly skipped — `plugin303/Cargo.toml:31-33` gives it `required-features = ["standalone"]`, which this feature set does not satisfy.

- [ ] **Step 6: Check both CPU baselines resolve**

```bash
RUSTFLAGS="-C target-cpu=x86-64-v3" cargo check -p plugin303 \
  --target x86_64-pc-windows-msvc --no-default-features --features gui
RUSTFLAGS="-C target-cpu=x86-64-v2" cargo check -p plugin303 \
  --target x86_64-pc-windows-msvc --no-default-features --features gui
```

Expected: both PASS. This confirms rustc accepts both `target-cpu` names for this triple.

- [ ] **Step 7: Commit**

```bash
git add docs/superpowers/plans/2026-09-22-windows-x86_64-build-targets.md .cargo/config.toml
git commit -m "docs: add Windows x86_64 VST3+CLAP build plan"
```

---

## Task 2: Add the Windows CI workflow

**Files:**
- Create: `.github/workflows/build-windows.yml`
- Reference (do not modify): `.github/workflows/build.yml` — the structural model for inputs, env, packaging and the release job

**Interfaces:**
- Consumes: the feature set proven in Task 1 (`--no-default-features --features gui`), the `TB303` bundle name from `bundler.toml:3`, package name `plugin303`
- Produces: two uploaded artifacts named `TB303-windows-x86_64-v3` and `TB303-windows-x86_64-v2`, each a single `.zip`. The `release` job globs `artifacts/**/*.zip`.

### Design notes

- **Matrix, not copy-paste.** `build.yml` and the three macOS workflows hand-copy each job; that is how `build-macos.yml:107-130` ended up with steps mis-indented *inside* a `run:` block (they never execute). Two CPU variants differing by one string is exactly what `strategy.matrix` is for.
- **`shell: pwsh` for every scripted step.** `windows-latest` defaults to `pwsh` anyway, but stating it is explicit. Do not use the bash `zip`/`test -e` idioms from `build.yml` — Git Bash on the runner has no `zip`.
- **Verify the DLL, not just the folder.** On Windows the VST3 "bundle" is a directory and the payload keeps the `.vst3` extension: `TB303.vst3/Contents/x86_64-win/TB303.vst3`. Asserting only that `TB303.vst3` exists would pass on an empty directory. The `.clap` is a bare DLL with the extension swapped — a plain file.
- **`Compress-Archive -Path $stage`** (directory path, no trailing `\*`) includes the `$stage` folder itself at the archive root, matching what `cd $RUNNER_TEMP && zip -r "$OUT.zip" "$OUT"` produces in `build.yml:73`.
- **`fail-fast: false`** so a v3 failure does not cancel the v2 build.
- **Caching.** No existing workflow caches; a cold `lto = "fat"`, `codegen-units = 1` release build of egui + nih-plug on a hosted runner is slow. `Swatinem/rust-cache@v2` keyed per variant is added here because hosted runners, unlike the self-hosted box, start from an empty `target/` every run.
- **Release job runner.** Set to `[self-hosted, linux, x64]` to match `build.yml:85`. If the self-hosted box is not reliably online, change that one line to `ubuntu-latest` — `actions/download-artifact@v4` works across runner types either way.

- [ ] **Step 1: Write the workflow file**

Create `.github/workflows/build-windows.yml` with exactly this content:

```yaml
name: build-plugins-windows

# Manual trigger only. Builds Windows x86_64 VST3 + CLAP bundles on a
# GitHub-hosted windows-latest runner (native MSVC toolchain) in two CPU
# variants, uploads them as zips, and (optionally) attaches the zips to a
# GitHub Release.
#  - v3      : x86-64-v3 (AVX2+FMA+BMI; Haswell 2013+ / Excavator 2015+).
#  - v2      : x86-64-v2 (SSE4.2+POPCNT; Nehalem 2008+). Compatibility build.
#  - release : downloads both zips and publishes a GitHub Release.
#
# Features: --no-default-features --features gui
#   GUI is included; the `standalone` binary is not (plugin hosts do not need
#   it, and it would drag in cpal/ASIO link surface).
on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag (blank = build-<sha8>, no release published if 'release' input is false)"
        required: false
        default: ""
      release:
        description: "Publish a GitHub Release with the zips"
        type: boolean
        default: true
      prerelease:
        description: "Mark the release as a pre-release"
        type: boolean
        default: true
      draft:
        description: "Create the release as a draft"
        type: boolean
        default: false

permissions:
  contents: write   # required for the release job to create tags/releases

env:
  PKG: plugin303
  BUNDLE_NAME: TB303
  TARGET: x86_64-pc-windows-msvc
  CARGO_TERM_COLOR: always

jobs:
  windows-x86_64:
    runs-on: windows-latest
    strategy:
      fail-fast: false
      matrix:
        include:
          - variant: v3
            target_cpu: x86-64-v3
          - variant: v2
            target_cpu: x86-64-v2
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust (MSVC host + target)
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc

      - name: Cache cargo registry + target dir
        uses: Swatinem/rust-cache@v2
        with:
          key: windows-${{ matrix.variant }}

      - name: Build + bundle (${{ matrix.target_cpu }})
        shell: pwsh
        env:
          # MUST be plain RUSTFLAGS, not CARGO_TARGET_<TRIPLE>_RUSTFLAGS.
          # .cargo/config.toml pins -C target-cpu=x86-64-v3 under
          # [target.'cfg(target_arch = "x86_64")'], which matches this triple.
          # Plain RUSTFLAGS replaces that bucket outright; the per-triple env
          # var is merged *with* it, which would silently make the v2 build
          # AVX2. See Cargo's rustflags precedence rules.
          RUSTFLAGS: -C target-cpu=${{ matrix.target_cpu }}
        run: |
          cargo xtask bundle $env:PKG --release --target $env:TARGET --no-default-features --features gui

      - name: Locate bundles
        id: find
        shell: pwsh
        run: |
          $dir = "target/$env:TARGET/bundled"
          if (-not (Test-Path $dir)) { $dir = "target/bundled" }
          # On Windows the VST3 bundle is a directory and the payload DLL keeps
          # the .vst3 extension, so assert the DLL itself - an empty directory
          # would otherwise pass.
          $vst3Dll = "$dir/$env:BUNDLE_NAME.vst3/Contents/x86_64-win/$env:BUNDLE_NAME.vst3"
          if (-not (Test-Path -PathType Leaf $vst3Dll)) {
            Get-ChildItem -Recurse $dir | Select-Object FullName
            throw "missing VST3 payload: $vst3Dll"
          }
          if (-not (Test-Path -PathType Leaf "$dir/$env:BUNDLE_NAME.clap")) {
            Get-ChildItem -Recurse $dir | Select-Object FullName
            throw "missing CLAP: $dir/$env:BUNDLE_NAME.clap"
          }
          "dir=$dir" | Out-File -FilePath $env:GITHUB_OUTPUT -Append

      - name: Package zip
        shell: pwsh
        run: |
          $sha8  = "${{ github.sha }}".Substring(0, 8)
          $out   = "$env:BUNDLE_NAME-windows-x86_64-${{ matrix.variant }}-$sha8"
          $stage = Join-Path $env:RUNNER_TEMP $out
          New-Item -ItemType Directory -Force -Path $stage | Out-Null
          Copy-Item -Recurse "${{ steps.find.outputs.dir }}/$env:BUNDLE_NAME.vst3" $stage
          Copy-Item "${{ steps.find.outputs.dir }}/$env:BUNDLE_NAME.clap" $stage
          $zip = Join-Path $env:RUNNER_TEMP "$out.zip"
          # -Path <dir> (no trailing \*) keeps the top-level folder inside the
          # archive, matching the `zip -r "$OUT.zip" "$OUT"` layout used by the
          # linux job in build.yml.
          Compress-Archive -Path $stage -DestinationPath $zip -Force
          "ZIP=$zip" | Out-File -FilePath $env:GITHUB_ENV -Append

      - uses: actions/upload-artifact@v4
        with:
          name: ${{ env.BUNDLE_NAME }}-windows-x86_64-${{ matrix.variant }}
          path: ${{ env.ZIP }}
          if-no-files-found: error

  release:
    if: ${{ inputs.release }}
    needs: [windows-x86_64]
    runs-on: [self-hosted, linux, x64]
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Compute release tag
        id: meta
        env:
          IN_TAG: ${{ inputs.tag }}
        run: |
          TAG="$IN_TAG"
          [ -n "$TAG" ] || TAG="build-${GITHUB_SHA::8}"
          echo "tag=$TAG" >> "$GITHUB_OUTPUT"

      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ steps.meta.outputs.tag }}
          name: ${{ steps.meta.outputs.tag }}
          target_commitish: ${{ github.sha }}
          prerelease: ${{ inputs.prerelease }}
          draft: ${{ inputs.draft }}
          files: artifacts/**/*.zip
          fail_on_unmatched_files: true
```

- [ ] **Step 2: Lint the YAML locally before pushing**

The repo has a live example of why this matters: `build-macos.yml:107-130` has three steps swallowed into a `run:` block and they silently never run.

```bash
cd /Users/gus/Git/vibe303
python3 -c "import yaml,sys; d=yaml.safe_load(open('.github/workflows/build-windows.yml')); \
  j=d['jobs']['windows-x86_64']; \
  print('job steps:', len(j['steps'])); \
  print('variants:', [m['variant'] for m in j['strategy']['matrix']['include']]); \
  print('release needs:', d['jobs']['release']['needs'])"
```

Expected output:
```
job steps: 7
variants: ['v3', 'v2']
release needs: ['windows-x86_64']
```

(7 steps, not 6: checkout, toolchain install, cache, build, locate, package, upload. The cache step was added after this lint expectation was written.)

If `job steps` is not 7, a step got absorbed into a previous `run:` block — fix the indentation.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/build-windows.yml
git commit -m "ci: build Windows x86_64 VST3+CLAP in v2 and v3 CPU variants"
```

- [ ] **Step 4: Dispatch the workflow without publishing a release**

```bash
git push -u origin HEAD
gh workflow run build-windows.yml -f release=false
sleep 20 && gh run list --workflow=build-windows.yml --limit 1
```

Then watch it:

```bash
gh run watch "$(gh run list --workflow=build-windows.yml --limit 1 --json databaseId -q '.[0].databaseId')"
```

Expected: both matrix legs succeed; the `release` job is skipped (`release=false`).

- [ ] **Step 5: Download and inspect both artifacts**

```bash
RUN=$(gh run list --workflow=build-windows.yml --limit 1 --json databaseId -q '.[0].databaseId')
rm -rf /tmp/win-artifacts && mkdir -p /tmp/win-artifacts
gh run download "$RUN" -D /tmp/win-artifacts
find /tmp/win-artifacts -name '*.zip'
for z in /tmp/win-artifacts/*/*.zip; do echo "== $z"; unzip -l "$z"; done
```

Expected, for each of the two zips, entries including:
- `TB303-windows-x86_64-v3-<sha8>/TB303.clap`
- `TB303-windows-x86_64-v3-<sha8>/TB303.vst3/Contents/x86_64-win/TB303.vst3`

(and the same with `v2`). Both `.vst3` payload and `.clap` should be non-trivial in size (MBs, not KBs) — a KB-sized file means the wrong artifact got copied.

- [ ] **Step 6: Confirm the two variants are genuinely different builds**

This is the check that catches the `RUSTFLAGS` trap. If the `cfg(target_arch)` block leaked into the v2 job, both binaries would be byte-identical.

```bash
cd /tmp/win-artifacts
for z in */*.zip; do unzip -oq "$z" -d ext; done
find ext -name 'TB303.clap' -exec shasum -a 256 {} \;
```

Expected: **two different SHA-256 digests**. Identical digests mean the v2 build was compiled as v3 — re-read the "RUSTFLAGS trap" section and fix the env var before continuing.

- [ ] **Step 7: Commit nothing; record the run URL in the PR/commit description**

No code change in this step. Note the successful run id so the reviewer can see the artifacts.

---

## Task 3: Local Windows build script + documentation

**Files:**
- Create: `scripts/build-windows-x86_64.ps1`
- Modify: `README.md:7` (platform list), `README.md:135-138` (CI section), and the `### Building VST3 and CLAP Bundles` block at `README.md:110`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: the same target triple, feature set and bundle name as Task 2
- Produces: `scripts/build-windows-x86_64.ps1`, accepting `-Variant v3|v2` (default `v3`) and honouring `$env:VST3_DIR` / `$env:CLAP_DIR` overrides — the PowerShell analogue of `scripts/build-linux-x86_64-v3.sh:20-21`

- [ ] **Step 1: Write the PowerShell build script**

Create `scripts/build-windows-x86_64.ps1`:

```powershell
#!/usr/bin/env pwsh
# Build and install the TB303 VST3 + CLAP bundles for x86_64-pc-windows-msvc.
#
# PowerShell counterpart to scripts/build-linux-x86_64-v3.sh. Run from a
# Windows host with the MSVC build tools and a stable Rust toolchain.
#
#   ./scripts/build-windows-x86_64.ps1              # x86-64-v3 (AVX2), default
#   ./scripts/build-windows-x86_64.ps1 -Variant v2  # x86-64-v2 (compat)
#   $env:VST3_DIR = 'D:\VST3'; ./scripts/build-windows-x86_64.ps1
[CmdletBinding()]
param(
    [ValidateSet('v3', 'v2')]
    [string]$Variant = 'v3'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Target      = 'x86_64-pc-windows-msvc'
$Pkg         = 'plugin303'
$BundleName  = 'TB303'
$TargetCpu   = "x86-64-$Variant"

$CommonFiles = if ($env:CommonProgramFiles) { $env:CommonProgramFiles } else { 'C:\Program Files\Common Files' }
$Vst3Dir     = if ($env:VST3_DIR) { $env:VST3_DIR } else { Join-Path $CommonFiles 'VST3' }
$ClapDir     = if ($env:CLAP_DIR) { $env:CLAP_DIR } else { Join-Path $CommonFiles 'CLAP' }

$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot
try {
    # Plain RUSTFLAGS, not CARGO_TARGET_<TRIPLE>_RUSTFLAGS: .cargo/config.toml
    # pins x86-64-v3 under [target.'cfg(target_arch = "x86_64")'], which also
    # matches this triple. Plain RUSTFLAGS replaces that bucket; the per-triple
    # form is merged with it and would defeat -Variant v2.
    $env:RUSTFLAGS = "-C target-cpu=$TargetCpu"

    rustup target add $Target 2>$null | Out-Null

    Write-Host "Building $Pkg for $Target ($TargetCpu)..."
    cargo xtask bundle $Pkg --release --target $Target --no-default-features --features gui
    if ($LASTEXITCODE -ne 0) { throw "cargo xtask bundle failed ($LASTEXITCODE)" }

    $dir = "target/$Target/bundled"
    if (-not (Test-Path $dir)) { $dir = 'target/bundled' }

    $vst3Src = Join-Path $dir "$BundleName.vst3"
    $clapSrc = Join-Path $dir "$BundleName.clap"
    $vst3Dll = Join-Path $vst3Src "Contents/x86_64-win/$BundleName.vst3"

    if (-not (Test-Path -PathType Leaf $vst3Dll)) { throw "missing VST3 payload: $vst3Dll" }
    if (-not (Test-Path -PathType Leaf $clapSrc)) { throw "missing CLAP: $clapSrc" }

    New-Item -ItemType Directory -Force -Path $Vst3Dir, $ClapDir | Out-Null

    $vst3Dst = Join-Path $Vst3Dir "$BundleName.vst3"
    if (Test-Path $vst3Dst) { Remove-Item -Recurse -Force $vst3Dst }
    Copy-Item -Recurse $vst3Src $vst3Dst

    $clapDst = Join-Path $ClapDir "$BundleName.clap"
    if (Test-Path $clapDst) { Remove-Item -Force $clapDst }
    Copy-Item $clapSrc $clapDst

    Write-Host "Installed:"
    Write-Host "  $vst3Dst"
    Write-Host "  $clapDst"
}
finally {
    Pop-Location
}
```

- [ ] **Step 2: Syntax-check the script without running it**

The macOS host cannot execute the install steps, but it can parse the file if `pwsh` is available:

```bash
cd /Users/gus/Git/vibe303
if command -v pwsh >/dev/null; then
  pwsh -NoProfile -Command '
    $t = $null; $e = $null
    [System.Management.Automation.Language.Parser]::ParseFile(
      "scripts/build-windows-x86_64.ps1", [ref]$t, [ref]$e) | Out-Null
    if ($e.Count) { $e | ForEach-Object { $_.Message }; exit 1 }
    "parse OK"'
else
  echo "pwsh not installed; skipping parse check (CI in Task 2 is the real gate)"
fi
```

Expected: `parse OK`, or the skip message.

- [ ] **Step 3: Update `README.md`**

Change the platform line (currently `README.md:7`):

```markdown
- **Platforms:** macOS (Apple Silicon), Linux (x86_64 AVX2), Windows (x86_64)
```

In the `### Building VST3 and CLAP Bundles` section (after the existing `cargo xtask bundle plugin303 --release` block at `README.md:111-118`), append:

````markdown
On Windows, build and install both bundles with:

```powershell
# x86-64-v3 (AVX2) by default; use -Variant v2 for pre-Haswell CPUs
./scripts/build-windows-x86_64.ps1
```

The Windows VST3 bundle is a directory, and its payload DLL keeps the `.vst3`
extension:

```
TB303.vst3/Contents/x86_64-win/TB303.vst3   # the DLL
TB303.clap                                  # a DLL with the extension swapped
```
````

In the `## CI / Cross-Compilation` list (currently `README.md:136-137`), add a third bullet:

```markdown
- **Windows:** Native `x86_64` MSVC builds on a GitHub-hosted runner, in two CPU
  variants — `x86-64-v3` (AVX2) and `x86-64-v2` (compatibility). Built with
  `--no-default-features --features gui`; the standalone binary is not shipped
  for Windows.
```

- [ ] **Step 4: Update `CHANGELOG.md`**

Add under the unreleased/current section, matching the file's existing heading style:

```markdown
### Added
- Windows `x86_64` VST3 + CLAP build targets. New `build-plugins-windows`
  GitHub Actions workflow builds on a hosted `windows-latest` runner in two CPU
  variants (`x86-64-v3` AVX2, `x86-64-v2` compatibility), and
  `scripts/build-windows-x86_64.ps1` builds and installs the same bundles
  locally. Windows bundles ship with the GUI but without the standalone binary.
```

- [ ] **Step 5: Commit**

```bash
git add scripts/build-windows-x86_64.ps1 README.md CHANGELOG.md
git commit -m "feat: add Windows x86_64 local build script and docs"
```

---

## Verification

End-to-end, after all three tasks:

1. **Code compiles for Windows (local, no CI needed):**
   ```bash
   cargo check -p plugin303 --target x86_64-pc-windows-msvc \
     --no-default-features --features gui --all-targets
   ```
   Expect `Finished`.

2. **Nothing regressed on the existing platforms:**
   ```bash
   cargo nextest run -p plugin303 --no-default-features
   cargo test -p plugin303 --test gui_build --test piano_roll_mapping
   cargo xtask bundle plugin303 --release   # native macOS bundle still builds
   ```
   Expect all green. Task 1-3 touch no Rust source, so any failure here is pre-existing — check `git stash`/`git status` before blaming this work.

3. **CI produces both variants:** run Step 4-6 of Task 2. The decisive assertion is Task 2 Step 6 — the two `TB303.clap` SHA-256 digests must differ.

4. **Manual host smoke test (requires a Windows machine — hand off if unavailable):**
   - Unzip `TB303-windows-x86_64-v2-<sha8>.zip`.
   - Copy `TB303.vst3` to `C:\Program Files\Common Files\VST3\` and `TB303.clap` to `C:\Program Files\Common Files\CLAP\`.
   - Rescan plugins in a host (Reaper scans both formats out of the box).
   - Confirm the plugin loads, the editor window opens at 1366x960 (`plugin303/tests/gui_build.rs:23-24`), knobs respond to the mouse, and a held MIDI note produces audio.
   - Repeat with the v3 zip on an AVX2-capable machine.

## Out of scope

Explicitly not part of this plan; open separate work if wanted:

- `aarch64-pc-windows-msvc` (Windows on ARM). Note the known `nih_plug_xtask` risk that non-x86_64 Windows payloads may still land in `Contents/x86_64-win/`.
- Authenticode code signing. The macOS script does ad-hoc `codesign` (`scripts/build-macos-arm64.sh:102-106`); there is no Windows equivalent and unsigned plugins load fine in every major host.
- A Windows `standalone.exe` (would need cpal/WASAPI validation).
- Cleaning up the three duplicated macOS workflows, including the broken step indentation in `build-macos.yml:107-130`. Real, but a different change.
- Adding `push`/`pull_request` triggers. All workflows remain `workflow_dispatch`-only.
