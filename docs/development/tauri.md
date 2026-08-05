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

Para executar o bundle Tauri, a distribuição precisa fornecer WebKitGTK 4.1, GTK3, OpenSSL,
librsvg e `xdg-desktop-portal` (usado pelos seletores nativos de backup/restauração). Os nomes
variam por distribuição; em Ubuntu 24.04 são `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libssl3`,
`librsvg2-2` e `xdg-desktop-portal`. O build local precisa também dos pacotes `-dev`
correspondentes às bibliotecas linkadas.

Os canais de distribuição são:

| Artefato | Canal | Status |
|---|---|---|
| Windows x64 NSIS (`.exe`) | GitHub Release | suportado a partir da próxima tag |
| Ubuntu 24.04 x64 (`.deb`) | GitHub Release | suportado a partir da próxima tag |
| Fedora 43 x64 (`.rpm`) | GitHub Release | suportado a partir da próxima tag |
| openSUSE Leap 16.0 x64 (`.rpm`) | GitHub Release | suportado a partir da próxima tag |
| openSUSE Leap 16.0 (`.rpm`) | OBS `home:rodrigosbrito:lyra/postgres-draco` | suportado |

O workflow `release.yml` é acionado apenas por uma tag `vX.Y.Z` existente (ou
manualmente apontando para ela), exige que a tag coincida com as versões do
workspace, frontend e Tauri, compila cada RPM dentro da distribuição de destino
e publica os quatro pacotes mais um `SHA256SUMS`. O instalador NSIS usa o modo
`currentUser`; a aplicação release usa o subsistema gráfico do Windows e não
abre uma janela de console. Assinatura Authenticode ainda depende da futura
configuração de um certificado no repositório.

Os manifests de desenvolvimento (`Cargo.toml`, `tauri.conf.json` e `frontend/package.json`) podem
estar à frente da última tag publicada. O RPM e a primeira entrada AppStream, porém, sempre
descrevem a mesma tag imutável. O teste `frontend/tests/distribution.test.mjs` impede divergência
entre os metadados publicados. A tag `v2.0.3` contém o shell Tauri oficial e está publicada no
OBS.

## Build offline e validação do pacote

O bundle de produção consome `frontend/dist` diretamente e não executa npm. As dependências npm
são apenas de teste, estão fixadas com integridade em `package-lock.json` e são instaladas no CI
com `npm ci --ignore-scripts`. No OBS, `cargo_vendor` gera `vendor.tar.zst`; build e testes usam
`cargo --locked --offline`.

Além dos testes Rust/frontend, o CI valida `.desktop` e AppStream sem rede, monta uma raiz de
instalação temporária, confere binário/ícone/metadados e rejeita bibliotecas dinâmicas ausentes.
Wayland, X11, leitor de tela e conexão PostgreSQL real permanecem no checklist manual porque
dependem de uma sessão desktop e de serviços externos reais.

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

Credenciais salvas por versões que usavam `oo7` tinham atributos diferentes dos usados por
`keyring`. No primeiro acesso no Linux, o backend procura a entrada legada diretamente no Secret
Service, copia para o novo namespace, confere a cópia e somente então remove a entrada antiga.
Isso vale para senha PostgreSQL, SSH/jump host e chaves do Assistente; nenhum valor passa pelo
frontend ou pelos logs.

## Paridade, rollback e remoção do GTK

O `draco-gtk` não deve ser removido junto com a criação do shell Tauri. A decisão registrada em
[`tauri-stabilization.md`](../migration/tauri-stabilization.md) exige um período de estabilização,
validação contra PostgreSQL real e confirmação dos artefatos instalados. Até lá, `cargo check -p
draco-gtk` continua no CI. Se o Tauri falhar em produção, o rollback é o pacote anterior do GTK;
os arquivos de configuração e o Secret Service continuam compatíveis.
