# Decisão de estabilização Tauri e rollback

## Decisão

O Tauri é o frontend oficial e será o binário distribuído a partir da versão `2.0.3`, mas o crate
`draco-gtk` permanece compilável por um período de estabilização. Removê-lo antes dos gates de
release dificultaria rollback e confundiria falhas de paridade com falhas de empacotamento.

## Checklist para apagar `draco-gtk`

- [x] CI bloqueia regressões Rust, bridge Tauri, contratos frontend/visuais e metadados de
      distribuição;
- [x] raiz de pacote temporária valida binário, desktop entry, AppStream, ícone e bibliotecas
      dinâmicas no CI;
- [x] RPM/OBS atualizado para a tag Tauri `v2.0.3`, com fontes imutáveis, patch AppStream e
      crates vendorizados; a revisão 17 concluiu com `succeeded` e `rpmlint` sem erros ou avisos;
- [ ] AUR construído, instalado e publicado a partir da tag Tauri `v2.0.3`, com SHA-256
      (publicação adiada pelo mantenedor em 04/08/2026; ver `packaging/RELEASE_PENDING.md`);

- [ ] três ciclos de release sem regressão bloqueadora no shell Tauri;
- [x] fluxo de conexão, Explorer, SQL, histórico, snippets, Dashboard, Admin e ERD validado contra
      PostgreSQL 18.4; Assistente validado localmente, com chamadas externas condicionadas às
      chaves dos provedores;
- [x] smoke test do binário release em uma raiz de instalação temporária, usando Wayland e
      X11/XWayland em 04/08/2026;
- [x] artefato RPM do OBS extraído e iniciado em Wayland, com `ldd` sem bibliotecas ausentes e
      sem dependências GTK4/libadwaita/GtkSourceView5;
- [ ] artefato AUR instalado sem dependências GTK4/libadwaita/GtkSourceView5;
- [x] migração de configurações e entradas legadas do Secret Service confirmada pelo E2E real;
- [x] checklist visual da issue #105 aprovado para loading, vazio, erro, acessibilidade e ações
      destrutivas;
- [ ] uma versão de rollback do pacote GTK publicada e testada.

Até todos os itens serem marcados, o CI mantém a etapa separada de `cargo check -p draco-gtk` no
job Linux, além de lint/testes do workspace, contratos frontend e build do binário Tauri.

## Rollback

O rollback é feito instalando o último pacote GTK estável e preserva os dados porque ambos os
frontends usam os mesmos caminhos XDG, IDs de conexão, snippets, histórico e Secret Service.
Nenhum comando de migração destrutiva deve ser adicionado como pré-requisito para iniciar o Tauri.
