# measure.ps1 - Measure baseline_mem.exe memory
# Usage: .\measure.ps1

$proc = Get-Process baseline_mem -ErrorAction SilentlyContinue
if (-not $proc) {
    Write-Host "baseline_mem.exe is not running. Start it first." -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "=== baseline_mem Memory Measurement ===" -ForegroundColor Cyan
Write-Host "PID: $($proc.Id)"
Write-Host "Working Set (MB)    : $([math]::Round($proc.WorkingSet64 / 1MB, 1))" -ForegroundColor Green
Write-Host "Private Memory (MB) : $([math]::Round($proc.PrivateMemorySize64 / 1MB, 1))" -ForegroundColor Green
Write-Host "Virtual Memory (MB) : $([math]::Round($proc.VirtualMemorySize64 / 1MB, 1))" -ForegroundColor Yellow
Write-Host ""
Write-Host "Press any key to re-measure (wait 3+ seconds after launch)..." -ForegroundColor Gray
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
