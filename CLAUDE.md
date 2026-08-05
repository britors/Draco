# Draco

Cliente desktop PostgreSQL do ecossistema **Lyra OS**. O único frontend e
artefato oficial é o app **Tauri 2**.

- Repositório: <https://github.com/britors/Draco>
- Identificador desktop: `org.lyraos.Draco`
- Versão do workspace: `2.1.0`
- Licença: GPL-3.0-or-later
- Binário oficial: `target/release/draco`

## Stack e restrições de produto

- Rust 2021, Tauri 2 + WebKitGTK 4.1 no shell oficial.
- `tokio-postgres` + `deadpool-postgres` para conexões e pool (máximo de 5
  conexões por driver); TLS via `rustls`.
- Túnel SSH e jump host em processo via `russh`; não chamar o executável `ssh`.
- Frontend oficial em HTML/CSS/JavaScript local, sem framework, bundler, CDN ou
  dependência de rede em runtime. Os fontes servidos pelo Tauri estão
  diretamente em `frontend/dist`.
- Credenciais via crate `keyring`: Secret Service (GNOME Keyring/KWallet) no
  Linux. Nunca persistir senha, passphrase ou chave de IA nos arquivos TOML,
  DOM, storage do navegador ou logs.
- Queries normais não dependem de `psql`. Backup/restauração são a exceção
  deliberada e usam `pg_dump`, `pg_restore` ou `psql` por meio de
  `draco-core::postgres::backup`.
- A distribuição oficial cobre Windows NSIS, DEB Ubuntu, RPM Fedora/openSUSE e
  RPM openSUSE via OBS.

## Arquitetura e dependências entre camadas

O workspace possui três crates:

- `draco-core`: motor independente de GUI. Contém conexões, pool, TLS, SSH,
  introspecção/DDL/administração PostgreSQL, backup/restauração, Assistente de
  IA, parser `schema.draco`, persistência XDG/TOML e credenciais.
- `draco-app`: camada de casos de uso usada pelo Tauri. É dona do
  `ConnectionManager`, do registro de operações canceláveis e dos DTOs
  serializáveis. Credenciais só entram como argumentos transitórios e nunca
  fazem parte dos DTOs.
- `src-tauri` (`draco-tauri`): bridge IPC fina e shell oficial. Comandos devem
  delegar para `draco-app`; não expor `PostgresDriver` ou acesso genérico a
  filesystem/processo.

Fluxo oficial:

```text
frontend/dist -> comandos Tauri -> draco-app -> draco-core -> PostgreSQL/SSH/XDG/keyring
```

## Estado funcional atual do Tauri

A bridge registrada em `src-tauri/src/main.rs` oferece:

- criação, teste obrigatório, edição, favoritos, conexão/desconexão e exclusão
  de conexões, incluindo SSH/jump host;
- Explorer lazy de schemas, tabelas, views, funções, procedures, sequences e
  triggers, mais busca global;
- Editor SQL com abas, seleção/buffer completo, execução de query ou script,
  `EXPLAIN` sem `ANALYZE`, cancelamento por `operationId`, highlighting local e
  autocomplete obtido do schema; a modal `Review with AI` envia a seleção ou o
  buffer atual para revisão de segurança, performance e legibilidade;
- grid de resultados virtualizado, detalhe de linha, cópia TSV e exportação
  CSV/JSON;
- histórico (máximo 50) e snippets por conexão;
- Dashboard, estatísticas, detalhe de tabela (colunas, constraints, índices,
  FKs, DDL e estatísticas de coluna) e ERD navegável;
- Activity/Locks, roles, `pg_cron`, extensões, `pg_stat_statements` e manutenção
  allowlisted (`VACUUM`/`ANALYZE`);
- backup/restauração canceláveis;
- preferências de tema/destaque, About/Pix e checagem somente leitura da release
  mais recente no GitHub;
- Assistente de IA por conexão com Anthropic, OpenAI ou Gemini.

A UI Tauri já cobre browse/edit paginado por PK, criação/alteração visual de
tabelas (incluindo FK inline e troca de PK), criação de schema/sequence/trigger,
uma área principal Programming com editor SQL dedicado (sem modal) para
views/funções/procedures/triggers, GitHub nativo com branches/diffs/commit/PR,
edição validada desses objetos e de sequences, recriação transacional de índices comuns, edição
de roles e jobs e reset de sequences. Os DTOs nunca aceitam um lote de `ALTER TABLE` pronto: o serviço
reconsulta a estrutura atual, valida os campos e reconstrói o diff no Rust.
Antes de ampliar uma superfície, conferir o contrato em
`docs/architecture/tauri-application-contract.md` e os comandos registrados em
`src-tauri/src/main.rs`.

## Invariantes de segurança

- Arquivos XDG/TOML armazenam somente metadados, preferências, histórico,
  snippets, configurações/histórico do Assistente e seu contador de uso.
