<#
.SYNOPSIS
  Bumps the app version everywhere and writes the release notes.

.DESCRIPTION
  Updates the version in: src-tauri/Cargo.toml, src-tauri/Cargo.lock,
  src-tauri/tauri.conf.json, package.json, package-lock.json and the title line
  of the four specs documents. Then writes release-notes/v<version>.md from the
  commit messages since the last v* tag (grouped by Conventional Commit type)
  and appends a line to specs/runtime.md.

  The notes are also copied to release/v<version>/RELEASE_NOTES.md the next
  time scripts\build.bat runs (it copies release-notes/v<version>.md when present).

.EXAMPLE
  .\scripts\bump-version.ps1 patch          # 0.1.0 -> 0.1.1
  .\scripts\bump-version.ps1 minor          # 0.1.0 -> 0.2.0
  .\scripts\bump-version.ps1 major          # 0.1.0 -> 1.0.0
  .\scripts\bump-version.ps1 1.4.2          # exact version
  .\scripts\bump-version.ps1 patch -DryRun  # show what would change, touch nothing
  .\scripts\bump-version.ps1 0.1.0          # the current version: only rewrite its release notes
#>
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$To,
  [switch]$DryRun,
  [switch]$NoNotes
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot   # this script lives in scripts\
Set-Location $root

function Read-Text($path) { [System.IO.File]::ReadAllText((Join-Path $root $path)) }
function Write-Text($path, $text) {
  # UTF-8 without BOM; the text keeps whatever line endings the file had.
  [System.IO.File]::WriteAllText((Join-Path $root $path), $text, (New-Object System.Text.UTF8Encoding($false)))
}

# --- current and new version -------------------------------------------------
$cargo = Read-Text "src-tauri\Cargo.toml"
if ($cargo -notmatch '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') { throw "No version found in src-tauri/Cargo.toml" }
$cur = [int[]]@($Matches[1], $Matches[2], $Matches[3])
$old = $cur -join "."

switch -Regex ($To) {
  '^major$' { $new = "$($cur[0] + 1).0.0" }
  '^minor$' { $new = "$($cur[0]).$($cur[1] + 1).0" }
  '^patch$' { $new = "$($cur[0]).$($cur[1]).$($cur[2] + 1)" }
  '^\d+\.\d+\.\d+$' { $new = $To }
  default { throw "Give major, minor, patch or an exact version like 1.2.3 (got '$To')." }
}
# The current version again: only (re)write its release notes, change nothing else.
$notesOnly = ($new -eq $old)
if ($notesOnly -and $NoNotes) { throw "The version is already $old." }
Write-Host ($(if ($notesOnly) { "Version: $old (release notes only)" } else { "Version: $old -> $new" }) + $(if ($DryRun) { " (dry run)" } else { "" }))

# --- edits ---------------------------------------------------------------------
# Each entry: file, regex, replacement, how many matches to replace (0 = all).
$edits = @(
  @{ File = "src-tauri\Cargo.toml";        Re = '(?m)^(version\s*=\s*")[^"]+(")';                          Count = 1 },
  @{ File = "src-tauri\Cargo.lock";        Re = '(name = "local-dictation"\r?\nversion = ")[^"]+(")';      Count = 1 },
  @{ File = "src-tauri\tauri.conf.json";   Re = '("version"\s*:\s*")[^"]+(")';                             Count = 1 },
  @{ File = "package.json";                Re = '("version"\s*:\s*")[^"]+(")';                             Count = 1 },
  # package-lock.json: the root entry and packages[""], the first two matches.
  @{ File = "package-lock.json";           Re = '("version"\s*:\s*")[^"]+(")';                             Count = 2 },
  @{ File = "specs\api_reference.md";      Re = '(?m)^(# .*Open Local Dictation v)\d+\.\d+\.\d+()';      Count = 1 },
  @{ File = "specs\architecture_overview.md"; Re = '(?m)^(# .*Open Local Dictation v)\d+\.\d+\.\d+()';   Count = 1 },
  @{ File = "specs\developer_guide.md";    Re = '(?m)^(# .*Open Local Dictation v)\d+\.\d+\.\d+()';      Count = 1 },
  @{ File = "specs\full_documentation.md"; Re = '(?m)^(# .*Open Local Dictation v)\d+\.\d+\.\d+()';      Count = 1 }
)

