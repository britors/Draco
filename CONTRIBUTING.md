# Contribuindo com o Draco

## Pré-requisitos

- Rust estável recente (`cargo`, `rustc` ≥ 1.85)
- WebKitGTK 4.1, GTK3, OpenSSL, librsvg e `xdg-desktop-portal` para o shell Tauri
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

**Rodar:**
```bash
cargo run -p draco-tauri
```

**Lint e testes** (rodar antes de qualquer PR):
```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
(cd frontend && npm run check && npm test)
```

O workflow `.github/workflows/ci.yml` executa os mesmos contratos em Ubuntu 24.04 com Rust estável
e compila o binário Tauri oficial. Todos os passos de `cargo`
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
data/           # .desktop e metainfo AppStream
packaging/obs/  # spec RPM para o OBS (home:rodrigosbrito:lyra/postgres-draco)
```

## Arquitetura

- `draco-core` é agnóstico de UI: tudo que fala com o Postgres, com SSH ou com
  o disco vive lá, testável sem GTK.
- `draco-app` é a fronteira de aplicação: a UI deve
  consumir seus DTOs e casos de uso, sem importar `draco-core` diretamente.
- `src-tauri` expõe somente comandos finos sobre `draco-app`; capabilities são
  explícitas e o frontend não recebe credenciais do Secret Service.
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

## Pull requests

- Um PR por módulo ou correção, quando possível.
- Rode `cargo clippy --workspace` e `cargo test -p draco-core` antes de abrir
  o PR — ambos devem passar sem erros.

## Estilo de commit

```
feat: descrição curta

Corpo opcional explicando o porquê.
```

Tipos: `feat`, `fix`, `chore`, `refactor`, `docs`.
