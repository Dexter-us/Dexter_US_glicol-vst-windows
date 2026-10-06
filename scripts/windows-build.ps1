$ErrorActionPreference = "Stop"
Push-Location (Join-Path $PSScriptRoot "..")
try {
    $rustInfo = rustc -vV
    if ($LASTEXITCODE -ne 0) { throw "Rust is unavailable. Install Rust and Visual Studio C++ Build Tools." }
    if (-not ($rustInfo -match "host: x86_64-pc-windows-msvc")) {
        throw "Use the 64-bit Windows MSVC Rust toolchain for a 64-bit DAW."
    }
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw "Regression tests failed; no DLL will be installed." }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "Plugin build failed." }
    $dll = Join-Path (Get-Location) "target\release\glicol_vst.dll"
    if (-not (Test-Path $dll)) { throw "The build did not produce glicol_vst.dll." }
    Write-Host "Built DLL (close your DAW before replacing the installed copy):"
    Write-Host $dll
    Get-FileHash $dll -Algorithm SHA256
} finally {
    Pop-Location
}