$pending = @()
foreach ($e in $(if ($notesOnly) { @() } else { $edits })) {
  $text = Read-Text $e.File
  $rx = [regex]::new($e.Re)
  if (-not $rx.IsMatch($text)) { throw "Pattern not found in $($e.File); the file layout changed, update bump-version.ps1." }
  $out = $rx.Replace($text, ('${1}' + $new + '${2}'), $e.Count)
  if ($out -eq $text) { Write-Host "  = $($e.File) (already $new)"; continue }
  Write-Host "  ~ $($e.File)"
  $pending += @{ File = $e.File; Text = $out }
}

# --- release notes ---------------------------------------------------------------
$notesFile = "release-notes\v$new.md"
$notes = $null
if (-not $NoNotes) {
  $lastTag = (git describe --tags --abbrev=0 --match "v*" 2>$null)
  $range = if ($lastTag) { "$lastTag..HEAD" } else { "HEAD" }
  $subjects = @(git log $range --no-merges --pretty=format:"%s" 2>$null | Where-Object { $_ })
  $groups = [ordered]@{
    "feat"     = "New"
    "fix"      = "Fixed"
    "perf"     = "Faster"
    "refactor" = "Changed"
    "docs"     = "Documentation"
    "chore"    = "Maintenance"
  }
  $lines = @("# Open Local Dictation v$new", "", "Released $(Get-Date -Format 'yyyy-MM-dd')."
    $(if ($lastTag) { "Changes since $lastTag." } else { "First release notes: every change so far." }), "")
  $used = @{}
  foreach ($type in $groups.Keys) {
    $items = @($subjects | Where-Object { $_ -match "^$type(\([^)]*\))?!?:\s*(.+)$" } | ForEach-Object {
      $used[$_] = $true
      [regex]::Match($_, "^$type(\([^)]*\))?!?:\s*(.+)$").Groups[2].Value
    })
    if ($items.Count -eq 0) { continue }
    $lines += "## $($groups[$type])"
    $lines += ($items | ForEach-Object { "- " + ($_.Substring(0, 1).ToUpper() + $_.Substring(1)) })
    $lines += ""
  }
  $other = @($subjects | Where-Object { -not $used.ContainsKey($_) })
  if ($other.Count) {
    $lines += "## Other"
    $lines += ($other | ForEach-Object { "- $_" })
    $lines += ""
  }
  if ($subjects.Count -eq 0) { $lines += "No changes recorded."; $lines += "" }
  $lines += "## Install"
  $lines += "- **Setup** (installs for all users into Program Files, asks for administrator rights): ``Open-Local-Dictation-$new-setup.exe``"
  $lines += "- **Portable**: unzip ``Open-Local-Dictation-$new-portable.zip`` anywhere and run ``Open Local Dictation.exe``"
  $lines += ""
  $notes = ($lines -join "`r`n")
  Write-Host "  + $notesFile ($($subjects.Count) commits)"
}

if ($DryRun) { Write-Host "Dry run: nothing written."; exit 0 }

foreach ($p in $pending) { Write-Text $p.File $p.Text }
if ($notes) {
  New-Item -ItemType Directory -Force (Join-Path $root "release-notes") | Out-Null
  Write-Text $notesFile $notes
}

# AGENTS.md: every spec-touching change is logged in specs/runtime.md.
if (-not $notesOnly) {
  $log = Read-Text "specs\runtime.md"
  $stamp = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  if (-not $log.EndsWith("`n")) { $log += "`r`n" }
  Write-Text "specs\runtime.md" ($log + "- [$stamp] Version bumped $old -> $new (bump-version.ps1).`r`n")
}

Write-Host ""
Write-Host "Done. Next: review the diff and release-notes\v$new.md, commit ('chore: release v$new'), run scripts\build.bat, then publish with:"
Write-Host "  gh release create v$new release\v$new\*.exe release\v$new\*.zip --title 'v$new' --notes-file release-notes\v$new.md"
