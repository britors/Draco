# Matriz de paridade Rust + GTK4

Lista de aceite funcional usada na migração da antiga UI Electron/TypeScript
para a interface nativa Rust + GTK4/libadwaita. A implementação legada já foi
removida dos fontes (ver `git log` anterior à reescrita para referência do
comportamento original).

Estados permitidos: `pendente`, `em desenvolvimento`, `implementado`,
`validado` e `desvio aprovado`. `Implementado` indica que a superfície foi
entregue e compila/roda; `validado` exige também passar pelo checklist
transversal abaixo contra um Postgres real. Um desvio precisa apontar para a
decisão que o aprovou.

| Milestone | Módulo | Superfície obrigatória | Estado |
|---|---|---|---|
| M1 | Núcleo de dados (`draco-core`) | pool Postgres (`tokio-postgres`+`deadpool`, TLS via `rustls`), túnel SSH (`russh`, com jump host e known_hosts TOFU), introspecção/DDL/dashboard/stats/roles/jobs/activity/locks/extensions (`postgres::queries`), storage local (TOML/XDG), segredos (`oo7`), parser `schema.draco` | implementado |
| M2 | Conexões | criar/editar, túnel SSH e jump host | implementado (falta: testar antes de salvar, excluir, indicador de status, favoritos) |
| M2 | Explorer | árvore conexão → schema → tabela (leaf, sem colunas inline — ver M4), expansão lazy | implementado (falta: sequências/extensões no próprio Explorer, filtro/busca inline, badge de contagem de linhas) |
| M3 | Editor SQL | GtkSourceView5 (highlighting/scheme dependem de dados do sistema — ver nota), seletor de conexão, execução | implementado (falta: abas múltiplas, autocomplete, cancelamento, atalho Ctrl+Enter) |
| M3 | Resultados | grid (`ColumnView`, colunas dinâmicas por query) | implementado (falta: detalhe de linha, export CSV/JSON) |
| M3 | Histórico e Snippets | histórico de queries (máx. 50), snippets nomeados | pendente (`draco-core::store` já tem as funções; falta UI) |
| M4 | DDL Viewer | DDL completo somente leitura (GtkSourceView5 read-only) | implementado |
| M4 | Índices e Constraints | listagem, tamanho, tipo, definição | implementado |
| M4 | Mapa de FKs | FKs de saída/entrada | implementado (falta: clicar para navegar até a tabela referenciada) |
| M4 | Detalhe de Tabela | colunas (tipo completo, default, PK/FK), constraints, índices, mapa de FK | implementado (falta: estatísticas de coluna, `get_column_stats` já existe em `draco-core`) |
| M5 | Dashboard de Conexão | gauges custom (`cairo`) para cache hit/uso de conexões/rollback rate, info de servidor/banco, KPIs, top 10 tabelas com barra proporcional | implementado (falta: abrir automaticamente ao conectar) |
| M5 | Stats do Banco | bloat de tabela, índices não usados, hot spots de seq scan — dobrado na mesma aba do dashboard | implementado |
| M6 | Criador de Tabela | formulário visual, colunas dinâmicas (tipo/null/PK/unique/default), preview de `CREATE TABLE` (botão "Refresh Preview", não totalmente live) | implementado (falta: FK inline, "nova schema" — hoje só cria na schema já aberta) |
| M6 | Editor de Tabela (`ALTER`) | — | **pendente** (não iniciado nesta passada) |
| M6 | Editor de Função | GtkSourceView5, Save, Validate (BEGIN/ROLLBACK), Test | implementado |
| M7 | Roles | listar, excluir, atributos (superuser/createdb/createrole/login) | implementado (falta: formulário de criação — `create_role` já existe em `draco-core`) |
| M7 | Jobs (pg_cron) | listar, pausar/retomar (switch), excluir; detecta pg_cron ausente | implementado (falta: criar/editar job, histórico de execução — `create_job`/`update_job`/`get_job_runs` já existem) |
| M7 | Activity & Locks | sessões (`pg_stat_activity`) com cancelar, locks bloqueados/bloqueantes | implementado |
| M7 | Sequences | listar, next value | implementado (falta: reset value — `seq_set_val` já existe) |
| M7 | Extension Manager | instaladas + até 30 disponíveis, instalar/remover um clique | implementado |
| M7 | VACUUM/ANALYZE | — | **pendente** (não iniciado; `run_vacuum` já existe em `draco-core`) |
| M8 | ERD | diagrama de FKs (`cairo`+`GestureDrag`, sem precedente nos apps irmãos), arrastar tabela, pan do canvas | implementado (falta: zoom) |
| M8 | Busca Global | `Ctrl+P` entre tabelas/views/colunas/funções, clique abre o detalhe da tabela | implementado |
| M8 | Atalhos e Preferências | `Ctrl+P` (busca), `Ctrl+T` (nova query); tema já é automático via `libadwaita` | implementado (parcial — faltam os demais atalhos do README antigo: `Ctrl+Enter`/`F8` rodar, `Ctrl+Shift+S` snippet, etc.; sem tela de preferências/configurações persistidas ainda) |

