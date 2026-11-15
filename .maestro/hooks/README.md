# Claude Code Notification Hooks

Notificacoes automaticas quando o Claude Code precisa da sua atencao.

## Setup Rapido

### 1. Notificacao Desktop

**Ja funciona!** Nenhuma configuracao necessaria.

### 2. Configurar Notificacoes

Execute no Claude Code:

```
/setup-notifications
```

Voce pode configurar:
- Notificacoes Desktop (som e popup)
- Notificacoes Slack (DM)
- Tipos de alerta (permissao, idle)

---

## Arquivo de Configuracao

As configuracoes sao salvas **por projeto** em `.claude/notifications.conf`:

```bash
# Configuracoes de notificacao para este projeto
# Valores: true | false

NOTIFY_DESKTOP=true
NOTIFY_SLACK=true
NOTIFY_PERMISSION_PROMPT=true
NOTIFY_IDLE_PROMPT=true
SLACK_USER_ID=U0XXXXXXXX
```

**Prioridade:** Config do projeto > Config global (`~/.claude/notifications.conf`)

**`SLACK_USER_ID` e so global:** o destino do Slack vem apenas de `~/.claude/notifications.conf` ou `~/.claude/slack_user_id`. Um `SLACK_USER_ID` no conf do projeto e ignorado, para que um repo clonado nao possa redirecionar suas notificacoes (#1102).

---

## Como funciona

| Evento | Notificacao |
|--------|-------------|
| Claude precisa de permissao | Desktop + Slack (se habilitados) |
| Claude aguarda sua resposta | Desktop + Slack (se habilitados) |

---

## Plataformas Suportadas

| Plataforma | Desktop | Slack |
|------------|---------|-------|
| macOS | Nativo com som | DM |
| Linux | notify-send | DM |
| Windows | Toast | DM |
| WSL | Toast | DM |

---

## Configuracoes via Variavel de Ambiente

### Desativar notificacao desktop temporariamente

```bash
export CLAUDE_NOTIFY_DESKTOP=false
```

---

## Troubleshooting

### Slack nao envia mensagens

1. Verifique a config: `cat .claude/notifications.conf`
2. Execute `/setup-notifications` novamente

### Notificacao desktop nao aparece (Linux)

```bash
sudo apt install libnotify-bin  # Ubuntu/Debian
```

### Notificacao desktop nao aparece (Windows)

Verifique se notificacoes estao habilitadas em Configuracoes > Sistema > Notificacoes.
