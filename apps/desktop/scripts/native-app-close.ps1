$target = Join-Path $PSScriptRoot "..\..\..\tests\native\$($MyInvocation.MyCommand.Name)"
& $target @args
exit $LASTEXITCODE
