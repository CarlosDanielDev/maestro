# Script de notificacao para Claude Code (Windows)
# Recebe JSON via stdin com informacoes da notificacao
#
# Configuracao:
#   Execute: /setup-notifications
#   Config por projeto: .claude/notifications.conf
#
# Para desativar notificacao desktop (apenas Slack):
#   $env:CLAUDE_NOTIFY_DESKTOP = "false"

# Bot Token do time (configurado no repositorio)
$SlackBotToken = $env:SLACK_BOT_TOKEN  # Set via environment variable

# Detectar diretorio do projeto (onde o script esta)
$ScriptDir = $PSScriptRoot
$ProjectMaestroDir = Split-Path -Parent $ScriptDir
$ProjectRoot = Split-Path -Parent $ProjectMaestroDir

# Arquivos de configuracao (prioridade: .maestro/ > .claude/ legacy > global)
$ProjectConfigFile = Join-Path $ProjectMaestroDir "notifications.conf"
$LegacyProjectConfigFile = Join-Path $ProjectRoot ".claude/notifications.conf"
$LegacyConfigFile = Join-Path $env:USERPROFILE ".claude/notifications.conf"
$LegacySlackFile = Join-Path $env:USERPROFILE ".claude/slack_user_id"

# Config files are parsed line by line into a hashtable. A file line never
# names a variable (#1309: Set-Variable let a cloned repo's conf overwrite any
# script variable). Only the five known keys are accepted, and only with safe
# value shapes ([A-Za-z0-9_.@-]*). Everything else is ignored.
#
# SLACK_USER_ID is the Slack destination, so it is user-global only: it is
# read from ~/.claude/notifications.conf or ~/.claude/slack_user_id, never
# from a project conf (#1102, #1309: a cloned repo could redirect notifications).
$ConfigLineRegex = '^(NOTIFY_DESKTOP|NOTIFY_SLACK|NOTIFY_PERMISSION_PROMPT|NOTIFY_IDLE_PROMPT|SLACK_USER_ID)=([A-Za-z0-9_.@-]*)$'
$SettingKeys = @("NOTIFY_DESKTOP", "NOTIFY_SLACK", "NOTIFY_PERMISSION_PROMPT", "NOTIFY_IDLE_PROMPT")

# $Keys picks which keys the file may set: $SettingKeys or "SLACK_USER_ID".
function Read-NotifyConfigFile($File, [string[]]$Keys, $Config) {
    foreach ($line in Get-Content -LiteralPath $File) {
        if ($line -cmatch $ConfigLineRegex -and $Keys -ccontains $matches[1]) {
            $Config[$matches[1]] = $matches[2]
        }
    }
}

function Get-NotifyConfig {
    # Valores padrao
    $config = @{
        NOTIFY_DESKTOP = "true"
        NOTIFY_SLACK = "true"
        NOTIFY_PERMISSION_PROMPT = "true"
        NOTIFY_IDLE_PROMPT = "true"
        SLACK_USER_ID = ""
    }

    # NOTIFY_* prioridade: .maestro/notifications.conf > .claude/notifications.conf (legacy) > global
    foreach ($file in @($ProjectConfigFile, $LegacyProjectConfigFile, $LegacyConfigFile)) {
        if (Test-Path -LiteralPath $file) {
            Read-NotifyConfigFile $file $SettingKeys $config
            break
        }
    }

    # Slack destination: user-global files only (#1102, #1309).
    if (Test-Path -LiteralPath $LegacyConfigFile) {
        Read-NotifyConfigFile $LegacyConfigFile "SLACK_USER_ID" $config
    }
    if (-not $config.SLACK_USER_ID -and (Test-Path -LiteralPath $LegacySlackFile)) {
        $config.SLACK_USER_ID = (Get-Content -LiteralPath $LegacySlackFile -Raw).Trim()
    }

    return $config
}

# Dot-sourced (by tests/test-notify-config.ps1): stop after the definitions.
if ($MyInvocation.InvocationName -eq '.') {
    return
}

$config = Get-NotifyConfig
$NOTIFY_DESKTOP = $config.NOTIFY_DESKTOP
$NOTIFY_SLACK = $config.NOTIFY_SLACK
$NOTIFY_PERMISSION_PROMPT = $config.NOTIFY_PERMISSION_PROMPT
$NOTIFY_IDLE_PROMPT = $config.NOTIFY_IDLE_PROMPT
$SLACK_USER_ID = $config.SLACK_USER_ID

$input = $Input | Out-String
$notificationType = if ($input -match '"notification_type":"([^"]*)"') { $matches[1] } else { "" }

switch ($notificationType) {
    "permission_prompt" {
        $title = "Claude Code"
        $message = "Permissao necessaria para executar"
        $emoji = ":warning:"
    }
    "idle_prompt" {
        $title = "Claude Code"
        $message = "Aguardando sua resposta"
        $emoji = ":hourglass:"
    }
    default {
        $title = "Claude Code"
        $message = "Precisa da sua atencao"
        $emoji = ":bell:"
    }
}

# Verificar se deve notificar baseado no tipo
$shouldNotify = switch ($notificationType) {
    "permission_prompt" { $NOTIFY_PERMISSION_PROMPT -eq "true" }
    "idle_prompt" { $NOTIFY_IDLE_PROMPT -eq "true" }
    default { $true }
}

if (-not $shouldNotify) {
    exit 0
}

# ===== SLACK NOTIFICATION =====
if ($NOTIFY_SLACK -eq "true" -and $SLACK_USER_ID -and $SlackBotToken) {
    $projectName = Split-Path -Leaf (Get-Location)

    $slackPayload = @{
        channel = $SLACK_USER_ID
        blocks = @(
            @{
                type = "section"
                text = @{
                    type = "mrkdwn"
                    text = "$emoji *$title*`n$message"
                }
            }
            @{
                type = "context"
                elements = @(
                    @{
                        type = "mrkdwn"
                        text = ":file_folder: Projeto: ``$projectName``"
                    }
                )
            }
        )
    } | ConvertTo-Json -Depth 10

    # Send async to not block
    Start-Job -ScriptBlock {
        param($token, $payload)
        $headers = @{
            "Authorization" = "Bearer $token"
            "Content-Type" = "application/json; charset=utf-8"
        }
        Invoke-RestMethod -Uri "https://slack.com/api/chat.postMessage" -Method Post -Body $payload -Headers $headers
    } -ArgumentList $SlackBotToken, $slackPayload | Out-Null
}

# Skip desktop notification if disabled via env or config
if ($env:CLAUDE_NOTIFY_DESKTOP -eq "false" -or $NOTIFY_DESKTOP -ne "true") {
    exit 0
}

# ===== DESKTOP NOTIFICATION =====
Add-Type -AssemblyName System.Windows.Forms

$balloon = New-Object System.Windows.Forms.NotifyIcon
$balloon.Icon = [System.Drawing.SystemIcons]::Information
$balloon.BalloonTipTitle = $title
$balloon.BalloonTipText = $message
$balloon.BalloonTipIcon = [System.Windows.Forms.ToolTipIcon]::Info
$balloon.Visible = $true
$balloon.ShowBalloonTip(5000)

Start-Sleep -Milliseconds 100
$balloon.Dispose()

exit 0
