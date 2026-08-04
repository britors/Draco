# Release Linux pendente

A tag imutável `v2.0.3` foi publicada e os metadados locais de RPM/OBS, AUR e AppStream apontam
para ela. Este registro permanece até os canais de pacote serem construídos, instalados e
publicados. A publicação no AUR foi adiada por decisão do mantenedor em 04/08/2026.

Antes da próxima publicação:

1. construir, instalar e publicar o RPM no OBS;
2. quando autorizado, construir, instalar e publicar o pacote AUR;
3. remover este arquivo após confirmar os artefatos publicados.

O checksum AUR foi calculado do tarball da tag, nunca de `main`, e não usa `SKIP`.

## Validação local concluída em 04/08/2026

- testes frontend, Rust, Tauri, Clippy e E2E PostgreSQL real passaram;
- metadados desktop/AppStream foram validados sem rede;
- o binário release iniciou em Wayland e X11/XWayland a partir de uma raiz de instalação
  temporária;
- `ldd` não encontrou bibliotecas ausentes nem dependências GTK4/libadwaita/GtkSourceView5;
- o fallback `draco-gtk` continua compilável até os gates de rollback e estabilização serem
  satisfeitos.
