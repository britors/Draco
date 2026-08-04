# Release Linux pendente

O código em desenvolvimento está em `2.0.3`, mas RPM/OBS, AUR e AppStream ainda apontam para
`1.1.3`. A tag `v1.1.3` é anterior à introdução de `src-tauri`; portanto, seu tarball não pode ser
usado para validar nem publicar o bundle Tauri atual.

Antes da próxima publicação:

1. criar a tag imutável `v2.0.3` a partir do worktree validado;
2. atualizar `Version`/`pkgver`, `Release`/`pkgrel`, AppStream e changelog;
3. baixar o tarball da tag e substituir o SHA-256 do `aur/PKGBUILD`;
4. gerar o tarball de fontes e `vendor.tar.zst` para o OBS;
5. construir, instalar e abrir RPM e AUR em Wayland e X11;
6. remover este arquivo — o contrato de distribuição exige um registro enquanto as versões
   publicada e de desenvolvimento forem diferentes.

Não reutilizar a tag `v1.1.3`, não publicar com `SKIP` e não criar checksum a partir de `main`,
porque esse branch é mutável.

## Validação local concluída em 04/08/2026

- testes frontend, Rust, Tauri, Clippy e E2E PostgreSQL real passaram;
- metadados desktop/AppStream foram validados sem rede;
- o binário release iniciou em Wayland e X11/XWayland a partir de uma raiz de instalação
  temporária;
- `ldd` não encontrou bibliotecas ausentes nem dependências GTK4/libadwaita/GtkSourceView5;
- o fallback `draco-gtk` continua compilável até os gates de rollback e estabilização serem
  satisfeitos.
