[CmdletBinding()]
param(
    [ValidateSet("cpu", "vulkan")]
    [string] $Backend = "cpu"
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$downloadRoot = Join-Path $repoRoot ".local\downloads"
$llamaRoot = Join-Path $repoRoot ".local\llama.cpp\$Backend"
$modelRoot = Join-Path $repoRoot "models"
$modelName = "Qwen3-4B-Q4_K_M.gguf"
$modelPath = Join-Path $modelRoot $modelName
$modelSize = 2497280256L
$modelSha256 = "7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5"
$modelRevision = "bc640142c66e1fdd12af0bd68f40445458f3869b"
$modelUrl = "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/$modelRevision/$modelName"
$llamaArchiveName = "llama-b11284-bin-win-$Backend-x64.zip"
$llamaArchive = Join-Path $downloadRoot $llamaArchiveName
$llamaUrl = "https://github.com/ggml-org/llama.cpp/releases/download/b11284/$llamaArchiveName"
$llamaSha256 = if ($Backend -eq "vulkan") {
    "69c70371c2c9fe3e80859cdbb48596aed3e4265627e470a90d5a9f66dd6779d5"
} else {
    "d1ae718af63497d74caf4a012e4550bbbdfab2d9faceefbd5f06b73dacbe903a"
}

function Get-Sha256([string] $Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Download-VerifiedFile(
    [string] $Url,
    [string] $Destination,
    [string] $ExpectedSha256,
    [Nullable[long]] $ExpectedSize = $null
) {
    if (Test-Path -LiteralPath $Destination) {
        $existingHash = Get-Sha256 $Destination
        if ($existingHash -eq $ExpectedSha256) {
            Write-Host "Verified existing file: $Destination"
            return
        }
        throw "Existing file has an unexpected SHA-256: $Destination ($existingHash)"
    }

    $partialPath = "$Destination.part"
    Write-Host "Downloading $Url"
    & curl.exe --fail --location --retry 3 --retry-delay 2 --continue-at - --output $partialPath $Url
    if ($LASTEXITCODE -ne 0) {
        throw "Download failed with curl exit code $LASTEXITCODE"
    }

    if ($ExpectedSize.HasValue -and (Get-Item -LiteralPath $partialPath).Length -ne $ExpectedSize.Value) {
        throw "Downloaded file has an unexpected size: $partialPath"
    }

    $actualHash = Get-Sha256 $partialPath
    if ($actualHash -ne $ExpectedSha256) {
        throw "Downloaded file has an unexpected SHA-256: $partialPath ($actualHash)"
    }

    Move-Item -LiteralPath $partialPath -Destination $Destination
}

New-Item -ItemType Directory -Path $downloadRoot, $llamaRoot, $modelRoot -Force | Out-Null

Download-VerifiedFile `
    -Url $llamaUrl `
    -Destination $llamaArchive `
    -ExpectedSha256 $llamaSha256

$llamaServer = Get-ChildItem -LiteralPath $llamaRoot -Filter "llama-server.exe" -File -Recurse -ErrorAction SilentlyContinue |
    Select-Object -First 1
if (-not $llamaServer) {
    Expand-Archive -LiteralPath $llamaArchive -DestinationPath $llamaRoot -Force
    $llamaServer = Get-ChildItem -LiteralPath $llamaRoot -Filter "llama-server.exe" -File -Recurse |
        Select-Object -First 1
}
if (-not $llamaServer) {
    throw "llama-server.exe was not found in the pinned llama.cpp archive"
}

Download-VerifiedFile `
    -Url $modelUrl `
    -Destination $modelPath `
    -ExpectedSha256 $modelSha256 `
    -ExpectedSize $modelSize

Write-Host ""
Write-Host "Local LLM runtime is ready."
Write-Host "Backend: $Backend"
Write-Host "Server:  $($llamaServer.FullName)"
Write-Host "Model:  $modelPath"
Write-Host "Start it with: .\scripts\start-local-llm.ps1 -Backend $Backend"
