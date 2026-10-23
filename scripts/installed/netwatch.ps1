# Sample outbound TCP connections of runout.exe and its WebView2 child processes; map IPs to names via the DNS cache.
# -RootPid picks one instance when another runout.exe (e.g. the installed app) is also running.
param([int]$Seconds = 60, [string]$Out = "net.csv", [int]$RootPid = 0)
$root = if ($RootPid) { $RootPid } else { (Get-Process runout -ErrorAction Stop | Select-Object -First 1).Id }
$seen = @{}
for ($i = 0; $i -lt $Seconds; $i++) {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name
  $pids = @($root); $grew = $true
  while ($grew) { $grew = $false; foreach ($p in $all) { if ($pids -contains $p.ParentProcessId -and -not ($pids -contains $p.ProcessId)) { $pids += $p.ProcessId; $grew = $true } } }
  foreach ($c in Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $pids -contains $_.OwningProcess -and $_.RemoteAddress -notmatch '^(127\.|::1|0\.0\.0\.0|::)$' -and $_.RemotePort -ne 0 }) {
    $proc = ($all | Where-Object ProcessId -eq $c.OwningProcess).Name
    $k = "$proc|$($c.RemoteAddress):$($c.RemotePort)|$($c.LocalPort)"
    if (-not $seen.ContainsKey($k)) { $seen[$k] = $i }
  }
  Start-Sleep -Milliseconds 1000
}
$dns = @{}; Get-DnsClientCache -ErrorAction SilentlyContinue | Where-Object { $_.Data } | ForEach-Object { if (-not $dns.ContainsKey($_.Data)) { $dns[$_.Data] = $_.Entry } }
$rows = foreach ($k in $seen.Keys) { $proc, $ep, $lp = $k -split '\|'; $ip = $ep -replace ':\d+$', ''; [pscustomobject]@{ at_s = $seen[$k]; process = $proc; remote = $ep; lport = $lp; host = $dns[$ip] } }
$rows | Sort-Object at_s | Export-Csv -NoTypeInformation $Out
$rows | Sort-Object at_s | Format-Table -AutoSize | Out-String -Width 200
