Parent: -

# maestro TUI layout optimization + session sidebar with circular progress rings

## Overview

Duas frentes, um épico: (A) refactor incremental do layout do TUI — sem reinventar nada — e (B) um trilho lateral de sessions com anéis de progresso circulares na Overview.

**Frente A — layout.** A home mantém três panes lado a lado em qualquer largura (`src/tui/screens/home/draw.rs:77-84`, split horizontal 30/35/35 sem breakpoint), então em telas pequenas os labels cortam no meio sem reticências e o activity log fica sem linhas — dor real reportada no issue #1157 (60x15). O grid de sessions da Overview também não tem respiro: linhas e colunas são splits contíguos `Constraint::Ratio(1, n)` (`src/tui/panels.rs:272-288`), então painéis adjacentes colam borda dupla na borda dupla, e `panel_border_type` ignora os argumentos e devolve sempre `BorderType::Double` (`src/tui/panels.rs:332-334`).

**Frente B — sidebar de sessions com anéis.** Hoje não existe um sumário visual compacto por session: a Overview é uma grade de painéis grandes. Os dados para um anel por session já existem: `session.context_pct: f64` em 0..1 (`src/session/types.rs:392`, já renderizado como gauge em `src/tui/panels.rs:464`), status com cor centralizada em `Theme::status_color` (`src/tui/theme.rs:659-678`), provider já classificado em `AgentVisualKind` (`src/tui/agent_badge.rs:14-31`). O que não existe é o rendering do anel: anel circular não é widget nativo do ratatui. Essa é a bifurcação central do épico — Canvas do ratatui vs matriz braille desenhada à mão vs arcos em half-blocks — resolvida por medição no spike {{s1}} antes de qualquer widget final.

**Posicionamento (não é colisão):** já existe uma série UX-0 aberta de chrome/sidebar ACCORDION para navegação de ferramentas por buckets (#826, #827, #832; scaffold sem call sites em `src/tui/navigation/sidebar_state.rs:1-4`). Este épico é outra coisa: um trilho de SESSIONS com anéis dentro da Overview. Não depende daquela série e não compartilha arquivos além de `src/tui/ui.rs`.

## Expected Behavior

- Home em terminal estreito (abaixo de um threshold definido na issue) empilha os panes em vez de cortar labels; labels cortados terminam com `…`; o activity log recebe pelo menos uma linha (#1157).
- Células do grid da Overview têm um respiro de 1 col/1 row entre si e uma tabela de estilo de borda documentada (selected/conflito/normal), sem regressão na paginação.
- A Overview mostra, à esquerda, um trilho fixo de ~4 colunas com um anel por session: arco preenchido sobre trilha escura, glifo do provider dentro do anel, uma linha em branco entre anéis, scroll quando sessions > altura, e destaque visual no anel da session selecionada.
- O anel comunica estado assim (especificação fechada em {{s2}}): anel cheio = Completed; anel vazio = Queued/Spawning; arco parcial = em progresso (arco = `context_pct`); vermelho piscando = Errored/FailedGates/Killed; a cor do anel segue `Theme::status_color`.
- Navegação: setas continuam movendo a seleção do grid, e a seleção é compartilhada — o anel correspondente ganha destaque e a janela do trilho rola para mantê-lo visível; Enter abre o Detail pelo caminho existente (`src/tui/input_handler.rs:1099-1110`). `k` continua sendo kill (`src/tui/input_handler.rs:1054-1062`), então NÃO se usa j/k para navegação.

## Escopo do épico

### Frente A — layout optimization

- {{a1}} — refactor(tui): home panes empilham abaixo de um threshold de largura, labels truncam com `…`, linhas separadoras vazias colapsam (fecha a dor #1157).
- {{a2}} — refactor(tui): células do grid da Overview ganham gap de 1 col/1 row e uma tabela de estilo de borda documentada.

### Frente B — sidebar com anéis

- {{s1}} — spike(tui): medir as três técnicas de rendering de anel e escolher uma com números.
- {{s2}} — feat(tui): widget puro de anel de progresso (arco + cor de status + blink).
- {{s3}} — feat(tui): trilho (container) de sessions — um anel por session com glifo do provider, respiro e scroll.
- {{s4}} — feat(tui): dockar o trilho na Overview com navegação compartilhada por teclado.

## Dependency Graph (Implementation Order)

Level 0 — no dependencies:
• {{s1}} spike: rendering de anéis (medição de custo)
• {{a1}} refactor: home responsiva (fecha a dor #1157)
• {{a2}} refactor: células do grid com respiro e bordas consistentes

Level 1 — depends on {{s1}}:
• {{s2}} feat: widget de anel de progresso

Level 2 — depends on {{s2}}:
• {{s3}} feat: trilho de sessions (container + scroll + provider glyph)

Level 3 — depends on {{s3}} + {{a2}}:
• {{s4}} feat: dock na Overview + navegação compartilhada

Sequence: {{s1}} ∥ {{a1}} ∥ {{a2}} → {{s2}} → {{s3}} → {{s4}}

## Done when

- [ ] todas as sub-issues fechadas por PR mergeado — `gh issue list -R CarlosDanielDev/maestro --state open` não lista mais nenhuma delas — owner: implementer
- [ ] a Overview com o trilho renderiza em 80x24 e 120x40 sem regressão de snapshot nos testes existentes — owner: implementer

## ADR stub

Bifurcação do épico: (a) fazer as duas frentes como uma só issue grande (rejeitado — viola a regra de sizing do wave, ~100k de contexto por issue); (b) duas épicos separados (rejeitado — o trilho muda a largura disponível do grid da Overview, então o refactor de células {{a2}} deve aterrissar antes do dock; um épico mantém a ordem visível); (c) um épico com 6 issues na sequência `{{s1}} ∥ {{a1}} ∥ {{a2}} → {{s2}} → {{s3}} → {{s4}}`. Recomendação (para discordar): (c).

## Out of scope

- A série UX-0 de chrome/sidebar accordion de navegação por buckets (#826/#827/#831/#832/#833/#835) — outra feature, outro trilho.
- Novos modos de TUI ou substituição do grid de painéis da Overview — o trilho é adicional.
- Métrica de "progresso de tarefa" sintética: maestro não tem % de tarefa; o arco usa `context_pct` e a semântica de status especificada em {{s2}}.
- Temas novos; tudo usa o theme system existente (`src/tui/theme.rs`).

## Execução

- `wave plan` sobre este diretório, depois `/setwave:wave {{epic}}`.
- Gate: a lista em `.wave.json`.
