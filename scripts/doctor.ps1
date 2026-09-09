$checks = @()
$checks += [PSCustomObject]@{Name='WSL'; OK=[bool](Get-Command wsl.exe -ErrorAction SilentlyContinue)}
$checks += [PSCustomObject]@{Name='Unibox data directory'; OK=(Test-Path "$env:LOCALAPPDATA\Unibox")}
$checks | Format-Table -AutoSize
