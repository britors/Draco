# Matriz de paridade Tauri + GTK4 de fallback

Lista de aceite funcional usada na migração da antiga UI Electron/TypeScript
para o frontend local Tauri 2. O `draco-gtk` é mantido como fallback compilável
durante a estabilização; a decisão e o checklist para removê-lo estão em
[`tauri-stabilization.md`](tauri-stabilization.md).

Estados permitidos: `pendente`, `em desenvolvimento`, `implementado`,
`validado` e `desvio aprovado`. `Implementado` indica que a superfície foi
entregue e compila/roda; `validado` exige também passar pelo checklist
transversal abaixo contra um Postgres real. Um desvio precisa apontar para a
decisão que o aprovou.

| Milestone | Módulo | Superfície obrigatória | Estado |
|---|---|---|---|
| M1 | Núcleo de dados (`draco-core`) | pool Postgres (`tokio-postgres`+`deadpool`, TLS via `rustls`), túnel SSH (`russh`, com jump host e known_hosts TOFU), introspecção/DDL/dashboard/stats/roles/jobs/activity/locks/extensions (`postgres::queries`), storage local (TOML/XDG), segredos (`oo7`), parser `schema.draco` | implementado |
| M2 | Conexões | criar/editar, testar obrigatoriamente antes de salvar, status inline, favoritos, excluir com confirmação e remoção da senha no Secret Service, túnel SSH e jump host | implementado (falta: validar SSH/jump host no Tauri contra endpoint real) |
| M2 | Explorer | Tauri: árvore conexão → schema → tabela/view/função/procedure/sequence/trigger, expansão lazy, filtro inline e estimativa de linhas; GTK: árvore equivalente | implementado (falta no Tauri: extensões no próprio Explorer e navegação ao clicar em objetos que não são tabelas) |
| M3 | Editor SQL | Tauri: textarea local com syntax highlighting (overlay `<pre>` sincronizado, tokenizer próprio em `sql-highlight.js`, sem CodeMirror/bundler — decisão do ADR-0002), autocomplete baseado no schema (`completion_data`/`sql-autocomplete.js`: schemas, tabelas, colunas e funções via `get_completion_data`, com escopo por `tabela.coluna` quando qualificado), seletor de conexão, seleção ou buffer completo, execução e cancelamento por operação, abas múltiplas numeradas, atalhos `F8`/`Ctrl+Enter` (rodar), `F10` (`EXPLAIN` puro em JSON, nunca `ANALYZE`) e `Ctrl+Shift+S` (snippet); GTK: GtkSourceView5 | implementado (falta no Tauri: highlighting incremental por statement dentro de scripts com múltiplas queries) |
| M3 | Resultados | Tauri: grid com virtualização real por janela de scroll (`virtual-list.js` + duas linhas-espaçadoras, altura fixa por linha — não há mais corte em 500 linhas), detalhe de linha acessível, cópia TSV e export CSV/JSON testados com delimitadores, quebras de linha, `NULL` e valores estruturados; GTK: `ColumnView` | implementado |
| M3 | Histórico e Snippets | histórico de queries (máx. 50) e snippets nomeados vinculados à conexão; Tauri permite salvar, carregar, renomear e excluir com confirmação | implementado |
| M4 | DDL Viewer | DDL completo somente leitura (GtkSourceView5 read-only) | implementado |
| M4 | Índices e Constraints | listagem, tamanho, tipo, definição | implementado |
| M4 | Mapa de FKs | FKs de saída/entrada | implementado (falta: clicar para navegar até a tabela referenciada) |
| M4 | Detalhe de Tabela | Tauri: colunas (tipo completo, default, PK/FK), constraints, índices, mapa de FK, DDL completo e estatísticas de coluna; GTK mantém as abas equivalentes | implementado (falta no Tauri: browse/edit de dados e navegação pelas FKs) |
| M5 | Dashboard de Conexão | gauges custom (`cairo`) para cache hit/uso de conexões/rollback rate, info de servidor/banco, KPIs, top 10 tabelas com barra proporcional | implementado (falta: abrir automaticamente ao conectar) |
| M5 | Stats do Banco | bloat de tabela, índices não usados, hot spots de seq scan — dobrado na mesma aba do dashboard | implementado |
| M6 | Criador de Tabela | formulário visual, colunas dinâmicas (tipo/null/PK/unique/default), preview de `CREATE TABLE` (botão "Refresh Preview", não totalmente live) | implementado (falta: FK inline, "nova schema" — hoje só cria na schema já aberta) |
| M6 | Editor de Tabela (`ALTER`) | renomear tabela, adicionar/remover/renomear coluna, mudar tipo/nullable/default (`table_editor.rs`, aberto pelo botão "Edit" no Detalhe de Tabela), preview do SQL, aplicação atômica (`BEGIN`/`COMMIT`), confirmação obrigatória (`adw::AlertDialog`) para `DROP COLUMN`/`ALTER COLUMN TYPE` | implementado (compila e passa `cargo clippy`/`cargo test`; falta: mudar PRIMARY KEY, e validar o fluxo completo contra um Postgres real — a geração de SQL tem testes unitários, mas a execução em si (`alter_table`/`batch_execute`) ainda não rodou contra um banco) |
| M6 | Editor de Função | GtkSourceView5, Save, Validate (BEGIN/ROLLBACK), Test | implementado |
| M7 | Roles | Tauri e GTK: listar, criar sem transportar senha, excluir e exibir atributos (superuser/createdb/createrole/login); Tauri bloqueia `pg_*` e exige digitar o nome para excluir | implementado (backend e fronteira `draco-app` validados contra PostgreSQL 18.4; falta smoke visual da webview Tauri e edição de role existente) |
| M7 | Jobs (pg_cron) | Tauri e GTK detectam extensão ausente, listam, pausam/retomam e excluem com confirmação | implementado (falta no Tauri: criar/editar job e histórico de execução — `create_job`/`update_job`/`get_job_runs` já existem) |
| M7 | Activity & Locks | sessões (`pg_stat_activity`) com cancelamento confirmado de query ativa via `pg_cancel_backend`, preservando a sessão; locks bloqueados/bloqueantes | implementado (falta ação assistida para navegar do lock ao PID bloqueador) |
| M7 | Sequences | listar, next value | implementado (falta: reset value — `seq_set_val` já existe) |
| M7 | Extension Manager | Tauri e GTK mostram instaladas + até 30 disponíveis e permitem instalar/remover; Tauri confirma instalação, exige nome exato para remoção e protege `plpgsql` | implementado (mutações dependem das permissões PostgreSQL da conexão) |
| M7 | Query Stats (pg_stat_statements) | Tauri e GTK: calls, tempo médio/total, rows, top 30, reset confirmado, detecção da extensão, abertura da SQL em nova aba e envio da consulta com métricas ao Assistente; Tauri também ordena por total/calls/média | implementado (backend e fronteira Tauri validados contra PostgreSQL 18.4; envio ao Assistente coberto pelo contrato local, sem consumir API externa no E2E) |
| M7 | VACUUM/ANALYZE | Tauri e GTK: VACUUM, ANALYZE, VACUUM ANALYZE e VACUUM FULL por tabela; FULL exige confirmação nominal e alerta de lock exclusivo | implementado (backend `ANALYZE` validado contra PostgreSQL 18.4; não há manutenção em nível de banco/schema) |
| M8 | ERD | Tauri: diagrama de FKs com pan, zoom, reset, seleção e navegação para detalhe; GTK: `cairo`+`GestureDrag` com pan | implementado |
| M8 | Busca Global | `Ctrl+P` entre tabelas/views/colunas/funções, clique abre o detalhe da tabela | implementado |
| M8 | Atalhos e Preferências | Tauri: `Ctrl+K` (palette), `Ctrl+T` (nova query), `F8`/`Ctrl+Enter` (rodar), `F10` (EXPLAIN) e `Ctrl+Shift+S` (snippet); GTK mantém `Ctrl+P`, `Ctrl+T`, `F8` e `F10` | implementado (parcial — falta alinhar o atalho da busca entre os frontends) |
| M8 | Preferências, atualizações e about | Tauri: tela de Preferências com tema claro/escuro e 5 cores de destaque (persistidos em `AppSettings`), checagem manual/automática de atualização contra o release mais recente do GitHub (`draco-core::updates`, somente leitura, nunca instala) e aba About com licença, repositório e doação via Pix (SVG estático commitado, chave/copia-e-cola conferidos por teste contra `qrcode` como devDependency — sem chamada de rede em runtime) | implementado (falta no GTK: tela equivalente — hoje o tema GTK segue só o `libadwaita` do sistema) |
| M10 | Assistente de IA | capacidade nova (não existia na versão Electron), inspirada no `vega-gtk::assistant`: aba por conexão (botão na linha do host, ao lado de Dashboard/Admin), Anthropic/OpenAI/Gemini, chave só no Secret Service (`oo7`, nunca `secret-tool` shell-out como no Vega), acesso **somente leitura** ao banco — `list_schemas`/`list_tables`/`describe_table`/`explain_query`/`run_select`/`get_performance_health` (`draco-core::assistant`), sem nenhuma ferramenta de escrita: índices/rewrites sugeridos são só texto para o usuário rodar manualmente no Editor SQL | implementado (falta: validar contra Postgres real e contra as três APIs de fato; sem anexos de arquivo/imagem, ao contrário do Vega — fora de escopo por ora; limite diário usa dia UTC, não o fuso local) |

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
(`home:rodrigosbrito:lyra/postgres-draco`) e AUR (`postgres-draco`) — nome de
pacote diferente do app porque "draco" simples já é usado pelo projeto
"graphics" do openSUSE e pelo `extra/draco` oficial do Arch.

## Critérios transversais por módulo

- [x] comportamento nominal do backend e de `draco-app` contra PostgreSQL 18.4
      (`scripts/test-live-postgres.sh`, 04/08/2026); a webview Tauri continua pendente;
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

O Editor de Tabela (M6) foi o primeiro módulo a implementar o critério de
"confirmação inequívoca" acima, via `adw::AlertDialog` antes de `DROP COLUMN`/
`ALTER COLUMN TYPE`. O mesmo padrão (`confirm_destructive` em `admin.rs`) foi
retrofitado em seguida para drop de role, drop de extension e delete de job —
única mutação destrutiva ainda sem esse diálogo: `drop_table` (Explorer/M4,
sem UI de exclusão de tabela ainda).
