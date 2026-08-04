# Desenvolvimento e entrega do app Tauri

## Artefato oficial

O binário oficial agora é `target/release/draco`, produzido pelo crate
`draco-tauri`. O package id continua `org.lyraos.Draco`; `draco-gtk` permanece no
workspace apenas durante a estabilização e não é o artefato de distribuição.

```sh
cargo run -p draco-tauri
cargo build --locked --release -p draco-tauri
```

O frontend é estático e não tem dependências npm de runtime. Os testes podem ser executados
offline depois do checkout:

```sh
(cd frontend && npm run check && npm test)
cargo test --workspace --exclude draco-gtk
cargo clippy --workspace --exclude draco-gtk --all-targets -- -D warnings
```

## Dependências Linux

Para executar o bundle Tauri, a distribuição precisa fornecer WebKitGTK 4.1, GTK3, OpenSSL e
librsvg. Os nomes variam por distribuição; em Ubuntu 24.04 são `libwebkit2gtk-4.1-0`, `libgtk-3-0`,
`libssl3` e `librsvg2-2`. O build local precisa também dos pacotes `-dev` correspondentes.

O caminho de distribuição inicial é:

| Artefato | Canal | Status |
|---|---|---|
| RPM | OBS `home:rodrigosbrito:lyra/postgres-draco` | suportado |
| pacote Arch | AUR `postgres-draco` | suportado |
| bundle Tauri `.deb`/AppImage | build local/QA | experimental, não é requisito de publicação |

## Debug e logs

Rode a aplicação pelo terminal para preservar o stderr:

```sh
RUST_BACKTRACE=1 cargo run -p draco-tauri
```

O frontend nunca registra senhas, chaves, SQL ou resultados em storage do navegador. Erros de IPC
são envelopes genéricos; diagnósticos detalhados devem ser investigados no processo Rust sem copiar
credenciais para issues ou logs públicos.

## Configurações existentes

O Tauri usa os mesmos arquivos XDG e a mesma camada `draco-core` do GTK. Conexões, snippets,
histórico, preferências e chaves não são migrados por uma segunda rotina: o app novo lê a fonte
existente e preserva os IDs. O usuário deve manter o Secret Service disponível no primeiro
lançamento; nenhuma senha é convertida para TOML ou para `localStorage`.

## Paridade, rollback e remoção do GTK

O `draco-gtk` não deve ser removido junto com a criação do shell Tauri. A decisão registrada em
[`tauri-stabilization.md`](../migration/tauri-stabilization.md) exige um período de estabilização,
validação contra PostgreSQL real e confirmação dos artefatos instalados. Até lá, `cargo check -p
draco-gtk` continua no CI. Se o Tauri falhar em produção, o rollback é o pacote anterior do GTK;
os arquivos de configuração e o Secret Service continuam compatíveis.
