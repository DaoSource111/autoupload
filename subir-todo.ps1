# Automatic run: schedules the queue of ALL channels and saves the log in logs\.
# Started by the scheduled task (instalar-tarea.ps1) or by hand:  & 'E:\subidor-youtube\subir-todo.ps1'
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot
New-Item -ItemType Directory -Force logs | Out-Null
$log = "logs\{0:yyyy-MM-dd_HHmm}.log" -f (Get-Date)
& .\.venv\Scripts\subidor.exe schedule --all --yes *>&1 | Tee-Object -FilePath $log
$codigo = $LASTEXITCODE
"codigo=$codigo" | Tee-Object -FilePath $log -Append
exit $codigo
