# STUDY — session follow-ups + setwave fold-in (2026-11-27)

Durable notes of the 2026-11-27 study session. Scaffold lives beside this file
(`index.tsv` + bodies, lint 6/7 green, `wave plan --dry-run` = 7 issues, 3 deps, nothing created).

## 1. Review — maestro's own product sessions

- A session's record (`crates/maestro-core/src/session/types.rs:330`) carries `last_message`,
  `activity_log`, turns, cost, tokens, `files_touched` — all dropped at teardown.
  `maestro-state.json` holds 0 session records after runs; only aggregates survive.
- The settle path (`crates/maestro-tui/src/tui/app/completion_pipeline.rs:13`) runs gates + PR.
  Nothing reads the ending for leftover work.
- `.maestro/logs/`: 99 logs, all synthetic — leaked by tests (`app/mod.rs:402` +
  `logger.rs:20`, relative `default_dir()`). Real session logs: none survive.
- 5 `pending_prs` entries `permanently_failed` since 2026-04-30/05-25, never reaped
  (`pr_retry.rs:89`, removal only for completed at `pr_retry.rs:128-131`).
- Cost of the missing harvest loop (orchestration history): 25 follow-ups accumulated
  2026-11-14 → 11-24, reconciled by hand in one full session → #1485–#1500
  (21 filed, 1 fixed, 1 obsolete, 1 dup, 3 unconfirmable). Waves 20/21 follow-ups all filed
  (#1505–#1509, #1514, #1515). Backlog at study time: zero.

## 2. Side agent — decision frame

- Regex/hook extraction rejected as the core: 4 of the 25 historical items were stale/obsolete —
  the work is judgment (dedup vs open issues, re-check vs main), not extraction.
- Chosen shape (scaffolded): record at settle (s1) → cheap one-shot harvester, opt-in (s2) →
  owner review in Run summary (s3) → export to a lint-clean setwave plan dir (s4);
  `wave plan` stays the only door to GitHub. Consent doctrine preserved.
- Open pendency (blocks epic Ready): parent umbrella for the epic — standalone (`Parent: -`,
  precedent #1183) vs a future workflow epic. Owner: Carlos.

## 3. setwave × maestro issues/milestones layer — the fold-in sizing

Decision already on record (MEMORY.md 2026-11-10): setwave stays the GitHub execution layer;
fold-in of the wave loop into the product is the candidate next phase — **shell out or plugin
first, never a Rust port** of wave.py (~4.9k tested lines).

### What maestro already has

- `provider/github/` — full gh layer (client, transport, pr, merge, ci, labels, redaction) + issue_cache.
- `issue_browser` — list, multi-select, shared launch dialog (#1262), launch budget. No readiness notion.
- `issue_wizard` — AI-assisted issue creation with its own lint/gate.
- `milestone_health/` (1,114 lines) — own DoR checker (`dor.rs`), dependency graph (`graph.rs`),
  report/patch. Powers the milestone review surface.
- `orchestration/dag.rs` (269 lines) — `parse_blocked_by`, dependency levels.
- Session engine: spawns agents in worktrees with prompts, queue, budgets, retry, teardown.

### What setwave exposes (wave.py CLI)

- JSON today: `next`, `epics`, `verify`, `order`, `guarantees` (all `--json`).
- Text only: `status`, `lint`, `why`, `agents`, `sweep`, `stats`.
- Write side: `dispatch`, `prepare`/`amend`, `judge`, `merge`, `resolve`, `close-parents`,
  `tick`, `plan` (the only GitHub-creation door), `adopt`, `cleanup`, `hooks`.
- `guarantees` = tested promise list — the natural place to pin a stable `--json` contract.

### The three real overlaps (two sources of truth today)

1. **DoR**: maestro `milestone_health/dor.rs` vs `wave lint`. Pivot already resolved in setwave's
   favor (machine-checked beats prose). Maestro's checker predates that decision.
2. **Blocked-by graph**: maestro `dag::parse_blocked_by` + `milestone_health/graph.rs` vs wave's
   `blocked_by` API wiring. #1494 (open) already wants ONE parser inside maestro.
3. **Spawn engines**: `wave dispatch` (worktrees + prompts + tmux seats) vs maestro's session
   pool (worktrees + prompts + provider processes). Same job, two engines.

### Options

- **(a) Read-side link — ~1 wave (3–4 issues).** A `wave` provider module (feature-detect like
  `gh auth`; parse `--json`) feeding: issue_browser rows with READY/NOT-READY + `why`
  (answers #1268's "can an agent take this now?"), milestones tab with `wave status` tree,
  milestone review with `wave lint` gaps. Needs small setwave additions: `--json` on
  `status`/`lint`, pinned via `guarantees`. Companion: #1494 (one parser) so the TUI graph and
  wave's graph cannot disagree.
- **(b) Execution fold-in — an epic, ~2–3 waves (6–10 issues).** Maestro sessions become the
  seats: dispatch metadata drives the pool, `agents --watch` liveness feeds session health,
  judge/verify verdicts land in the Run summary, merge behind armed confirms (design already
  sketched in #1277). Unifies overlap #3. This is the pivot's "candidate next phase".
- **(c) Rust port of wave.py — rejected** (pivot decision; 4.9k tested lines).

Recommended order: (a) → harvester epic (this dir) → (b). (a) is cheap, ships visible value,
and forces the stable `--json` contract (b) needs anyway. The harvester scaffold's s4-export
(maestro → `wave plan`) is connection dot #0 — it already exists in this plan dir.

## 4. Fold-in total — contagem de issues (baseline: v0.34.0 lançada)

Visão do owner (2026-11-27): setwave = maestro em escala menor; juntar os dois mundos —
a inteligência de issues pequenas do setwave + o motor de spawn do maestro — consumindo
menos tokens, sem perder qualidade nem o flow do TUI. Follow-ups gerados por sessions são
naturais (planejamento nunca é perfeito); o harvester torna a imperfeição barata.

Dados reais (`wave stats`, 2026-09-30 → 11-27): 7.570 runs; 315 dispatches (≈315 seat sessions);
1.414 judge invocations (955 com falha/rodada — o maior custo repetido); lint 242 runs, 179 falhas
(refinamento iterativo funciona, mas custa); merge 321 runs, 74,7s mediana.

**Contagem: 21 issues** — 7 já rascunhadas neste dir, 14 novas:

| bloco | issues | o quê |
|---|---|---|
| já scaffoldado | 7 | harvester epic + s1–s4 + 2 bugs (este dir, lint 6/7) |
| setwave (repo dele) | 2 | `--json` em status/lint/why pinado em guarantees; dispatch emitindo seats machine-readable |
| read-side no maestro | 4 | provider wave (detect/spawn/parse/cache); readiness no issue browser (#1268); roadmap/milestones via `wave status` (#1272); milestone review via `wave lint` + aposentar dor.rs p/ fallback (#1283) |
| fold-in de execução | 7 | dispatch bridge (sessions = seats); prompts via run_plan estruturado (o cortador de tokens); liveness `agents --watch` → health; vereditos judge/verify → Run summary + PR review; merge bridge atrás de confirm armado (#1277); ledger único (#1370 ↔ stamps); cleanup único |
| prova de tokens | 1 | telemetria de custo por wave/seat nas Usage views (#1289) — mede a promessa "menos tokens" |

O que a 0.34.0 já entrega e NÃO conta: #1268, #1269, #1272, #1277, #1285, #1288, #1289, #1370,
#1494, #1496 — as superfícies onde os pontos pousam. Sem elas a conta seria ~31.

Epics: 3 (read-side, harvester, execução) + 2 soltas no repo setwave. Ritmo demonstrado
(4–5 merges/wave): ~5–6 waves. Milestones sugeridas: read-side + harvester → v0.36.0;
execução → milestone nova (v0.41.0 "setwave fold-in") — v0.36.0 já tem 24 abertas. Decisão: Carlos.

Economia de tokens — de onde vem (sem inventar número que não medi):
- reconcile sessions (a de 25 itens gerou transcript de 9,9 MB) → harvester lê records de ~2 KB no modelo mais barato;
- 1.414 judge runs: readiness pré-computada evita dispatch de NOT-READY (cada fail round = 1 seat + 1 judge session);
- addendum em prosa relido por seat (~2,5k tokens) → prompt montado por dados via run_plan, cortado por issue;
- orquestrador parando de gastar turno com polling de status (vereditos chegam no produto).
Não medi: tokens por wave/seat hoje (o log do wave guarda runs/duração; spend por budget existe via `--budget`).
A issue de telemetria existe exatamente pra transformar essa promessa em número.

## 5. Avaliação profunda — por que o motor multi-épico do maestro "não ficou bom"

Pergunta do owner (2026-11-27): manter setwave separado; querer o CONCEITO de autonomia e
entrosamento do setwave dentro do maestro, pra rodar vários épicos de uma vez. Avaliação
estática (código + estado real + git + MEMORY), sem rodar um team launch.

**Veredito: o maestro construiu o lançamento; o setwave é o ciclo de vida.** Cinco causas raiz:

1. **Unidade de trabalho errada.** Maestro modela a SESSION (ciclo completo,
   `session/types.rs:330`); a issue não tem ciclo de vida no produto — READY/dispatched/judged/
   merged não moram em lugar nenhum. `team_runs` persiste no tipo (`state/types.rs:105`) mas o
   estado real tem 0. Multi-épico nem existe no tipo: `TeamInput::IssueSet` carrega UM
   `primary_milestone` (`orchestration/scheduler.rs:33-38`). setwave: a issue É a máquina de
   estados e o estado mora no GitHub (sub-issues, blocked_by API, ledger no PR, comentários) —
   por isso `wave epics` lista todos os épicos do registry, prontos primeiro.

2. **Motor construído de baixo pra cima (telas), não de contratos.** 17 arquivos de
   `orchestration/` + `work/` carregam `#![allow(dead_code)]` (contracts, orchestrator, executor,
   queue, dependencies...) — compilam, têm testes, e a maior parte nunca ganhou consumidor.
   `team_runner.rs` admite no docstring: "true L1 sub-agent routing remains a follow-up" — o L1
   que enforçaria os contracts nunca existiu. O polimento foi pro momento do launch (PRs #1437,
   #1442, #1465: wizard, worst-case, budget lock). O setwave gasta o código DEPOIS do launch:
   1.414 judge runs em 2 meses.

3. **Sem gate de qualidade no loop.** O loop maestro termina em "gates passam + PR aberto";
   depois disso, humano. Sem judge (grep por judge/verdict: zero no motor), sem verify, sem
   ordem de merge (`provider/github/merge.rs` só CHECA conflito), sem baixa do Done-when.
   setwave: judge é merge-blocking; 122 garantias testadas (`wave guarantees --json`).

4. **O DOR existe mas não alimenta o dispatch.** `milestone_health/dor.rs` computa readiness
   pra TELA; scheduler/dispatch nunca consultam (grep: zero). setwave: `dispatch` roda doctor e
   RECUSA issue não-READY. Evidência do filtro trabalhando: 179 falhas de lint em 242 runs —
   custo pago antes do token, não depois.

5. **Entrosamento por memória vs por artefato.** Maestro: App struct + canais async +
   maestro-state.json reescrito por save — nada auditável de fora (a própria #1370 nasceu porque
   "the only observable surface is the TUI"). setwave: cada passo lê o artefato do anterior —
   issue body ← `prepare`; stamp ← dispatch; `judge-<PR>.json`; plan file com sha pinado ←
   `order --plan` (merge recusa se head moveu); ledger no corpo do PR; `log.jsonl` com 7.570
   runs. Crash mata o processo e perde nada.

**Os 5 invariantes transferíveis (o conceito, não o código):**
1. Issue é a máquina de estados; session é detalhe descartável.
2. Estado mora fora do processo (GitHub/artefatos); processo é descartável.
3. Todo handoff entre passos é artefato verificável por máquina.
4. Nada entra no motor sem contrato (gate de readiness no dispatch).
5. Nada sai sem veredito (judge blocking) e sem baixa (ledger aplicado no merge).
Meta-padrão: guarantees — toda promessa com o teste que a pinna.

**Boa notícia da 0.34.0:** #1268 (readiness), #1277 (merge armado), #1370 (ledger), #1494
(parser único) já são 4 dos 5 invariantes ganhando casa. O que falta de nativo: dispatch gate
(recusar não-READY), judge seat merge-blocking + superfície de veredito, baixa do ledger no
merge, ordem de merge, scheduler multi-milestone, registry de guarantees do maestro.

**Recontagem vs seção 4 (shell-out descartado pelo owner):** saem as 6 issues de ponte
(2 setwave + 4 read-side); entram ~8-10 nativas (gate, judge ×2-3, ledger tick, ordem, multi-epic
×1-2, guarantees). Total continua ~20-24 issues; o harvester (7, já rascunhado) é independente
e vale igual. Milestone natural do bloco nativo: a "v0.41.0 setwave-grade engine" — nome a decidir.

**Flags de honestidade:** leitura estática (não rodei team launch real); `#![allow(dead_code)]`
≠ arquivo 100% morto (teams launch USA scheduler/run_plan — as partes vivas são launch e níveis);
não medi tokens por seat (issue de telemetria continua sendo a resposta).

## 6. Hands-on — maestro nightly (30f3b1d8, v0.33.1) no lab maestro-qa-todo (2026-11-27)

Binário construído de origin/main em worktree limpa (`~/projects/maestro-nightly`); o checkout
principal estava divergente (-984 linhas vs origin/main — não era nightly). Lab isolado com
claude sonnet. Custo total do teste < $0,50.

**O que funciona de verdade:**
- doctor + trust gate (#1045): recusa comando com path sem workspace trusted, lista o que será
  confiado, recusa não-interativo sem `--yes`, re-checa quando o config muda. Impecável.
- `queue`: readiness real (Ready/Blocked, blocked-by da API). `run --issue 3` RECUSA:
  "#3 is blocked by #1 (open); pass --ignore-deps" — erro honesto e acionável.
- Loop single-issue headless: `run --once --issue 9` → worktree → sessão (prompt RICO: corpo da
  issue + acceptance criteria + safety guards + "unattended mode") → gates rodam de verdade
  (`.maestro/logs/gates/issue-9/tests_pass.log`) → push → PR #10 com Closes #9 → labels
  ready→done. 56s, $0.03.
- TUI: Home polida (tabs, budget, ações com teclas, What's new), trust/BYPASS visível.

**O quebrado, com mecanismo (smoking guns):**
1. **`team launch` está 100% morto.** Espião no argv revelou: o dispatch spawna
   `claude --print --output-format text ...` (headless.rs build_text_args) mas o consumidor
   (`drive_provider`, dispatch.rs:249) espera `StreamEvent::AssistantMessage` — que só o caminho
   stream-json emite. Mismatch texto-vs-stream = falha garantida em toda máquina:
   "malformed parser output: provider closed stream without AssistantMessage". Os testes usam
   SchedulerRunner enlatado ("without touching providers", team_launch.rs:36) — CI verde com a
   feature morta.
2. **Prompt do team é uma linha:** "Implement issue #N and return CodeChange." (pipeline.rs:44) —
   sem corpo da issue, sem critérios, sem schema do "CodeChange". O prompt L2 rico existe
   (orchestrator.rs:53) mas o caminho headless usa a máquina determinística com instrução stub.
3. **Team roda no checkout principal** (sem worktree, cwd herdado — espião confirmou) e em
   permission mode default; o caminho single-issue cria worktree isolada.
4. **Team ignora blocked-by** (despachou #3 bloqueada; o `run` recusa a mesma issue).
5. **Observabilidade da falha:** erro aponta pro parser (camada errada), zero session log, zero
   state record, zero custo registrado, RUST_LOG mudo.
6. **PR aberto existente ⇒ trabalho novo órfão e silencioso:** issue #1 (PR #4 velho aberto) —
   sessão completou, commit fa0f791 ficou só local, nada empurrado, nada dito no sumário.
7. **PR #10 diz "Cost: $0.00"** — custo real $0.03. O evento COMPLETED carrega $0.00 com a linha
   COST $0.03 logo acima (log 4c217535) — bug de accounting que vira número falso em artefato.
8. **Estado final inconsistente:** `gates_pending: True` e `worktree_branch: None` em sessões
   completed (gates rodaram; branch existiu).
9. **TUI cega pro próprio estado:** Dashboard "SESSIONS 0 / PR PIPELINE 0" com 4 PRs abertos no
   repo (incl. PR #10 criado pelo maestro 20min antes) e 3 sessões no state; header $0.00 com
   custo no state. "Finished work shows up here" — não aparece.
10. **Issues browser sem readiness** (só número+título) — o CLI `queue` sabe mais que o TUI
    (gap do #1268 observado vivo).
11. **Vazamento de ambiente pros seats:** os SessionStart hooks da máquina (caveman mode!,
    ai-memory profile), 13 eventos de hook, ~40 MCP servers e o CLAUDE.md global entram em todo
    seat — custo de tokens por seat + interferência comportamental (seat gastou saída
    reconciliando "your global rule vs harness reminder"); logs poluídos de UNKNOWN.
12. **Maestro não dirige kimi:** sem AgentKind::Kimi; schema stream-json do kimi difere do
    claude; `--permission-mode` é opção desconhecida no kimi. (kimi 2.5.3 testado.)

**Veredito hands-on:** o instinto do owner está correto e agora tem mecanismo. O single-run é
produto; o multi-issue é um universo paralelo que não compartilha NADA com ele (builder de
prompt, worktree, readiness, transporte do provider, superfície de erro) — dois motores, um
polido, um morto. E as superfícies (TUI) não leem o estado que o motor escreve. São os 5
invariantes da seção 5 observados ao vivo.

## 7. BENCHMARK medido — maestro vs setwave, mesmas tarefas, mesma máquina (2026-11-27)

Lab: maestro-qa-todo, base e054123, modelo sonnet nos dois braços. 3 issues boas
(count/remove/--version, com Done-when) + 1 ruim ("improve the todo thing").

| métrica | braço maestro (nightly) | braço setwave |
|---|---|---|
| issues → PR | 3/3 (#15–17) | 3/3 (#22–24) |
| wall (3 issues) | 90s (pool 2) | 97s (3 paralelos) |
| custo total | **$0.07** | **$1.31** (18× por issue: $0.44 vs $0.023) |
| custo reportado pela ferramenta | "$0.00" no corpo do PR (falso) | custo real no result event |
| gate independente (meu re-run) | 3/3 OK | 3/3 OK |
| ledger Done-when no PR | 0/3 (verify: LEDGER-MISSING) | 3/3 (nit: #24 sem Closes no body) |
| worktrees | isoladas + teardown limpo ✓ | isoladas pelo dispatch ✓ (nit: .codegraph suja o verify do próprio setwave) |
| issue ruim | despachou: $0.02, 40s, escopo duplicado (#1/#2), PR #21 que o próprio sumário do seat negou | **recusou em ~3s a $0.00**, gaps nomeados |
| double-dispatch (PR aberto) | commit órfão silencioso (fa0f791) | recusou nomeando o PR (wave why) |
| motor multi-issue (team launch) | 0/3 (morto por construção); 76 linhas de resíduo não-commitado no checkout principal com relatório "0 succeeded" | é o braço inteiro: 315 dispatches/2 meses |
| blocked-by | run ✓ recusa / team ✗ ignora | dispatch ✓ (API) |
| falha | "malformed parser output" (camada errada), sem logs | premissa nomeada + evidência |

Leitura: código bom os dois produzem (6/6 gates OK no meu re-run). A diferença é tudo em volta:
gate antes de gastar, dedup, ledger, honestidade de artefato, estado auditável. E o custo é
assimétrico ao contrário do esperado: o seat disciplinado do setwave custa 18× mais — a meta
"menos tokens" do owner aponta pro híbrido: seat barato do maestro + contrato do setwave.

Nota de ambiente: o lab não tem CI (verify marca CI-UNKNOWN — doutrina correta: falha de
ferramenta nunca é verde); clone shallow quebrou o primeiro verify (refspec só de main) —
ambos artefatos do lab, não dos motores.

## 8. COUNCIL (skill /council, triad shipping, --quick) — veredito sobre o épico pós-0.34.0

Membros: Torvalds + Musashi + Feynman (personas de ~/.claude/agents, claude sonnet, 2 rounds:
análise 200w → posição final 75w). Brief = números das seções 6–7. Custos: 6 chamadas ≈ $0.25.

**Veredito unânime (3/3):**
1. **0.34.1 (patch)** — 5 consertos: motor team morto (text-vs-stream), resíduo de subagentes
   no checkout principal (data loss — "ship first"), custo falso ($0.00 no PR + total_cost que
   reseta entre runs: 0.0737 → 0.0245 medido), Dashboard cego ("0 PRs" com 4 abertos), guard de
   PR-existente (fim do commit órfão silencioso).
2. **Próxima minor livre = v0.37.0 (NÃO v0.41.0)** — o conceito setwave em fatias: DOR gate no
   dispatch, ledger no merge, guarantees registry, recusa de double-dispatch, state machine da
   issue. v0.35/v0.36 já estão cheias; Musashi: considerar reordenar o motor à frente de input
   standardization.
3. **Judge por último e opt-in** até medir o custo (flag do Feynman: os 18× do setwave não estão
   atribuídos — medir antes de copiar).
4. **Insistência unânime:** o primeiro patch inclui UM teste no contrato real do provider
   (argv real vs parser real). Fakes enlatados esconderam o motor morto — esse teste É a
   primeira guarantee do registry.

Regra de corte (Feynman): "o código prometia X e faz não-X → patch; nunca prometeu X → minor".
Hedge do Musashi: se o patch listar mais de ~6 itens, é uma minor disfarçada.

## 9. Viabilidade — "issue artifacts" como conceito nativo do maestro (2026-11-27)

Motivação: o owner pediu o STUDY.md como "artifact" da #1518. A API do GitHub não tem endpoint
de anexo pra issues (o botão "Add artifact" da UI nova é web-only) — o equivalente durável via
API é um comentário com `<details>` (postado: #1518 comment 7224777327). Isso expõe o gap: o
maestro não tem onde pendurar arquivos de conhecimento de uma issue (estudos, plan dirs, patches
de evidência como team-residue.patch, logs de benchmark, vereditos, session-end records).

Fatos checados no código (nightly 30f3b1d8):
- `compose_prompt` (`orchestration/dispatch.rs:90`) monta 3 seções de texto — nenhum conceito
  de arquivo anexo; seats recebem só prosa.
- Issue browser JÁ tem painel de preview (`issue_browser/draw.rs:34` `draw_preview`) — superfície
  pronta pra listar artifacts.
- Zero conceito de artifact/attachment no core (grep).
- `issue_cache` no state guarda number/title/body — sem anexos.

Opções de storage:
- (a) Comentário-artifact no GitHub (um por arquivo, edit-in-place): durável, cross-machine,
  visível na web, zero storage novo. Limite 65k chars/comentário → chunk ou repo-commit pra
  arquivos grandes. **Recomendada como store de conhecimento** — a lição medida do lab é que
  estado local não viaja (a era "box" morreu com a máquina).
- (b) `.maestro/artifacts/<issue>/` local: cache offline rápido, mas machine-bound. Papel: cache.
- (c) Repo-committed (`plans/`): versionado e revisável — certo pra plan dirs (padrão setwave),
  errado pra evidência efêmera.

Integração com o que já está fileado: o prompt do seat carrega REFERÊNCIAS (paths/URLs) e não
conteúdo — a resposta medida pro custo 18×; o harvester (#1535) lê o session-end record como
artifact; o judge (#1531) cita artifacts como evidência; o export (#1537) escreve plan dirs que
são artifacts; o lifecycle record (#1526) é o índice natural.

Veredito: viável, 1 story pra primeira fatia (attach + list + prompt-by-reference), filha do
#1524, milestone v0.41.0, sem blockers. Cresce por uso (chunking, cache local, surfaces) em vez
de nascer grande.

## Status

- **SHIPPED 2026-11-27 (owner OK in conversation):** milestone **v0.34.1 — Engine Repair** (#69):
  epic #1518 + patches #1519–#1523 (p2←p1 wired). Milestone **v0.41.0 — Setwave-grade Engine**
  (#70): epic #1524 + slices #1525–#1532 (e3←e2, e4←e2, e6←e1, e7←e8+e3 wired) **plus the
  harvester epic #1533 + #1534–#1539** (s2←s1, s3←s2, s4←s3 wired; bug-logs #1538 and
  bug-pendingprs #1539 as extra leaves). Sub-issues linked, blocked_by via API, dependency
  graphs for both epics in the milestone #70 description, `wave lint` green on all leaves
  (Miller âmbar warnings inside ceiling). numbers.json in each plan dir.
- Harvester bodies corrected before filing: labels to repo convention (`type:feature`),
  milestone v0.36.0→v0.41.0, s1 premise updated to the measured truth (records persist but
  incomplete/unread — not "dropped at teardown").
- Epic bodies carry the known top-level `Parent: -` note (#1183 precedent).
- The harvester scaffold (this dir) remains local, unfiled — its own OK pending.
