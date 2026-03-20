#!/bin/bash

# Claude Code Notification Hook
# Setup: /setup-notifications | Config: ~/.claude/notifications.conf

set -euo pipefail

# =============================================================================
# CONSTANTS
# =============================================================================

# Detectar diretorio do projeto (onde o script esta)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_CLAUDE_DIR="$(dirname "$SCRIPT_DIR")"

readonly CONFIG_FILE="$PROJECT_CLAUDE_DIR/notifications.conf"
readonly LEGACY_CONFIG_FILE="$HOME/.claude/notifications.conf"
readonly LEGACY_SLACK_FILE="$HOME/.claude/slack_user_id"
readonly SLACK_API_URL="https://slack.com/api/chat.postMessage"
readonly SLACK_BOT_TOKEN="${SLACK_BOT_TOKEN:-}"  # Set via environment variable
readonly DESKTOP_MESSAGE_MAX_LENGTH=100

# =============================================================================
# CONFIGURATION
# =============================================================================

load_config() {
  NOTIFY_DESKTOP=true
  NOTIFY_SLACK=true
  NOTIFY_PERMISSION_PROMPT=true
  NOTIFY_IDLE_PROMPT=true
  SLACK_USER_ID=""

  # Prioridade: config do projeto > config global
  if [[ -f "$CONFIG_FILE" ]]; then
    source "$CONFIG_FILE"
  elif [[ -f "$LEGACY_CONFIG_FILE" ]]; then
    source "$LEGACY_CONFIG_FILE"
  fi

  if [[ -z "$SLACK_USER_ID" && -f "$LEGACY_SLACK_FILE" ]]; then
    SLACK_USER_ID=$(tr -d '[:space:]' < "$LEGACY_SLACK_FILE")
  fi
}

# =============================================================================
# JSON PARSING
# =============================================================================

extract_json_field() {
  local json="$1"
  local field="$2"
  echo "$json" | grep -o "\"$field\":\"[^\"]*\"" | cut -d'"' -f4 || true
}

extract_message_field() {
  local json="$1"
  echo "$json" | sed -n 's/.*"message":"\([^"]*\)".*/\1/p'
}

# =============================================================================
# MESSAGE FORMATTING
# =============================================================================

get_tool_context() {
  local tool_name="$1"
  case "$tool_name" in
    Bash|bash)   echo "comando bash" ;;
    Write|write) echo "escrita de arquivo" ;;
    Edit|edit)   echo "edicao de arquivo" ;;
    Read|read)   echo "leitura de arquivo" ;;
    Task|task)   echo "subagent" ;;
    *)           echo "$tool_name" ;;
  esac
}

get_notification_emoji() {
  local type="$1"
  case "$type" in
    permission_prompt) echo ":warning:" ;;
    idle_prompt)       echo ":hourglass:" ;;
    *)                 echo ":bell:" ;;
  esac
}

get_notification_message() {
  local type="$1"
  local context="$2"
  case "$type" in
    permission_prompt)
      [[ -n "$context" ]] && echo "Permissao para $context" || echo "Permissao necessaria"
      ;;
    idle_prompt)
      echo "Aguardando sua resposta"
      ;;
    *)
      echo "Precisa da sua atencao"
      ;;
  esac
}

