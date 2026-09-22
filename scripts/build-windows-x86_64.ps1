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