- Senhas PostgreSQL, SSH/jump host e chaves de API ficam no credential store sob
  os serviços `draco` e `draco-ai`. Chamadas síncronas do crate `keyring` devem
  continuar dentro de `tokio::task::spawn_blocking`.
- `draco-core::legacy_secrets` é a ponte Linux de uso único para entradas antigas
  do `oo7`: copia, relê para verificar e só então remove o item legado. Não
  remover essa compatibilidade antes do período de upgrade da linha `2.x`.
- SQL, resultados, credenciais e passphrases não devem aparecer em logs ou
  envelopes de erro. Logs de ferramentas externas devem redigir caminhos
  escolhidos pelo usuário.
- Conteúdo fornecido pelo usuário vai ao DOM com `textContent`/DOM APIs, nunca
  como HTML não confiável.
- A capability Tauri é restrita à janela `main`; a CSP permite apenas assets
  locais/data e IPC. Não adicionar plugin genérico de shell ou filesystem.
- Mutações destrutivas exigem confirmação explícita e, quando aplicável,
  confirmação nominal.
- O Assistente só pode inspecionar o banco. Suas ferramentas são
  `list_schemas`, `list_tables`, `describe_table`, `explain_query`,
  `run_select` (máximo 50 linhas) e `get_performance_health`. Não adicionar DDL
  ou DML ao tool loop; sugestões SQL devem ser texto para revisão manual.
- `EXPLAIN` do editor e do Assistente nunca usa `ANALYZE`.

## Persistência e estado

`draco-core/src/store.rs` usa `directories::ProjectDirs` e arquivos TOML no
diretório de configuração XDG. Os arquivos atuais cobrem conexões, histórico,
snippets, preferências e estado do Assistente (`ai-settings.toml`,
`ai-history.toml`, `ai-usage.toml`). O histórico de IA é separado por ID de
conexão e o limite diário usa dias UTC.

`Application` mantém drivers vivos por ID de conexão e clona o handle do
driver antes de queries longas para não segurar o mutex do manager durante todo
o I/O. Queries, scripts, `EXPLAIN`, backup e restore podem registrar um
`operationId`; o registro deve ser removido ao concluir, falhar ou cancelar.
Cancelamento de query é direcionado ao PID do backend da operação, evitando
cancelar outra query concorrente da mesma conexão.

## Regras do frontend oficial

- Alterar primeiro os tokens em `frontend/dist/styles.css`; temas claro/escuro
  e as cinco cores de destaque dependem deles.
- Manter estados explícitos de loading, vazio, sucesso e erro, navegação por
  teclado e foco visível.
- O frontend usa o global Tauri (`window.__TAURI__`) e somente comandos
  registrados em `src-tauri/src/main.rs`.
- Não introduzir `fetch` no frontend. As únicas saídas de rede atuais ficam no
  backend: checagem de atualização e provedores do Assistente.
- Módulos testáveis isoladamente vivem em arquivos como `sql-highlight.js`,
  `sql-autocomplete.js`, `virtual-list.js` e `result-export.js`.

## Build, lint e testes

Build oficial:

```sh
cargo build --locked --release -p draco-tauri
./target/release/draco
```

Validação equivalente à CI:

```sh
(cd frontend && npm ci --ignore-scripts && npm run check && npm test)
cargo fmt --check -p draco-app -p draco-tauri
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked -p draco-tauri
desktop-file-validate data/org.lyraos.Draco.desktop
appstreamcli validate --no-net data/org.lyraos.Draco.metainfo.xml
```

O teste contra PostgreSQL real é ignorado por padrão e busca a senha no
credential store a partir de `DRACO_TEST_CONN_ID`:

```sh
./scripts/test-live-postgres.sh
```

Ele aceita também `DRACO_TEST_HOST`, `DRACO_TEST_DB` e `DRACO_TEST_USER`; nunca
colocar a senha no ambiente. A última execução documentada passou contra
PostgreSQL 18.4, mas ainda não cobre webview instalada, SSH/jump host real nem
as três APIs de IA.

O workspace, o RPM/OBS e o AppStream estão em `2.1.0`. Releases devem usar uma
tag imutável; nunca reutilizar uma tag nem gerar o tarball de um branch mutável.

## Documentação que acompanha mudanças

- Contrato `draco-app`/IPC: `docs/architecture/tauri-application-contract.md`.
- Ameaças e invariantes: `docs/security/threat-model.md`.
- Design system: `docs/design/frontend-design-system.md`.
- Desenvolvimento e distribuição Linux: `docs/development/tauri.md`.
- Teste real: `docs/testing/live-postgres.md`.

Ao concluir uma superfície, atualizar a documentação do contrato
correspondente. O código e os comandos registrados são a fonte de verdade
quando um documento estiver defasado.
