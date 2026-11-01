# 빌드 후 스킬 폴더에 설치 (Windows PowerShell). Rust 설치 필요: https://rustup.rs
# 사용: powershell -ExecutionPolicy Bypass -File scripts\install.ps1 [-Dest <스킬폴더>]
param([string]$Dest = "$env:USERPROFILE\.gemini\config\skills\inspector42")
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo build --release
New-Item -ItemType Directory -Force -Path "$Dest\bin" | Out-Null
Copy-Item "target\release\inspector42.exe" "$Dest\bin\inspector42.exe" -Force
Copy-Item "skill\inspector42\SKILL.md" $Dest -Force
Write-Host "installed -> $Dest"
