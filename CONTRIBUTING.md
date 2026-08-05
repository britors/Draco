# Contribuindo com o Draco

## Pré-requisitos

- Rust estável recente (`cargo`, `rustc` ≥ 1.85)
- WebKitGTK 4.1, GTK3, OpenSSL, librsvg e `xdg-desktop-portal` para o shell Tauri; GTK4,
  libadwaita e GtkSourceView5 apenas para o fallback `draco-gtk`
- Uma instância PostgreSQL rodando para teste manual

## Setup

```bash
git clone https://github.com/britors/Draco.git
cd Draco
cargo build --locked --release -p draco-tauri
```

## Fluxo de desenvolvimento

**Build oficial:**
```bash
cargo build --locked --release -p draco-tauri
```

**Build de transição GTK:**
```bash
cargo check -p draco-gtk
```

**Rodar:**
```bash
cargo run -p draco-tauri
```

**Lint e testes** (rodar antes de qualquer PR):
```bash
cargo clippy --locked --workspace --exclude draco-gtk --all-targets -- -D warnings
cargo test --locked --workspace --exclude draco-gtk
(cd frontend && npm run check && npm test)
```

O workflow `.github/workflows/ci.yml` executa os mesmos contratos em Ubuntu 24.04 com Rust estável,
além de compilar o binário Tauri oficial e o frontend GTK de rollback. Todos os passos de `cargo`
no CI usam `--locked`: qualquer PR que mude `version` em `Cargo.toml` ou adicione/atualize uma
dependência precisa rodar `cargo check --workspace` localmente antes de commitar, para que
`Cargo.lock` já saia sincronizado — senão o job falha com "cannot update the lock file". Testes que
dependem de PostgreSQL, Secret Service ou sessão gráfica continuam no checklist E2E documentado em
`docs/testing/live-postgres.md`. Os contratos de distribuição também validam versões, app id,
desktop entry, AppStream e dependências do RPM/OBS.

O workflow `.github/workflows/release.yml` roda somente para tags `vX.Y.Z` (ou
reexecução manual de uma tag), valida a versão dos manifests e publica NSIS
Windows, DEB Ubuntu, RPM Fedora e RPM openSUSE com checksums SHA-256. Não use uma
branch como entrada manual: o job exige uma tag existente apontando exatamente
para o commit compilado.

## Estrutura do repositório

```
draco-core/     # pool Postgres, túnel SSH, queries, storage local, segredos —
                # sem dependência de nenhum toolkit gráfico
draco-app/      # casos de uso e DTOs serializáveis, sem dependência de toolkit
src-tauri/      # shell Tauri 2, capabilities e bridge IPC tipada
frontend/       # frontend web local empacotado pelo Tauri
draco-gtk/      # frontend GTK4/libadwaita de fallback (binário `draco-gtk`)
data/           # .desktop, metainfo AppStream, ícones
packaging/obs/  # spec RPM para o OBS (home:rodrigosbrito:lyra/postgres-draco)
docs/migration/ # matriz de paridade e decisão de estabilização Tauri
```

## Arquitetura

- `draco-core` é agnóstico de UI: tudo que fala com o Postgres, com SSH ou com
  o disco vive lá, testável sem GTK.
- `draco-app` é a fronteira de aplicação para futuros frontends: a UI deve
  consumir seus DTOs e casos de uso, sem importar `draco-core` diretamente.
- `src-tauri` expõe somente comandos finos sobre `draco-app`; capabilities são
  explícitas e o frontend não recebe credenciais do Secret Service.
- `draco-gtk` roda o loop do GTK na thread principal e um runtime `tokio`
  dedicado numa thread própria (`draco-tokio`, ver `draco-gtk/src/main.rs`).
  Trabalho async é disparado com `runtime_handle.spawn(...)` e o
  `JoinHandle` é aguardado dentro de um
  `glib::MainContext::default().spawn_local(...)` na thread GTK — nunca se
  bloqueia o main loop nem a thread do tokio uma na outra.
- `frontend` usa assets locais; a CSP do Tauri bloqueia scripts, frames e
  recursos remotos.
- Segredos (senhas de conexão, senha de túnel SSH) passam pelo Serviço de
  Segredos do sistema via `keyring` — nunca são gravados em disco em texto plano
  nem logados.

## Regras do frontend oficial

- O Tauri usa somente assets locais e não adiciona CDN, framework web ou
  dependência npm de runtime.
- Toda tela precisa representar loading, vazio, erro e recuperação; ações
  destrutivas pedem confirmação explícita.
- Dados vindos de queries ou usuários são inseridos como texto, nunca como HTML
  confiável implicitamente.
- Segredos não entram no DOM, `localStorage`, `sessionStorage`, logs ou eventos.

## Regras do fallback GTK

A interface deve usar **somente** widgets GTK4/libadwaita nativos:

- Cores, espaçamento e tipografia seguem o tema `libadwaita` ativo (claro/escuro
  automático) — sem CSS custom hardcoded fora do necessário.
- Ícones via ícones simbólicos do tema do sistema (nome `-symbolic`).
- Editor SQL: `GtkSourceView5` (highlighting + completion nativos).
- Grids/listas: `gtk::ColumnView` + `gio::ListStore` (virtualizado).
- Visualizações custom (gauges do dashboard, ERD): `gtk::DrawingArea` + `cairo`.

## Migração Electron → Rust/GTK

O Draco era um app Electron/TypeScript; a reescrita atual segue o padrão dos
outros apps do ecossistema Lyra OS (Vega, Beam, Sulafat, Chord — workspace
`<nome>-core` + `<nome>-gtk`, app id `org.lyraos.<Nome>`). A matriz de
paridade funcional em
[`docs/migration/rust-gtk-parity.md`](docs/migration/rust-gtk-parity.md) é a
lista de aceite: ao portar um módulo, atualize o estado da linha
correspondente (`pendente` → `em desenvolvimento` → `implementado` →
`validado`).

## Pull requests

- Um PR por módulo/milestone da matriz de paridade, quando possível.
- Rode `cargo clippy --workspace` e `cargo test -p draco-core` antes de abrir
  o PR — ambos devem passar sem erros.
- Ao completar uma superfície da matriz de paridade, atualize
  `docs/migration/rust-gtk-parity.md` no mesmo PR.

## Estilo de commit

```
feat: descrição curta

Corpo opcional explicando o porquê.
```

Tipos: `feat`, `fix`, `chore`, `refactor`, `docs`.
