# Draco

Cliente de banco de dados (explorador de esquemas, editor SQL, administração)
para PostgreSQL, app do ecossistema **Lyra OS**.
GitHub: https://github.com/britors/Draco

## Stack

- **Linguagem**: Rust
- **Plataforma**: desktop Linux nativo, GTK4 + libadwaita
- **Driver**: `tokio-postgres` — sem dependência de CLI externo (`psql`)
- **Túnel SSH**: `russh` (em processo, sem shell out pro binário `ssh`)
- **Editor SQL**: GtkSourceView5 (sem Monaco/CDN)
- **Segredos**: `oo7` (Secret Service / GNOME Keyring) — nunca texto plano
- **Build**: `cargo` (workspace)

Reescrito de Electron/TypeScript para Rust/GTK4 seguindo o mesmo padrão dos
outros apps do ecossistema (Vega, Beam, Sulafat, Chord).

## Arquitetura

Workspace Cargo com dois crates:
- `draco-core` — pool Postgres, túnel SSH, queries de introspecção/DDL/stats,
  storage local (TOML/XDG via `directories`) e segredos. **Sem dependência de
  nenhum toolkit gráfico** — deve compilar e ser testável isoladamente.
- `draco-gtk` — frontend GTK4/libadwaita, binário `draco`.

Bridging async → GTK (ver `draco-gtk/src/main.rs`): um runtime `tokio`
multi-thread roda numa thread própria (`draco-tokio`); o loop GTK fica na
main thread. Trabalho que precisa do reactor tokio (`tokio-postgres`, `russh`)
é disparado com `runtime_handle.spawn(...)` e o `JoinHandle` é aguardado
dentro de `glib::MainContext::default().spawn_local(...)` na thread GTK —
nunca se bloqueia uma thread na outra.

## Regras de UI — obrigatórias

A UI deve parecer **nativa do GNOME/Lyra**:
- **Somente** widgets GTK4/libadwaita — tema `libadwaita` ativo (claro/escuro
  automático), sem CSS custom hardcoded fora do necessário
- **Sem bibliotecas de UI externas** (Electron, web frameworks, CSS frameworks)
- Ícones simbólicos do tema do sistema (`-symbolic`)
- Editor SQL: `GtkSourceView5` (highlighting + completion nativos)
- Grids/listas: `gtk::ColumnView` + `gio::ListStore` (virtualizado)
- Visualizações custom (gauges do dashboard, ERD): `gtk::DrawingArea` + `cairo`,
  seguindo o precedente de `vega-gtk/src/ui/sparkline.rs`

## Segredos e logs

- Senhas de conexão, SSH e jump host: `oo7` (Secret Service). Nunca gravar em
  disco em texto plano nem logar.
- Log via `tracing`, nível controlado por `DRACO_LOG`. Conteúdo de query e
  credenciais nunca aparecem em log.

## Progresso da reescrita

A matriz de paridade funcional (o que já foi portado da versão Electron, o
que falta) fica em
[`docs/migration/rust-gtk-parity.md`](docs/migration/rust-gtk-parity.md).
Atualize a linha do módulo correspondente ao terminar de portar uma
superfície.

## Escopo confirmado

- Só Linux (GTK4/libadwaita via OBS + AUR) — sem build Windows. Nenhum app do
  ecossistema Lyra OS sustenta Windows hoje.
- v1 mira paridade ampla com a versão Electron anterior (dashboard, ERD,
  editores de tabela/função, roles, pg_cron, activity/locks, stats — ver
  matriz de paridade), não um MVP deliberadamente reduzido.
