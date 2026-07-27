<p align="center">
  <img src="logo.svg" alt="Draco" width="200">
</p>

<p align="center">
  <a href="https://github.com/britors/Draco/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/britors/Draco" alt="License">
  </a>
  <a href="https://github.com/britors/Draco/issues">
    <img src="https://img.shields.io/github/issues/britors/Draco" alt="Issues">
  </a>
</p>

**Draco** é o cliente de banco de dados do ecossistema **Lyra OS**: explorador de
esquemas, editor SQL e ferramenta de administração para PostgreSQL. Funciona em
qualquer distribuição Linux moderna, com integração visual prioritária ao Lyra
(GNOME/Wayland).

- Driver Postgres assíncrono (`tokio-postgres`) — sem CLI externo (`psql`).
- Túnel SSH (incluindo jump host) feito em processo (`russh`), sem depender do
  binário `ssh`.
- Interface em GTK4 + libadwaita; editor SQL em GtkSourceView5 (sem Monaco/CDN).
- Nenhuma senha ou passphrase é manuseada em texto plano — armazenamento
  delegado ao Serviço de Segredos do sistema (GNOME Keyring/KWallet, via
  `oo7`), já integrado ao sistema.

> **Status**: em reescrita ativa de Electron/TypeScript para Rust + GTK4. Veja
> [`docs/migration/rust-gtk-parity.md`](docs/migration/rust-gtk-parity.md) para
> o que já foi portado e o que falta.

---

## Estrutura do repositório

- `draco-core`: pool Postgres, túnel SSH, queries de introspecção/DDL/stats,
  storage local (TOML/XDG) e segredos — sem dependência de nenhum toolkit
  gráfico.
- `draco-gtk`: frontend GTK4/libadwaita (binário `draco`).
- `data`: `.desktop`, metadados AppStream e ícones.
- `packaging/obs`: artefatos para o pacote RPM no OBS
  (`home:rodrigosbrito:lyra/draco`).
- `aur`: `PKGBUILD` para o Arch User Repository.
- `docs/migration`: matriz de paridade funcional da reescrita.

## Compilando

Dependências de sistema (nomes Fedora/openSUSE): `gtk4-devel` (≥ 4.12),
`libadwaita-devel` (≥ 1.5), `gtksourceview5-devel` (≥ 5.0), um compilador
Rust estável recente (`cargo`, `rustc` ≥ 1.85).

```sh
cargo build --release
./target/release/draco
```

Variável de ambiente `DRACO_LOG` controla o nível de log
(`tracing-subscriber`), por exemplo `DRACO_LOG=debug ./target/debug/draco`.
Senhas e conteúdo de query nunca são registrados nos logs.

### Testes

```sh
cargo test -p draco-core
```

## Instalação

Ainda não há pacotes publicados — o rewrite está em andamento (ver matriz de
paridade). Quando a v1 estiver pronta: RPM via OBS
(`home:rodrigosbrito:lyra/draco`) e AUR (`draco`). Não há build Windows —
nenhum app do ecossistema Lyra OS sustenta essa plataforma hoje.

## Licença

[GPL-3.0-or-later](LICENSE) © Rodrigo Brito
