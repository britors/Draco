# Draco

Cliente de banco de dados (explorador de esquemas, editor SQL, administração)
para PostgreSQL, app do ecossistema **Lyra OS**.
GitHub: https://github.com/britors/Draco

## Stack

- **Linguagem**: Rust
- **Plataforma**: desktop Linux, Tauri 2 + WebKitGTK 4.1; GTK4 legado em transição
- **Driver**: `tokio-postgres` — sem dependência de CLI externo (`psql`)
- **Túnel SSH**: `russh` (em processo, sem shell out pro binário `ssh`)
- **Editor SQL**: frontend local HTML/CSS/JavaScript (sem framework/CDN)
- **Segredos**: `oo7` (Secret Service / GNOME Keyring) — nunca texto plano
- **Build**: `cargo` (workspace)

O build oficial é `cargo build --locked --release -p draco-tauri`; use
`cargo check -p draco-gtk` somente para verificar o fallback de transição.

Reescrito de Electron/TypeScript para Rust/Tauri 2 seguindo o mesmo padrão de
separação de núcleo e frontend dos outros apps do ecossistema.

## Arquitetura

Workspace Cargo com núcleo, aplicação e dois frontends:
- `draco-core` — pool Postgres, túnel SSH, queries de introspecção/DDL/stats,
  storage local (TOML/XDG via `directories`) e segredos. **Sem dependência de
  nenhum toolkit gráfico** — deve compilar e ser testável isoladamente.
- `draco-app` — casos de uso e DTOs sem toolkit.
- `src-tauri` — shell oficial Tauri 2, binário `draco`.
- `draco-gtk` — frontend GTK4/libadwaita de fallback, binário `draco-gtk`.

Bridging async → GTK (ver `draco-gtk/src/main.rs`): um runtime `tokio`
multi-thread roda numa thread própria (`draco-tokio`); o loop GTK fica na
main thread. Trabalho que precisa do reactor tokio (`tokio-postgres`, `russh`)
é disparado com `runtime_handle.spawn(...)` e o `JoinHandle` é aguardado
dentro de `glib::MainContext::default().spawn_local(...)` na thread GTK —
nunca se bloqueia uma thread na outra.

## Regras do frontend oficial — obrigatórias

A UI deve parecer **dark-first, local e acessível**:
- assets, fontes e scripts somente locais; nenhum CDN ou recurso remoto;
- estados de loading, vazio, erro e confirmação destrutiva explícitos;
- dados inseridos pelo usuário sempre via `textContent`, nunca HTML não confiável;
- IPC somente por comandos registrados no `src-tauri`;
- segredos nunca entram no DOM, storage do navegador ou logs.

O frontend GTK segue as regras nativas abaixo somente enquanto existir como fallback.

## Segredos e logs

- Senhas de conexão, SSH e jump host: `oo7` (Secret Service). Nunca gravar em
  disco em texto plano nem logar.
- O fallback GTK usa `tracing` com nível controlado por `DRACO_LOG`; o shell
  Tauri deve ser depurado pelo stderr do processo. Conteúdo de query e
  credenciais nunca aparecem em log.

## Progresso da reescrita

A matriz de paridade funcional (o que já foi portado da versão Electron, o
que falta) fica em
[`docs/migration/rust-gtk-parity.md`](docs/migration/rust-gtk-parity.md).
Atualize a linha do módulo correspondente ao terminar de portar uma
superfície.

## Build e escopo confirmado

- Só Linux (Tauri/WebKitGTK via OBS + AUR) — sem build Windows. Nenhum app do
  ecossistema Lyra OS sustenta Windows hoje.
- O binário oficial é `target/release/draco`; o GTK permanece compilável até o
  checklist de [`docs/migration/tauri-stabilization.md`](docs/migration/tauri-stabilization.md).
- v1 mira paridade ampla com a versão Electron anterior (dashboard, ERD,
  editores de tabela/função, roles, pg_cron, activity/locks, stats — ver
  matriz de paridade), não um MVP deliberadamente reduzido.
