# Decisão de estabilização Tauri e rollback

## Decisão

O Tauri é o frontend oficial e o binário distribuído a partir da versão `1.1.3`, mas o crate
`draco-gtk` permanece compilável por um período de estabilização. Removê-lo agora dificultaria
rollback e confundiria falhas de paridade com falhas de empacotamento.

## Checklist para apagar `draco-gtk`

- [ ] três ciclos de release sem regressão bloqueadora no shell Tauri;
- [ ] fluxo de conexão, Explorer, SQL, histórico, snippets, Dashboard, Admin, ERD e Assistente
      validado contra PostgreSQL real;
- [ ] smoke test do binário instalado em Wayland e X11;
- [ ] RPM OBS e AUR instalados sem dependências GTK4/libadwaita/GtkSourceView5;
- [ ] migração de configurações e Secret Service confirmada em upgrade;
- [ ] checklist visual da issue #105 aprovado para loading, vazio, erro, acessibilidade e ações
      destrutivas;
- [ ] uma versão de rollback do pacote GTK publicada e testada.

Até todos os itens serem marcados, o CI mantém a etapa separada de `cargo check -p draco-gtk` no
job Linux, além de lint/testes do workspace, contratos frontend e build do binário Tauri.

## Rollback

O rollback é feito instalando o último pacote GTK estável e preserva os dados porque ambos os
frontends usam os mesmos caminhos XDG, IDs de conexão, snippets, histórico e Secret Service.
Nenhum comando de migração destrutiva deve ser adicionado como pré-requisito para iniciar o Tauri.
