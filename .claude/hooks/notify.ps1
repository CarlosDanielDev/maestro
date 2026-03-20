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
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ProjectClaudeDir = Split-Path -Parent $ScriptDir

# Arquivos de configuracao (prioridade: projeto > global)
$ProjectConfigFile = Join-Path $ProjectClaudeDir "notifications.conf"
$LegacyConfigFile = Join-Path $env:USERPROFILE ".claude\notifications.conf"
$LegacySlackFile = Join-Path $env:USERPROFILE ".claude\slack_user_id"

# Valores padrao
$NOTIFY_DESKTOP = "true"
$NOTIFY_SLACK = "true"
$NOTIFY_PERMISSION_PROMPT = "true"
$NOTIFY_IDLE_PROMPT = "true"
$SLACK_USER_ID = ""

# Carregar configuracao (projeto primeiro, depois global)
$ConfigFile = if (Test-Path $ProjectConfigFile) { $ProjectConfigFile } elseif (Test-Path $LegacyConfigFile) { $LegacyConfigFile } else { $null }

if ($ConfigFile) {
    Get-Content $ConfigFile | ForEach-Object {
        if ($_ -match "^([A-Z_]+)=(.*)$") {
            Set-Variable -Name $matches[1] -Value $matches[2]
        }
    }
}

# Fallback para arquivo legacy de Slack
if (-not $SLACK_USER_ID -and (Test-Path $LegacySlackFile)) {
    $SLACK_USER_ID = (Get-Content $LegacySlackFile -Raw).Trim()
}

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
