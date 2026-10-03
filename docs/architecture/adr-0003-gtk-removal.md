# ADR-0003: remoção do frontend GTK e plano de rollback

- Status: aceito (registro retroativo)
- Data da decisão: 2026-08-05
- Data do registro: 2026-10-03
- Escopo: crate `draco-gtk`, distribuição Linux e retorno a uma versão anterior
- Origem: issues #104, #112 e #121

## Contexto

A migração para o Tauri 2 (#104) previa manter o `draco-gtk` compilável durante um período de
estabilização e só removê-lo depois de um checklist registrado em
`docs/migration/tauri-stabilization.md`. A versão `2.0.3` (04/08/2026) foi a primeira distribuída
com o binário Tauri; o crate GTK ainda estava no workspace e era checado pelo CI.

O commit `f4a214c` (05/08/2026, 09:47 -03) removeu o `draco-gtk`, os assets GResource, a matriz
de paridade e o documento de estabilização. A `2.0.4`, publicada horas depois, já saiu sem GTK.

## Decisão

O Tauri 2 é o único frontend e artefato oficial. O `draco-gtk` não volta ao workspace; novas
superfícies são construídas somente em `frontend/dist` + `draco-app`.

## Estado do checklist no momento da remoção

O checklist de `tauri-stabilization.md` (recuperável com
`git show f4a214c^:docs/migration/tauri-stabilization.md`) estava assim:

| Item | Estado em 05/08/2026 |
|---|---|
| CI bloqueia regressões Rust, bridge Tauri, contratos frontend/visuais e metadados | cumprido |
| Raiz de pacote temporária valida binário, desktop entry, AppStream, ícone e bibliotecas | cumprido |
| RPM/OBS atualizado para `v2.0.3` com fontes imutáveis e `rpmlint` limpo | cumprido |
| Fluxo principal validado contra PostgreSQL 18.4 | cumprido |
| Smoke do binário release em Wayland e X11/XWayland | cumprido |
| RPM do OBS iniciado em Wayland sem dependências GTK4/libadwaita/GtkSourceView5 | cumprido |
| Migração de configurações e segredos legados confirmada pelo E2E real | cumprido |
| Checklist visual da issue #105 | cumprido |
| **Três ciclos de release sem regressão bloqueadora no shell Tauri** | **não cumprido** |
| **Uma versão de rollback do pacote GTK publicada e testada** | **não cumprido** |

A remoção, portanto, antecipou dois gates. O motivo registrado no commit foi eliminar código e
assets obsoletos; nenhuma regressão funcional exigia o GTK naquele momento.

### Situação posterior

- Depois da remoção foram publicadas as releases `2.0.4`, `2.0.5`, `2.1.0` e `2.1.4` (as tags
  `2.1.1` a `2.1.3` não tiveram GitHub Release).
- As correções desde então foram de empacotamento e integração com o desktop, não do shell Tauri:
  ícones em tamanhos padrão (`2.0.5`), build travado da CI/release (`52f8a99`) e identidade da
  aplicação no GNOME (`2.1.4`). Nenhuma delas teria sido resolvida voltando ao GTK.
- O gate de "três ciclos" fica considerado cumprido retroativamente.
- O gate de "pacote GTK de rollback publicado" **não** será cumprido: nenhuma release do GitHub,
  nem o OBS, publicou um binário GTK com o motor `2.x`. O plano abaixo substitui esse gate.

## Plano de rollback

### Caminho padrão: voltar para uma release Tauri anterior

As tags são imutáveis e cada release anexa pacotes nativos. Para desfazer uma regressão, baixe o
pacote da release anterior em <https://github.com/britors/Draco/releases> e instale-o por cima
da versão atual:

```sh
sudo apt install --allow-downgrades ./draco_<versão>_ubuntu24.04_amd64.deb   # Ubuntu
sudo dnf downgrade ./draco_<versão>_fedora43_x86_64.rpm                       # Fedora
sudo zypper install --oldpackage ./draco_<versão>_opensuse-leap16.0_x86_64.rpm # openSUSE
```

O repositório OBS publica somente o build mais recente de `postgres-draco`, então o downgrade no
openSUSE usa o RPM da GitHub Release. Esse RPM é gerado pelo Tauri com outro nome de pacote
(`productName` = `Draco`) e instala os mesmos arquivos; quem usa o OBS precisa remover o
`postgres-draco` antes (`sudo zypper remove postgres-draco`). Remover o pacote não apaga a
configuração XDG nem o credential store. No Windows, rode o instalador NSIS da versão anterior.

Os dados são preservados porque todas as versões `2.x` usam os mesmos caminhos XDG, IDs de
conexão e serviços do credential store (`draco` e `draco-ai`). As mudanças de formato desde a
`2.0.3` foram somente aditivas, e nenhum struct usa `deny_unknown_fields`:

- `settings.toml` ganhou `programming_workspace` (opcional). Uma versão anterior ignora o campo,
  mas o descarta se regravar as preferências;
- `github-settings.toml` é um arquivo novo, ignorado por versões anteriores.

### Último recurso: compilar o GTK da tag `v2.0.3`

`v2.0.3` é a última tag que contém o `draco-gtk` e a única em que ele usa o mesmo `draco-core`
`2.x`, inclusive o `keyring`. Não existe pacote publicado; é preciso compilar:

```sh
git clone https://github.com/britors/Draco.git
cd Draco
git checkout v2.0.3
cargo build --locked --release -p draco-gtk
```

São necessários os pacotes de desenvolvimento de GTK4, libadwaita e GtkSourceView5. Esse caminho
nunca foi validado depois da remoção e não recebe correções; existe apenas para recuperar o acesso
caso as releases Tauri fiquem inutilizáveis em um ambiente específico.

### Não fazer: voltar para a linha `1.x`

As versões `1.x` (GTK) gravavam senhas com o `oo7`. Na primeira leitura, a `2.x` copia cada entrada
para o `keyring`, relê para verificar e **remove a entrada legada**
(`draco-core::legacy_secrets`). Um usuário que volte para a `1.x` depois de usar a `2.x` não
encontra mais as senhas e precisa digitá-las de novo. Por isso a `1.x` não é um destino de
rollback suportado.

## Consequências

- Não há mais custo de manter dois frontends nem dependências GTK4/libadwaita/GtkSourceView5 no
  build, no CI ou nos pacotes.
- O rollback suportado é sempre para uma release Tauri anterior; o resultado depende de as
  mudanças de formato continuarem aditivas. Qualquer mudança não aditiva em arquivos XDG/TOML ou
  nos nomes de serviço do credential store precisa de um novo ADR com estratégia de migração e
  retorno.
- `draco-core::legacy_secrets` continua necessário até o fim do período de upgrade da linha `2.x`,
  como já registrado no `CLAUDE.md`.
