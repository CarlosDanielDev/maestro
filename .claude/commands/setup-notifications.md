# Setup Notifications

Configure as notificacoes do Claude Code (desktop e Slack) para este projeto.

## Arquivo de Configuracao

As configuracoes sao salvas em `.claude/notifications.conf` (na raiz do projeto) com o formato:

```
NOTIFY_DESKTOP=true
NOTIFY_SLACK=true
NOTIFY_PERMISSION_PROMPT=true
NOTIFY_IDLE_PROMPT=true
SLACK_USER_ID=U0XXXXXXXX
```

## Fluxo do Comando

### 1. Verificar configuracao atual

Primeiro, leia o arquivo de configuracao do projeto (se existir):

```bash
cat .claude/notifications.conf 2>/dev/null || echo "Arquivo nao existe ainda"
```

### 2. Perguntar ao usuario o que deseja configurar

Use AskUserQuestion com as opcoes:
- "Ver/alterar status das notificacoes" - Ativar/desativar tipos de notificacao
- "Configurar Slack" - Configurar User ID do Slack
- "Resetar configuracoes" - Voltar ao padrao (tudo ativado)

### 3. Se escolher "Ver/alterar status"

Mostre o status atual e pergunte com multiSelect=true:

```
"Quais notificacoes voce quer ATIVAR?"
- Notificacoes Desktop (som e popup no sistema)
- Notificacoes Slack (mensagem direta no Slack)
- Avisos de permissao (quando Claude precisa aprovar algo)
- Avisos de idle (quando Claude aguarda sua resposta)
```

Depois salve as escolhas:

```bash
cat > .claude/notifications.conf << 'EOF'
# Configuracoes de notificacao para este projeto
# Valores: true | false

NOTIFY_DESKTOP=true|false
NOTIFY_SLACK=true|false
NOTIFY_PERMISSION_PROMPT=true|false
NOTIFY_IDLE_PROMPT=true|false
SLACK_USER_ID=ID_ATUAL_OU_VAZIO
EOF
```

### 4. Se escolher "Configurar Slack"

Pergunte se o usuario ja tem o Slack User ID.

Se nao tiver, instrua:
1. Abra o Slack
2. Clique no seu nome/foto no canto superior direito
3. Clique em "Perfil"
4. Clique nos tres pontinhos (...)
5. Clique em "Copiar ID do membro"

Peca o User ID e salve:

```bash
# Ler config atual
source .claude/notifications.conf 2>/dev/null

# Atualizar apenas o SLACK_USER_ID mantendo outras configs
cat > .claude/notifications.conf << EOF
# Configuracoes de notificacao para este projeto
# Valores: true | false

NOTIFY_DESKTOP=${NOTIFY_DESKTOP:-true}
NOTIFY_SLACK=${NOTIFY_SLACK:-true}
NOTIFY_PERMISSION_PROMPT=${NOTIFY_PERMISSION_PROMPT:-true}
NOTIFY_IDLE_PROMPT=${NOTIFY_IDLE_PROMPT:-true}
SLACK_USER_ID=NOVO_ID_AQUI
EOF
```

Envie mensagem de teste:

```bash
cat > /tmp/slack_test.json << 'PAYLOAD'
{"channel":"USER_ID","text":":white_check_mark: *Slack configurado com sucesso!* Voce recebera notificacoes do Claude Code."}
PAYLOAD
curl -s -X POST \
  -H "Authorization: Bearer $SLACK_BOT_TOKEN" \
  -H "Content-type: application/json; charset=utf-8" \
  -d @/tmp/slack_test.json \
  "https://slack.com/api/chat.postMessage"
```

### 5. Se escolher "Resetar configuracoes"

```bash
cat > .claude/notifications.conf << 'EOF'
# Configuracoes de notificacao para este projeto
# Valores: true | false

NOTIFY_DESKTOP=true
NOTIFY_SLACK=true
NOTIFY_PERMISSION_PROMPT=true
NOTIFY_IDLE_PROMPT=true
SLACK_USER_ID=
EOF
echo "Configuracoes resetadas para o padrao (tudo ativado, Slack nao configurado)"
```

### 6. Mostrar resumo final

Apos qualquer alteracao, mostre o status atual:

```
Status das Notificacoes (projeto: nome_do_projeto):
- Desktop: ATIVADO/DESATIVADO
- Slack: ATIVADO/DESATIVADO (configurado: Sim/Nao)
- Permissoes: ATIVADO/DESATIVADO
- Idle: ATIVADO/DESATIVADO
```
