# Clears leftovers from a previous dev run so the next F5 / Ctrl+Shift+B starts cleanly:
#  - a still-running target\debug\dettino.exe locks the exe (cargo can't relink it) and,
#    via the single-instance plugin, makes the new instance exit right away;
#  - with -FreePort, a Vite server still holding :1420 makes `tauri dev` fail (strictPort).
# Only the dev build is touched; an installed Dettino keeps running.
param([switch]$FreePort)

$exe = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\target\debug\dettino.exe'))
$stale = Get-Process dettino -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }
if ($stale) {
    Write-Host "Stopping stale dev instance (pid $($stale.Id -join ', '))"
    $stale | Stop-Process -Force
    $stale | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}

if ($FreePort) {
    $owners = Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess -Unique
    foreach ($id in $owners) {
        $p = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($p -and $p.ProcessName -eq 'node') {
            Write-Host "Stopping Vite dev server on :1420 (pid $id)"
            Stop-Process -Id $id -Force
        } elseif ($p) {
            Write-Warning "Port 1420 is held by $($p.ProcessName) (pid $id); tauri dev will fail until it exits."
        }
    }
}
exit 0
