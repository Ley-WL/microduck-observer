$ErrorActionPreference = 'Stop'
$taskHostName = Read-Host 'New board IP address'
$taskScript = Join-Path $PSScriptRoot 'deploy-new-board.py'
python $taskScript --host $taskHostName
if ($LASTEXITCODE -ne 0) { throw 'Deployment failed; inspect output before retrying' }
Read-Host 'Press Enter to close'
