# Setup Notifications

Configure as notificacoes do Claude Code (desktop e Slack) para este projeto.

## Arquivo de Configuracao

As configuracoes sao salvas em `.claude/notifications.conf` (na raiz do projeto) com o formato:

```
NOTIFY_DESKTOP=true
NOTIFY_SLACK=true
NOTIFY_PERMISSION_PROMPT=true
NOTIFY_IDLE_PROMPT=true
```

O Slack User ID nao fica neste arquivo: ele fica so em `~/.claude/slack_user_id` (veja o passo 4).

Se voce configurou o Slack antes do #1299 e tem uma linha `SLACK_USER_ID=` no conf do projeto, mova o ID para `~/.claude/slack_user_id` e apague a linha — o hook ignora essa linha no conf do projeto.

## Fluxo do Comando

### 1. Verificar configuracao atual

Primeiro, leia o arquivo de configuracao do projeto (se existir):

```bash
cat .claude/notifications.conf 2>/dev/null || echo "Arquivo nao existe ainda"
cat ~/.claude/slack_user_id 2>/dev/null || echo "Slack User ID nao configurado"
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

O User ID e salvo so no arquivo global do usuario. O hook ignora `SLACK_USER_ID` no conf do projeto, para que um repo clonado nao possa redirecionar as notificacoes (#1102).

```bash
mkdir -p ~/.claude
printf '%s\n' "NOVO_ID_AQUI" > ~/.claude/slack_user_id
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
EOF
echo "Configuracoes resetadas para o padrao (tudo ativado). O Slack User ID em ~/.claude/slack_user_id nao foi alterado."
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
