# Weavatrix Loom alpha smoke - CLI + HTTP (no Studio browser).
# Usage (from weavatrix-loom root):
#   powershell -File ./scripts/alpha-smoke.ps1
# Optional: $env:WVX_HTTP_ADDR = "127.0.0.1:43917"
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$failed = 0
function Step {
  param([string]$Name, [scriptblock]$Body)
  Write-Host ""
  Write-Host "==== $Name ====" -ForegroundColor Cyan
  try {
    & $Body
    Write-Host "OK $Name" -ForegroundColor Green
  } catch {
    Write-Host "FAIL $Name : $_" -ForegroundColor Red
    $script:failed++
  }
}

Step -Name "CLI validate pilot" -Body {
  cargo run -p wvx-cli --quiet -- validate fixtures/pilot-json-pipeline.wvx.json | Out-Host
}

Step -Name "CLI run pilot" -Body {
  $out = cargo run -p wvx-cli --quiet -- run fixtures/pilot-json-pipeline.wvx.json 2>&1 | Out-String
  if ($out -notmatch 'loom') {
    throw "run output missing loom tag"
  }
  Write-Host ($out.Trim().Split("`n") | Select-Object -Last 8)
}

Step -Name "CLI registry check" -Body {
  cargo run -p wvx-cli --quiet -- registry check | Out-Null
}

Step -Name "CLI export-rust --dev --check (playground)" -Body {
  $outDir = Join-Path $env:TEMP "loom-alpha-smoke-export"
  if (Test-Path $outDir) { Remove-Item $outDir -Recurse -Force }
  cargo run -p wvx-cli --quiet -- export-rust fixtures/pilot-json-pipeline.wvx.json -o $outDir --dev --check | Out-Host
}

$base = if ($env:WVX_HTTP_ADDR) { "http://$($env:WVX_HTTP_ADDR)" } else { "http://127.0.0.1:43917" }
$serverUp = $false
try {
  $null = Invoke-RestMethod "$base/health" -TimeoutSec 2
  $serverUp = $true
} catch {
  Write-Host ""
  Write-Host "loom-server not running at $base - starting for HTTP smoke..." -ForegroundColor Yellow
}

$serverJob = $null
if (-not $serverUp) {
  $env:WVX_SESSION_TOKEN = "wvx-alpha-smoke-token"
  $serverJob = Start-Process -FilePath "cargo" -ArgumentList @("run","-p","loom-server","--quiet") `
    -WorkingDirectory $root -PassThru -WindowStyle Hidden
  $deadline = (Get-Date).AddSeconds(90)
  while ((Get-Date) -lt $deadline) {
    try {
      $null = Invoke-RestMethod "$base/health" -TimeoutSec 1
      $serverUp = $true
      break
    } catch {
      Start-Sleep -Milliseconds 500
    }
  }
  if (-not $serverUp) { throw "loom-server failed to start within 90s" }
}

Step -Name "HTTP health + bootstrap + validate + run + forge inventory" -Body {
  $boot = Invoke-RestMethod "$base/api/v1/auth/bootstrap"
  if (-not $boot.token) { throw "no bootstrap token" }
  $h = @{ "X-WVX-Token" = $boot.token; "Content-Type" = "application/json"; "Accept" = "application/json" }
  $proto = Invoke-RestMethod "$base/api/v1/protocol"
  if ($proto.product -ne "weavatrix-loom") { throw "unexpected product $($proto.product)" }
  $sum = Invoke-RestMethod "$base/api/v1/registry/summary" -Headers $h
  if (-not $sum.ok) { throw "registry summary failed" }
  $proj = Get-Content "fixtures/pilot-json-pipeline.wvx.json" -Raw | ConvertFrom-Json
  $bytes = [System.Text.Encoding]::UTF8.GetBytes((@{ project = $proj } | ConvertTo-Json -Depth 50 -Compress))
  $v = Invoke-RestMethod "$base/api/v1/project/validate" -Method POST -Headers $h -Body $bytes
  if (-not $v.ok) { throw "validate failed" }
  $runBytes = [System.Text.Encoding]::UTF8.GetBytes((@{ project = $proj; input_json = '{"hello":"world"}' } | ConvertTo-Json -Depth 50 -Compress))
  $r = Invoke-RestMethod "$base/api/v1/project/run" -Method POST -Headers $h -Body $runBytes
  if (-not $r.ok) { throw "run failed" }
  $fi = Invoke-RestMethod "$base/api/v1/forge/inventory" -Method POST -Headers $h -Body (@{ path = $root } | ConvertTo-Json)
  if (-not $fi.ok -or $fi.data.packages.Count -lt 1) { throw "forge inventory failed" }
  Write-Host "registry caps=$($sum.data.capabilities) impls=$($sum.data.implementations) packages=$($fi.data.packages.Count)"
}

if ($serverJob) {
  try { Stop-Process -Id $serverJob.Id -Force -ErrorAction SilentlyContinue } catch { }
  Get-Process -Name loom-server -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

Write-Host ""
if ($failed -gt 0) {
  Write-Host "ALPHA SMOKE FAILED: $failed steps" -ForegroundColor Red
  exit 1
}
Write-Host "ALPHA SMOKE PASSED" -ForegroundColor Green
exit 0
