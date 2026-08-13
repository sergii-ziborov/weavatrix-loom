# Start loom-server with a stable session token for local Studio alpha.
# Usage (from weavatrix-loom root):
#   pwsh ./scripts/dev-server.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $env:WVX_SESSION_TOKEN) {
  $env:WVX_SESSION_TOKEN = "wvx-dev-local"
}
if (-not $env:WVX_HTTP_ADDR) {
  $env:WVX_HTTP_ADDR = "127.0.0.1:43917"
}
if (-not $env:RUST_LOG) {
  $env:RUST_LOG = "info"
}

Write-Host "loom-server → http://$($env:WVX_HTTP_ADDR)"
Write-Host "token: $env:WVX_SESSION_TOKEN  (Studio bootstraps automatically via /api/v1/auth/bootstrap)"
Write-Host "Studio: cd ../loom-studio ; npm run dev"
Write-Host ""

cargo run -p loom-server