## Nota de ambiente: dados do GtkSourceView5

Nesta máquina de desenvolvimento (openSUSE Leap 16), o pacote
`libgtksourceview-5-0` do repo oficial só traz os `.rng`/`.dtd` de esquema —
**sem** `language-specs/sql.lang` nem `styles/*.xml` (gtksourceview-4 tem
`sql.lang`, a v5 não, neste repo). O código em `query_editor.rs` já trata
`LanguageManager::language("sql")` e `StyleSchemeManager::scheme(...)`
retornando `None` sem quebrar — o editor cai para texto monoespaçado sem
highlighting. Testado que roda sem crash; **não foi possível confirmar
visualmente o highlighting funcionando** nesta máquina por causa dessa lacuna
de pacote, não do código. Vale revalidar numa distro com os dados completos
(Fedora, Tumbleweed) antes de considerar essa superfície "validada".

## Descobertas fora da matriz original

Ao portar `draco-core` (M1), a auditoria de `src/main/ipc.ts` (versão Electron, já
removida dos fontes) revelou superfícies que não estavam no README anunciado e
por isso não entraram na matriz acima: **pg_dump/pg_restore** via GUI,
**monitor de replicação**, **log de slow queries**, **schema diff** entre duas
conexões e uma tabela de **migrations** (`_draco_migrations`, conceito
Prisma-like ligado ao `schema.draco`). Nenhuma delas foi portada para
`draco-core` ainda — ficam pendentes de decisão de escopo (portar em um M9 ou
descartar deliberadamente, como o Windows) antes de implementar a UI
correspondente.

## Desvios aprovados

Suporte Windows (2026-07-27): removido do escopo por decisão de produto. O
Draco Electron publicava um instalador `.exe` (NSIS); nenhum dos apps do
ecossistema Lyra OS (Vega, Beam, Sulafat, Chord) sustenta build Windows hoje,
e GTK4/libadwaita via MSYS2 exigiria infraestrutura de CI/empacotamento sem
precedente pra copiar. O rewrite é só Linux, empacotado via OBS
(`home:rodrigosbrito:lyra/draco`) e AUR.

## Critérios transversais por módulo

- [ ] comportamento nominal contra um Postgres real;
- [ ] resposta a conexão ausente/perdida durante a operação;
- [ ] túnel SSH indisponível ou credencial inválida;
- [ ] loading, vazio, erro e recuperação;
- [ ] operação longa não bloqueia a thread GTK (roda via
      `runtime_handle.spawn` + `glib::MainContext::spawn_local`, nunca
      bloqueando o main loop);
- [ ] mutação perigosa (DROP, DELETE, ALTER destrutivo) possui confirmação
      inequívoca;
- [ ] nenhuma senha, passphrase ou conteúdo de query é escrito em log
      (`tracing`, nível `DRACO_LOG`).
