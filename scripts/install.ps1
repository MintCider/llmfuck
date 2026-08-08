$ErrorActionPreference = 'Stop'

$Version = if ($env:LLMFUCK_VERSION) { $env:LLMFUCK_VERSION } else { '__LLMFUCK_VERSION__' }
$Repository = 'MintCider/llmfuck'
$InstallDir = if ($env:LLMFUCK_INSTALL_DIR) { $env:LLMFUCK_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }

if ($Version -eq ('__LLMFUCK' + '_VERSION__')) {
  throw 'This installer template must be downloaded from a tagged release.'
}
if ($Version -notmatch '^v\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
  throw "Invalid release version: $Version"
}
if (-not $IsWindows) {
  throw 'This PowerShell installer supports Windows only. Use install.sh on Linux or macOS.'
}
if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [Runtime.InteropServices.Architecture]::X64) {
  throw "Unsupported architecture: $([Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
}

$Target = 'x86_64-pc-windows-msvc'
$Archive = "llmfuck-$Version-$Target.zip"
$BaseUrl = "https://github.com/$Repository/releases/download/$Version"
$TempDir = Join-Path ([IO.Path]::GetTempPath()) "llmfuck-$([guid]::NewGuid())"
New-Item -ItemType Directory $TempDir | Out-Null

try {
  $ArchivePath = Join-Path $TempDir $Archive
  $ChecksumsPath = Join-Path $TempDir 'SHA256SUMS'
  Invoke-WebRequest "$BaseUrl/$Archive" -OutFile $ArchivePath
  Invoke-WebRequest "$BaseUrl/SHA256SUMS" -OutFile $ChecksumsPath

  $ChecksumLine = Get-Content $ChecksumsPath | Where-Object { $_ -match " $([regex]::Escape($Archive))$" } | Select-Object -First 1
  if (-not $ChecksumLine) { throw "No checksum found for $Archive" }
  $Expected = ($ChecksumLine -split '\s+')[0]
  $Actual = (Get-FileHash $ArchivePath -Algorithm SHA256).Hash
  if ($Actual -ne $Expected) { throw 'SHA-256 verification failed' }

  Expand-Archive $ArchivePath -DestinationPath $TempDir -Force
  New-Item -ItemType Directory -Force $InstallDir | Out-Null
  Copy-Item (Join-Path $TempDir "llmfuck-$Version-$Target\fuck.exe") (Join-Path $InstallDir 'fuck.exe') -Force

  $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  $PathEntries = @($UserPath -split ';' | Where-Object { $_ })
  if ($PathEntries -notcontains $InstallDir) {
    $NewUserPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
    [Environment]::SetEnvironmentVariable('Path', $NewUserPath, 'User')
  }
  if (($env:Path -split ';') -notcontains $InstallDir) {
    $env:Path = "$InstallDir;$env:Path"
  }

  Write-Host "Installed fuck $Version to $(Join-Path $InstallDir 'fuck.exe')"
  Write-Host "Run 'fuck config' to configure a provider and shell integration."
} finally {
  if (Test-Path $TempDir) {
    Remove-Item -Recurse -Force $TempDir
  }
}
