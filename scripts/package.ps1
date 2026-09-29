# Builds a release and packages it into dist\:
#   glide.exe, glide.ico               (inputs for the installer)
#   Glide-<version>-portable.zip       (glide.exe + licenses + readme)
#   Glide-<version>-setup.exe          (if Inno Setup is installed)
#   SHA256SUMS.txt
# Used by the release workflow and for local builds.
param(
    [string]$Version,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

if (-not $Version) {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
}
Write-Host "Packaging Glide $Version"

if (-not $SkipBuild) {
    cargo build --release --locked -p glide-app
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}

$dist = Join-Path $root 'dist'
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory $dist | Out-Null

Copy-Item target\release\glide.exe, target\release\glide-settings.exe $dist
# build.rs draws the icon into its OUT_DIR; take the newest one.
$ico = Get-ChildItem target\release\build -Recurse -Filter glide.ico | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $ico) { throw 'glide.ico not found; did build.rs run?' }
Copy-Item $ico.FullName $dist

$portable = Join-Path $dist 'portable'
New-Item -ItemType Directory $portable | Out-Null
Copy-Item target\release\glide.exe, target\release\glide-settings.exe, LICENSE-MIT, LICENSE-APACHE, README.md $portable
# Antivirus often holds a freshly copied exe open for a moment; retry briefly.
for ($try = 1; ; $try++) {
    try {
        Compress-Archive -Path "$portable\*" -DestinationPath (Join-Path $dist "Glide-$Version-portable.zip") -Force
        break
    } catch {
        if ($try -ge 5) { throw }
        Start-Sleep -Seconds 2
    }
}
Remove-Item $portable -Recurse -Force

$iscc = @(
    (Get-Command iscc -ErrorAction SilentlyContinue).Source,
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($iscc) {
    # Compile into a temp folder and move the result: antivirus scanning the
    # half-written setup.exe in place can make Inno Setup's final resource
    # update fail ("EndUpdateResource failed (110)").
    $out = Join-Path ([IO.Path]::GetTempPath()) "glide-iscc-$PID"
    New-Item -ItemType Directory -Force $out | Out-Null
    & $iscc /Q "/DAppVersion=$Version" "/DDistDir=$dist" "/O$out" installer\glide.iss
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
    Move-Item (Join-Path $out '*.exe') $dist
    Remove-Item $out -Recurse -Force
} else {
    Write-Warning 'Inno Setup not found; skipping the installer.'
}

Remove-Item (Join-Path $dist 'glide.ico')
$sums = Get-ChildItem $dist -File | Where-Object { $_.Name -ne 'SHA256SUMS.txt' } | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
}
$sums | Set-Content (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii

# Package-manager manifests for this exact build, from the templates in
# packaging\. They point at the GitHub release for this version, so they only
# work once the release is published and the repository is public.
$repo = 'Tech-Savant20/glide'
$base = "https://github.com/$repo/releases/download/v$Version"
$hash = @{}
foreach ($file in Get-ChildItem $dist -File) {
    $hash[$file.Name] = (Get-FileHash $file.FullName -Algorithm SHA256).Hash
}
$fill = {
    param($text)
    $text.Replace('{version}', $Version).Replace('{base}', $base).
        Replace('{zip_sha256}', "$($hash["Glide-$Version-portable.zip"])".ToLower()).
        Replace('{setup_sha256}', "$($hash["Glide-$Version-setup.exe"])")
}
$manifests = Join-Path $dist 'manifests'
New-Item -ItemType Directory $manifests | Out-Null
Get-ChildItem packaging -Recurse -File | ForEach-Object {
    # The winget manifests describe the installer; skip them if there is none.
    if ($_.Extension -eq '.yaml' -and -not $hash.ContainsKey("Glide-$Version-setup.exe")) { return }
    $text = & $fill ([IO.File]::ReadAllText($_.FullName))
    $folder = Join-Path $manifests $(if ($_.Extension -eq '.yaml') { 'winget' } else { 'scoop' })
    New-Item -ItemType Directory -Force $folder | Out-Null
    [IO.File]::WriteAllText((Join-Path $folder $_.Name), $text)
}
Get-ChildItem $dist | Format-Table Name, @{ n = 'KB'; e = { [math]::Round($_.Length / 1KB) } } -AutoSize