truncate_message() {
  local message="$1"
  local max_length="$2"

  if [[ ${#message} -gt $max_length ]]; then
    echo "${message:0:$max_length}..."
  else
    echo "$message"
  fi
}

# =============================================================================
# NOTIFICATION SENDERS
# =============================================================================

send_slack_notification() {
  local channel="$1"
  local text="$2"
  local tmp_payload="/tmp/claude_slack_$$.json"

  cat > "$tmp_payload" <<EOF
{"channel":"$channel","text":"$text"}
EOF

  curl -s -X POST \
    -H "Authorization: Bearer $SLACK_BOT_TOKEN" \
    -H "Content-type: application/json; charset=utf-8" \
    -d @"$tmp_payload" \
    "$SLACK_API_URL" > /dev/null 2>&1

  rm -f "$tmp_payload" 2>/dev/null
}

send_macos_notification() {
  local title="$1"
  local message="$2"
  # Remover caracteres problematicos
  title=$(echo "$title" | tr -cd '[:alnum:][:space:]._-')
  message=$(echo "$message" | tr -cd '[:alnum:][:space:]._-')
  osascript -e "display notification \"$message\" with title \"$title\" sound name \"Glass\"" 2>/dev/null || true
}

send_windows_notification() {
  local title="$1"
  local message="$2"
  powershell.exe -ExecutionPolicy Bypass -Command "
    Add-Type -AssemblyName System.Windows.Forms
    \$balloon = New-Object System.Windows.Forms.NotifyIcon
    \$balloon.Icon = [System.Drawing.SystemIcons]::Information
    \$balloon.BalloonTipTitle = '$title'
    \$balloon.BalloonTipText = '$message'
    \$balloon.BalloonTipIcon = [System.Windows.Forms.ToolTipIcon]::Info
    \$balloon.Visible = \$true
    \$balloon.ShowBalloonTip(5000)
    Start-Sleep -Milliseconds 100
    \$balloon.Dispose()
  "
}

send_linux_notification() {
  local title="$1"
  local message="$2"
  notify-send "$title" "$message"
}

send_desktop_notification() {
  local title="$1"
  local message="$2"

  if [[ "$OSTYPE" == "darwin"* ]]; then
    send_macos_notification "$title" "$message"
  elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]] || [[ -n "${WINDIR:-}" ]]; then
    send_windows_notification "$title" "$message"
  elif grep -qi microsoft /proc/version 2>/dev/null; then
    send_windows_notification "$title" "$message"
  elif command -v notify-send &> /dev/null; then
    send_linux_notification "$title" "$message"
  fi
}

# =============================================================================
# VALIDATION
# =============================================================================

should_notify() {
  local notification_type="$1"
  case "$notification_type" in
    permission_prompt) [[ "$NOTIFY_PERMISSION_PROMPT" == "true" ]] ;;
    idle_prompt)       [[ "$NOTIFY_IDLE_PROMPT" == "true" ]] ;;
    *)                 true ;;
  esac
}

can_send_slack() {
  [[ "$NOTIFY_SLACK" == "true" && -n "$SLACK_USER_ID" && -n "$SLACK_BOT_TOKEN" ]]
}

can_send_desktop() {
  [[ "$NOTIFY_DESKTOP" == "true" ]]
}

# =============================================================================
# MAIN
# =============================================================================

main() {
  load_config

  local input
  input=$(cat)

  local notification_type
  notification_type=$(extract_json_field "$input" "notification_type")

  should_notify "$notification_type" || exit 0

  local tool_name full_message context
  tool_name=$(extract_json_field "$input" "tool_name")
  full_message=$(extract_message_field "$input")
  context=""
  [[ -n "$tool_name" ]] && context=$(get_tool_context "$tool_name")

  local project_name emoji message
  project_name=$(basename "$(pwd)")
  emoji=$(get_notification_emoji "$notification_type")
  message=$(get_notification_message "$notification_type" "$context")

  # Slack notification (full message)
  if can_send_slack; then
    local slack_text="$emoji *$project_name* - $message"
    [[ -n "$full_message" ]] && slack_text="$slack_text\n\n$full_message"
    send_slack_notification "$SLACK_USER_ID" "$slack_text"
  fi

  # Desktop notification (truncated message)
  if can_send_desktop; then
    local desktop_message="$message"
    if [[ -n "$full_message" ]]; then
      local truncated
      truncated=$(truncate_message "$full_message" "$DESKTOP_MESSAGE_MAX_LENGTH")
      desktop_message="$message: $truncated"
    fi
    send_desktop_notification "$project_name" "$desktop_message"
  fi
}

main "$@"
