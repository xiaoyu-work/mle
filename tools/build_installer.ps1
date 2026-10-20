param(
    [Parameter(Mandatory)]
    [ValidateSet('x64', 'arm64')]
    [string]$Architecture
)

$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $compiler = Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue
    $candidates = @(
        if ($compiler) { $compiler.Source }
        Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'
        Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
    )
    $iscc = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
    if (-not $iscc) {
        throw 'Inno Setup 6 is required. Install JRSoftware.InnoSetup using winget.'
    }
    $version = [regex]::Match(
        (Get-Content 'Cargo.toml' -Raw),
        '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"'
    ).Groups[1].Value
    if (-not $version) { throw 'Cannot read application version from Cargo.toml.' }
    $exe = [IO.File]::ReadAllBytes((Join-Path $PWD 'dist\MLE\mle.exe'))
    $peOffset = [BitConverter]::ToInt32($exe, 0x3c)
    $machine = [BitConverter]::ToUInt16($exe, $peOffset + 4)
    $expected = if ($Architecture -eq 'arm64') { 0xAA64 } else { 0x8664 }
    if ($machine -ne $expected) { throw "Executable does not match $Architecture." }
    & $iscc "/DAppVersion=$version" "/DAppArch=$Architecture" 'tools\installer.iss'
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
    $output = "dist\installers\MLE-$version-windows-$Architecture-setup.exe"
    if (-not (Test-Path -LiteralPath $output -PathType Leaf)) {
        throw "Installer not generated: $output"
    }
    Write-Output "Windows installer: $output"
} finally {
    Pop-Location
}
