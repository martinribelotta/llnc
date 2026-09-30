[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$downloadRoot = Join-Path $repoRoot ".local\downloads"
$installRoot = Join-Path $repoRoot ".local\xpack"

$packages = @(
    @{
        Name = "xpack-qemu-riscv-9.2.4-1-win32-x64.zip"
        Url = "https://github.com/xpack-dev-tools/qemu-riscv-xpack/releases/download/v9.2.4-1/xpack-qemu-riscv-9.2.4-1-win32-x64.zip"
        Sha256 = "05df8a50c2109605c7ab382667aa4680d41cb262383053e5bb07d29de616eb1f"
        Size = 37760569L
        Executable = "qemu-system-riscv32.exe"
        Directory = "qemu-riscv"
    },
    @{
        Name = "xpack-riscv-none-elf-gcc-15.2.0-1-win32-x64.zip"
        Url = "https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases/download/v15.2.0-1/xpack-riscv-none-elf-gcc-15.2.0-1-win32-x64.zip"
        Sha256 = "85ef714dacd273b1dadf4af4892774520ac01915bfa6da816a56e7e41591e09e"
        Size = 465438733L
        Executable = "riscv-none-elf-as.exe"
        Directory = "riscv-none-elf-gcc"
    }
)

function Get-Sha256([string] $Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Install-XpackPackage([hashtable] $Package) {
    $archive = Join-Path $downloadRoot $Package.Name
    $destination = Join-Path $installRoot $Package.Directory
    $binary = Get-ChildItem -LiteralPath $destination -Filter $Package.Executable -File -Recurse -ErrorAction SilentlyContinue |
        Select-Object -First 1

    if ($binary) {
        Write-Host "Already installed: $($Package.Directory)"
        return
    }

    if (Test-Path -LiteralPath $archive) {
        $hash = Get-Sha256 $archive
        if ((Get-Item -LiteralPath $archive).Length -ne $Package.Size -or $hash -ne $Package.Sha256) {
            throw "Existing xPack archive failed size or SHA-256 verification: $archive"
        }
        Write-Host "Verified existing archive: $archive"
    } else {
        $partial = "$archive.part"
        Write-Host "Downloading $($Package.Url)"
        & curl.exe --fail --location --retry 3 --retry-delay 2 --continue-at - --output $partial $Package.Url
        if ($LASTEXITCODE -ne 0) {
            throw "Download failed with curl exit code $LASTEXITCODE"
        }
        if ((Get-Item -LiteralPath $partial).Length -ne $Package.Size) {
            throw "Downloaded archive has an unexpected size: $partial"
        }
        $hash = Get-Sha256 $partial
        if ($hash -ne $Package.Sha256) {
            throw "Downloaded archive has an unexpected SHA-256: $partial ($hash)"
        }
        Move-Item -LiteralPath $partial -Destination $archive
    }

    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force
    $binary = Get-ChildItem -LiteralPath $destination -Filter $Package.Executable -File -Recurse |
        Select-Object -First 1
    if (-not $binary) {
        throw "Expected executable $($Package.Executable) was not found after extraction"
    }
    Write-Host "Installed $($Package.Directory): $($binary.Directory.FullName)"
}

New-Item -ItemType Directory -Path $downloadRoot, $installRoot -Force | Out-Null
foreach ($package in $packages) {
    Install-XpackPackage $package
}

Write-Host ""
Write-Host "xPack RISC-V toolchain and QEMU are installed under .local\xpack."
Write-Host "Run builds with: .\scripts\build-qemu-example.ps1"
