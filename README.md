<p align="center">
  <img src="logo/draco_postgresql_logo_v3_no_cylinder.png" alt="Draco" width="200">
</p>

<p align="center">
  <sub>Logo inspirado na imagem de referência gerada com <a href="https://www.craiyon.com/pt/image/Hem-omdSQoWd0VBfMnBbHg">Craiyon</a>.</sub>
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

- Driver Postgres assíncrono (`tokio-postgres`) para queries — sem CLI externo
  (`psql`); backup/restauração usam explicitamente as ferramentas oficiais do
  PostgreSQL.
- Túnel SSH (incluindo jump host) feito em processo (`russh`), sem depender do
  binário `ssh`.
- Interface oficial Tauri 2, com frontend local sem CDN; o `draco-gtk` é o
  fallback compilável durante a estabilização.
- Nenhuma senha ou passphrase é manuseada em texto plano — armazenamento
  delegado ao Serviço de Segredos do sistema (GNOME Keyring/KWallet, via
  `keyring`), já integrado ao sistema.

> **Status**: Tauri 2 é o artefato oficial; o `draco-gtk` permanece compilável
> durante a transição. Veja
> [`docs/migration/rust-gtk-parity.md`](docs/migration/rust-gtk-parity.md) para
> o que já foi portado e o que falta.

---

## Estrutura do repositório

- `draco-core`: pool Postgres, túnel SSH, queries de introspecção/DDL/stats,
  storage local (TOML/XDG) e segredos — sem dependência de nenhum toolkit
  gráfico.
- `draco-app`: casos de uso e DTOs serializáveis, fronteira compartilhada entre
  os frontends.
- `src-tauri`: shell Tauri 2, capabilities mínimas e bridge IPC tipada.
- `frontend/dist`: shell web local empacotado pelo Tauri, sem dependências de
  rede em runtime.
- `draco-gtk`: frontend GTK4/libadwaita de fallback (binário `draco-gtk`).
- `data`: `.desktop`, metadados AppStream e ícones.
- `packaging/obs`: artefatos para o pacote RPM no OBS
  (`home:rodrigosbrito:lyra/postgres-draco`).
- `docs/migration`: matriz de paridade funcional da reescrita.

## Compilando

Dependências de sistema para o app oficial (nomes Fedora/openSUSE): WebKitGTK
4.1, GTK3, OpenSSL, librsvg e `xdg-desktop-portal` (seletores de arquivo
nativos), além de um compilador Rust estável recente
(`cargo`, `rustc` ≥ 1.85). Para compilar o fallback GTK, instale também
GTK4, libadwaita e GtkSourceView5.

```sh
cargo build --locked --release -p draco-tauri
./target/release/draco
```

Para validar o fallback GTK durante a estabilização:

```sh
cargo run -p draco-gtk --bin draco-gtk
```

No fallback GTK, a variável `DRACO_LOG` controla o nível de log, por exemplo
`DRACO_LOG=debug cargo run -p draco-gtk --bin draco-gtk`. Para o Tauri, use
`RUST_BACKTRACE=1 cargo run -p draco-tauri`. Senhas e conteúdo de query nunca
são registrados nos logs.

### Testes

```sh
cargo test -p draco-core
cargo test -p draco-app
cargo test -p draco-tauri
(cd frontend && npm run check && npm test)
```

## Instalação

Cada tag `vX.Y.Z` gera e anexa à GitHub Release quatro pacotes nativos:

- instalador Windows x64 em NSIS (`.exe`), sem janela de console e instalado
  para o usuário atual por padrão;
- pacote `.deb` compilado no Ubuntu 24.04;
- pacote `.rpm` compilado no Fedora 43;
- pacote `.rpm` compilado no openSUSE Leap 16.0.

O RPM oficial via OBS (`home:rodrigosbrito:lyra/postgres-draco`) continua
disponível para openSUSE. O nome do pacote OBS não é "draco" simples porque
esse nome já é usado pelo projeto "graphics" do openSUSE; o aplicativo continua
se chamando Draco.

> **Release Tauri:** a tag `v2.0.3` contém o novo aplicativo oficial e está
> publicada no OBS para openSUSE Leap 16.0. Os pacotes multiplataforma começam
> na próxima tag, pois o workflow não altera releases anteriores.

## Licença

[GPL-3.0-or-later](LICENSE) © Rodrigo Brito
