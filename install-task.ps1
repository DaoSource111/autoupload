# Registers a Windows task that runs upload-all.ps1 every day at the given time (10:00 by default).
# Usage:   & '<folder>\install-task.ps1'                 (or with -Time 21:30)
# Remove:  Unregister-ScheduledTask -TaskName 'AutoUpload' -Confirm:$false
param([string]$Time = '10:00')
$script = Join-Path $PSScriptRoot 'upload-all.ps1'
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$script`"" -WorkingDirectory $PSScriptRoot
$trigger = New-ScheduledTaskTrigger -Daily -At $Time
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -RunOnlyIfNetworkAvailable -ExecutionTimeLimit (New-TimeSpan -Hours 3)
Register-ScheduledTask -TaskName 'AutoUpload' -Action $action -Trigger $trigger -Settings $settings -Description 'Schedules on YouTube the videos waiting in the channels\*\queue folders' -Force | Out-Null
Write-Host "Scheduled task 'AutoUpload' registered: every day at $Time. Logs in $PSScriptRoot\logs"
