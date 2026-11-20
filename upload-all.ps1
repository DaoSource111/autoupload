# Automatic run: schedules the queue of ALL channels and saves the log in logs\.
# Started by the scheduled task (install-task.ps1) or by hand:  & '<folder>\upload-all.ps1'
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot
New-Item -ItemType Directory -Force logs | Out-Null
$log = "logs\{0:yyyy-MM-dd_HHmm}.log" -f (Get-Date)
& .\.venv\Scripts\autoupload.exe schedule --all --yes *>&1 | Tee-Object -FilePath $log
$code = $LASTEXITCODE
"exit_code=$code" | Tee-Object -FilePath $log -Append
exit $code
