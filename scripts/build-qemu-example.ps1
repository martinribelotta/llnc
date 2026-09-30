[CmdletBinding()]
param(
    [string] $Manifest = "examples/serial-hello/llnc.yaml",
    [switch] $GenerateOnly
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$toolchainRoot = Join-Path $repoRoot ".local\xpack"
$gnuBin = Get-ChildItem -LiteralPath $toolchainRoot -Filter "riscv-none-elf-as.exe" -File -Recurse -ErrorAction SilentlyContinue |
    Select-Object -First 1
$qemu = Get-ChildItem -LiteralPath $toolchainRoot -Filter "qemu-system-riscv32.exe" -File -Recurse -ErrorAction SilentlyContinue |
    Select-Object -First 1

if (-not $gnuBin -or -not $qemu) {
    throw "xPack toolchain is missing. Run .\scripts\setup-xpack-toolchain.ps1 first."
}

$binDirectory = $gnuBin.Directory.FullName
$qemuDirectory = $qemu.Directory.FullName
$env:PATH = "$binDirectory;$qemuDirectory;$env:PATH"

$llncArgs = @("run", "-p", "llnc", "--", "build", $Manifest)
if ($GenerateOnly) {
    $llncArgs += "--generate-only"
}

Push-Location $repoRoot
try {
    & cargo @llncArgs
    if ($LASTEXITCODE -ne 0) {
        throw "llnc build failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
