[CmdletBinding()]
param(
    [ValidateSet("cpu", "vulkan")]
    [string] $Backend = "cpu",
    [ValidateRange(1, 65535)]
    [int] $Port = 8080,
    [ValidateRange(512, 40960)]
    [int] $ContextSize = 8192
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$llamaRoot = Join-Path $repoRoot ".local\llama.cpp\$Backend"
$modelPath = Join-Path $repoRoot "models\Qwen3-4B-Q4_K_M.gguf"
$llamaServer = Get-ChildItem -LiteralPath $llamaRoot -Filter "llama-server.exe" -File -Recurse -ErrorAction SilentlyContinue |
    Select-Object -First 1

if (-not $llamaServer -or -not (Test-Path -LiteralPath $modelPath)) {
    throw "Local LLM files are missing. Run .\scripts\setup-local-llm.ps1 -Backend $Backend first."
}

$gpuLayers = if ($Backend -eq "vulkan") { "99" } else { "0" }
& $llamaServer.FullName `
    --model $modelPath `
    --host 127.0.0.1 `
    --port $Port `
    --ctx-size $ContextSize `
    --n-gpu-layers $gpuLayers `
    --jinja

if ($LASTEXITCODE -ne 0) {
    throw "llama-server exited with code $LASTEXITCODE"
}
