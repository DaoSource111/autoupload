# Registers a Windows task that runs subir-todo.ps1 every day at the given time (10:00 by default).
# Usage:   & 'E:\subidor-youtube\instalar-tarea.ps1'            (or with -Time 21:30; -Hora also works)
# Remove:  Unregister-ScheduledTask -TaskName 'Subidor YouTube' -Confirm:$false
param([Alias('Hora')][string]$Time = '10:00')
$script = Join-Path $PSScriptRoot 'subir-todo.ps1'
$accion = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$script`"" -WorkingDirectory $PSScriptRoot
$disparo = New-ScheduledTaskTrigger -Daily -At $Time
$ajustes = New-ScheduledTaskSettingsSet -StartWhenAvailable -RunOnlyIfNetworkAvailable -ExecutionTimeLimit (New-TimeSpan -Hours 3)
Register-ScheduledTask -TaskName 'Subidor YouTube' -Action $accion -Trigger $disparo -Settings $ajustes -Description 'Schedules on YouTube the videos waiting in the channels queue folders (\*\cola or \*\queue)' -Force | Out-Null
Write-Host "Scheduled task 'Subidor YouTube' registered: every day at $Time. Logs in $PSScriptRoot\logs"
