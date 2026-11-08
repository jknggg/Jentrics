param([string[]]$Paths)
$ErrorActionPreference = 'Stop'
if (-not $PSBoundParameters.ContainsKey('Paths')) {
    $Paths = @(git ls-files)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate tracked source files.' }
}
$oversized = @($Paths | Where-Object { $_ -match '\.(rs|[cm]?js|jsx|tsx?|css|scss|html|ps1|py|sh|sql)$' } | ForEach-Object {
    if (@(Get-Content -LiteralPath $_).Count -gt 600) { $_ }
})
if ($oversized.Count) { throw "Code files exceed 600 lines: $($oversized -join ', ')" }
Write-Output 'Source files satisfy the 600-line limit.'
