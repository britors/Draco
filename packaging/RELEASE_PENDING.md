# Release Linux pendente

A tag imutável `v2.0.3` foi publicada e os metadados locais de RPM/OBS, AUR e AppStream apontam
para ela. Este registro permanece até os canais de pacote serem construídos, instalados e
publicados. A publicação no AUR foi adiada por decisão do mantenedor em 04/08/2026.

O RPM `postgres-draco-2.0.3-lp160.2.1.x86_64.rpm` foi publicado pelo OBS na revisão 17. O
artefato assinado passou no `rpmlint` sem erros ou avisos, teve os metadados validados e iniciou
em Wayland a partir de uma raiz temporária.

Trabalho restante:

1. quando autorizado, construir, instalar e publicar o pacote AUR;
2. remover este arquivo após confirmar o artefato publicado.

O checksum AUR foi calculado do tarball da tag, nunca de `main`, e não usa `SKIP`.

## Validação local concluída em 04/08/2026

- testes frontend, Rust, Tauri, Clippy e E2E PostgreSQL real passaram;
- metadados desktop/AppStream foram validados sem rede;
- o binário release iniciou em Wayland e X11/XWayland a partir de uma raiz de instalação
  temporária;
- o RPM oficial do OBS iniciou em Wayland, tem todas as bibliotecas resolvidas e recebeu
  `0 errors, 0 warnings` do `rpmlint`;
- `ldd` não encontrou bibliotecas ausentes nem dependências GTK4/libadwaita/GtkSourceView5;
- o fallback `draco-gtk` continua compilável até os gates de rollback e estabilização serem
  satisfeitos.
