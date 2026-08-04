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
| M1 | Núcleo de dados (`draco-core`) | pool Postgres (`tokio-postgres`+`deadpool`, TLS via `rustls`), túnel SSH (`russh`, com jump host e known_hosts TOFU), introspecção/DDL/dashboard/stats/roles/jobs/activity/locks/extensions (`postgres::queries`), storage local (TOML/XDG), segredos (`keyring`, com Secret Service no Linux), parser `schema.draco` | implementado |
| M2 | Conexões | criar/editar, testar obrigatoriamente antes de salvar, status inline, favoritos, excluir com confirmação e remoção da senha no Secret Service, túnel SSH e jump host | implementado (falta: validar SSH/jump host no Tauri contra endpoint real) |
| M2 | Explorer | Tauri: árvore conexão → extensões e schema → tabela/view/função/procedure/sequence/trigger, expansão lazy, filtro inline e estimativa de linhas; tabelas/views e triggers navegam ao detalhe de tabela, enquanto funções/procedures/sequences abrem uma aba SQL com template e identificadores escapados; GTK: árvore equivalente | implementado |
| M3 | Editor SQL | Tauri: textarea local com syntax highlighting incremental por statement (overlay `<pre>` sincronizado, tokenizer próprio em `sql-highlight.js`, sem CodeMirror/bundler — decisão do ADR-0002), autocomplete baseado no schema (`completion_data`/`sql-autocomplete.js`: schemas, tabelas, colunas e funções via `get_completion_data`, com escopo por `tabela.coluna` quando qualificado), seletor de conexão, seleção ou buffer completo, execução e cancelamento por operação, abas múltiplas numeradas, atalhos `F8`/`Ctrl+Enter` (rodar), `F10` (`EXPLAIN` puro em JSON, nunca `ANALYZE`) e `Ctrl+Shift+S` (snippet); GTK: GtkSourceView5 | implementado |
| M3 | Resultados | Tauri: grid com virtualização real por janela de scroll (`virtual-list.js` + duas linhas-espaçadoras, altura fixa por linha — não há mais corte em 500 linhas), colunas redimensionáveis por ponteiro ou teclado, detalhe de linha acessível, cópia TSV e export CSV/JSON testados com delimitadores, quebras de linha, `NULL` e valores estruturados; GTK: `ColumnView` | implementado |
| M3 | Histórico e Snippets | histórico de queries (máx. 50) e snippets nomeados vinculados à conexão; Tauri permite salvar, carregar, renomear e excluir com confirmação | implementado |
| M4 | DDL Viewer | DDL completo somente leitura (GtkSourceView5 read-only) | implementado |
| M4 | Índices e Constraints | listagem, tamanho, tipo, definição | implementado |
| M4 | Mapa de FKs | FKs de saída/entrada; no Tauri, cada relação navega para a tabela da outra ponta | implementado |
| M4 | Detalhe de Tabela | Tauri: colunas (tipo completo, default, PK/FK), constraints, índices, mapa de FK navegável, DDL completo, estatísticas de coluna e browse paginado; insert aceita objeto JSON preservando defaults, enquanto update/delete usam obrigatoriamente a PK completa e preservam precisão de `bigint`/`numeric` no IPC; tabelas sem PK ficam read-only para update/delete; GTK mantém as abas equivalentes | implementado (falta: smoke visual do browse/edit contra PostgreSQL real) |
| M5 | Dashboard de Conexão | gauges custom (`cairo`) para cache hit/uso de conexões/rollback rate, info de servidor/banco, KPIs, top 10 tabelas com barra proporcional; o Tauri seleciona e abre automaticamente o dashboard após conectar | implementado |
| M5 | Stats do Banco | bloat de tabela, índices não usados, hot spots de seq scan — dobrado na mesma aba do dashboard | implementado |
| M6 | Criador de Tabela | Tauri e GTK: formulário visual, colunas dinâmicas (tipo/null/PK/unique/default) e preview de `CREATE TABLE`; o Tauri também aceita FK inline com ação `ON DELETE` allowlisted e cria schemas pelo Explorer | implementado (falta: smoke visual Tauri contra PostgreSQL real) |
| M6 | Editor de Tabela (`ALTER`) | Tauri e GTK: renomear tabela, adicionar/remover/renomear coluna, mudar tipo/nullable/default, preview do SQL e aplicação atômica (`BEGIN`/`COMMIT`); o backend Tauri reconstrói o diff a partir da introspecção atual, exige todas as colunas originais e suporta substituir/remover/adicionar PRIMARY KEY; `DROP COLUMN`, mudança de tipo ou PK exigem confirmação com o SQL final | implementado (geração coberta por testes; falta validar a execução completa contra PostgreSQL real) |
| M6 | Funções, procedures e triggers | Tauri: cria função/procedure por template local, carrega overload por argumentos de identidade, valida função em `BEGIN`/`ROLLBACK`, salva `CREATE [OR REPLACE]` e cria/edita triggers; GTK: editor de função GtkSourceView5 e criador de trigger | implementado (falta smoke visual Tauri contra PostgreSQL real) |
| M7 | Roles | Tauri e GTK: listar, criar e editar atributos/validade sem transportar senha, excluir e exibir atributos (superuser/createdb/createrole/login); Tauri bloqueia `pg_*`, confirma alterações e exige digitar o nome para excluir | implementado (backend e fronteira `draco-app` validados contra PostgreSQL 18.4; falta smoke visual da webview Tauri para o fluxo de edição) |
| M7 | Jobs (pg_cron) | Tauri e GTK detectam extensão ausente, listam, criam/editam, exibem até 50 execuções, pausam/retomam e excluem com confirmação | implementado (falta smoke visual do fluxo Tauri contra uma instalação com `pg_cron`) |
| M7 | Activity & Locks | sessões (`pg_stat_activity`) com cancelamento confirmado de query ativa via `pg_cancel_backend`, preservando a sessão; locks bloqueados/bloqueantes e ação para localizar visualmente o PID bloqueador | implementado |
| M7 | Sequences | Tauri: listar, criar pelo Explorer, abrir template SQL, avançar com confirmação e resetar valor após validação `i64` e confirmação destrutiva; GTK: criar, listar e next/reset value | implementado (falta smoke visual do fluxo Tauri contra PostgreSQL real) |
| M7 | Extension Manager | Tauri e GTK mostram instaladas + até 30 disponíveis e permitem instalar/remover; Tauri confirma instalação, exige nome exato para remoção e protege `plpgsql` | implementado (mutações dependem das permissões PostgreSQL da conexão) |
| M7 | Query Stats (pg_stat_statements) | Tauri e GTK: calls, tempo médio/total, rows, top 30, reset confirmado, detecção da extensão, abertura da SQL em nova aba e envio da consulta com métricas ao Assistente; Tauri também ordena por total/calls/média | implementado (backend e fronteira Tauri validados contra PostgreSQL 18.4; envio ao Assistente coberto pelo contrato local, sem consumir API externa no E2E) |
| M7 | VACUUM/ANALYZE | Tauri e GTK: VACUUM, ANALYZE, VACUUM ANALYZE e VACUUM FULL por tabela; FULL exige confirmação nominal e alerta de lock exclusivo | implementado (backend `ANALYZE` validado contra PostgreSQL 18.4; não há manutenção em nível de banco/schema) |
| M8 | ERD | Tauri: diagrama de FKs com pan, zoom, reset, seleção e navegação para detalhe; GTK: `cairo`+`GestureDrag` com pan | implementado |
| M8 | Busca Global | `Ctrl+P` entre tabelas/views/colunas/funções, clique abre o detalhe da tabela | implementado |
| M8 | Atalhos e Preferências | Tauri: `Ctrl+P` e `Ctrl+K` (palette/busca global), `Ctrl+T` (nova query), `F8`/`Ctrl+Enter` (rodar), `F10` (EXPLAIN) e `Ctrl+Shift+S` (snippet); GTK mantém `Ctrl+P`, `Ctrl+T`, `F8` e `F10` | implementado |
| M8 | Preferências, atualizações e about | Tauri: tela de Preferências com tema claro/escuro e 5 cores de destaque (persistidos em `AppSettings`), checagem manual/automática de atualização contra o release mais recente do GitHub (`draco-core::updates`, somente leitura, nunca instala) e aba About com licença, repositório e doação via Pix (SVG estático commitado, chave/copia-e-cola conferidos por teste contra `qrcode` como devDependency — sem chamada de rede em runtime) | implementado (falta no GTK: tela equivalente — hoje o tema GTK segue só o `libadwaita` do sistema) |
| M10 | Assistente de IA | capacidade nova (não existia na versão Electron), inspirada no `vega-gtk::assistant`: aba por conexão (botão na linha do host, ao lado de Dashboard/Admin), Anthropic/OpenAI/Gemini, chave só no credential store (`keyring`, Secret Service no Linux; nunca `secret-tool` shell-out como no Vega), acesso **somente leitura** ao banco — `list_schemas`/`list_tables`/`describe_table`/`explain_query`/`run_select`/`get_performance_health` (`draco-core::assistant`), sem nenhuma ferramenta de escrita: índices/rewrites sugeridos são só texto para o usuário rodar manualmente no Editor SQL | implementado (falta: validar contra Postgres real e contra as três APIs de fato; sem anexos de arquivo/imagem, ao contrário do Vega — fora de escopo por ora; limite diário usa dia UTC, não o fuso local) |
| M11 | CI e distribuição Linux | contratos frontend/componentes/estados visuais e metadados; lint/testes Rust; mock smoke da bridge Tauri; build do binário oficial; validação `.desktop`/AppStream; smoke de layout instalado e bibliotecas; AUR com SHA-256; OBS vendorizado/offline; dependências Tauri sem GTK4/libadwaita/GtkSourceView5; fallback GTK compilável | em desenvolvimento (automação local implementada; a tag/pacotes `v2.0.3`, smoke Wayland/X11, leitor de tela, upgrade e três ciclos estáveis ainda são gates externos; `packaging/RELEASE_PENDING.md`) |

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
- [x] resposta a conexão ausente/perdida durante a operação (operação recusada desconectada e
      reconexão validada pela fronteira `draco-app` em 04/08/2026);
- [ ] túnel SSH indisponível ou credencial inválida;
- [x] loading, vazio, erro e recuperação (contratos frontend e E2E PostgreSQL real);
- [x] operação longa não bloqueia a thread GTK (roda via
      `runtime_handle.spawn` + `glib::MainContext::spawn_local`, nunca
      bloqueando o main loop);
- [x] mutação perigosa (DROP, DELETE, ALTER destrutivo) possui confirmação
      inequívoca;
- [x] nenhuma senha, passphrase ou conteúdo de query é escrito em log
      (`tracing`, nível `DRACO_LOG`).

O Editor de Tabela (M6) foi o primeiro módulo a implementar o critério de
"confirmação inequívoca" acima, via `adw::AlertDialog` antes de `DROP COLUMN`/
`ALTER COLUMN TYPE`. O mesmo padrão (`confirm_destructive` em `admin.rs`) foi
retrofitado em seguida para drop de role, drop de extension e delete de job —
única mutação destrutiva ainda sem esse diálogo: `drop_table` (Explorer/M4,
sem UI de exclusão de tabela ainda).
