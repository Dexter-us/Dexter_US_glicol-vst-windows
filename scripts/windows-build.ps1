$ErrorActionPreference = "Stop"
Push-Location (Join-Path $PSScriptRoot "..")
try {
    $rustInfo = rustc -vV
    if ($LASTEXITCODE -ne 0 -or -not ($rustInfo -match "host: x86_64-pc-windows-msvc")) {
        throw "Use the native x64 Windows MSVC Rust toolchain."
    }
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw "VST3 regression tests failed." }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "VST3 release build failed." }
    $binaryDir = "target\release\bundle\Glicol VST.vst3\Contents\x86_64-win"
    New-Item -ItemType Directory -Force $binaryDir | Out-Null
    $binary = Join-Path $binaryDir "Glicol VST.vst3"
    # Keep this DLL filename solely to reuse the unchanged GitHub upload step.
    # This branch exports VST3, not VST2. Deliver it inside the bundle below.
    Copy-Item "target\release\glicol_vst.dll" $binary -Force
    Copy-Item LICENSE "target\release\bundle\LICENSE" -Force
    Copy-Item WINDOWS-VST3.md "target\release\bundle\WINDOWS-VST3.md" -Force
    Copy-Item THIRD-PARTY-LICENSES.txt "target\release\bundle\THIRD-PARTY-LICENSES.txt" -Force
    $hash = (Get-FileHash $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  Glicol VST.vst3/Contents/x86_64-win/Glicol VST.vst3" |
        Set-Content "target\release\bundle\SHA256SUMS.txt"
    Write-Host "Built genuine x64 VST3 bundle (not a renamed VST2 DLL):"
    Write-Host $binary
    Write-Host "VST3 module SHA256: $hash"
} finally {
    Pop-Location
}
