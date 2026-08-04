# Threat model da aplicação Tauri

## Fronteira de confiança

O frontend é conteúdo não confiável. Ele pode solicitar somente os comandos Rust registrados no
`invoke_handler`; não recebe acesso direto a `draco-core`, ao `ConnectionManager`, ao Secret
Service, ao filesystem ou a processos do sistema.

O backend Rust é o único componente autorizado a usar credenciais. Senhas de PostgreSQL/SSH e
chaves de provedores de IA entram apenas como argumentos transitórios ou são lidas do Secret
Service. Elas não aparecem em DTOs, respostas de comando, eventos, logs, histórico do frontend,
`localStorage` ou `sessionStorage`.

## Capabilities e CSP

A única janela é `main` e sua capability não concede permissões de plugins (`permissions: []`).
Não há plugins de filesystem, dialog, shell, process, HTTP ou URL remota habilitados. A execução de
backup usa somente os binários fixos `pg_dump`, `pg_restore` e `psql`, iniciados pelo backend sem
shell intermediário.

A CSP permite apenas assets locais e o canal IPC interno:

| Diretiva | Política | Motivo |
|---|---|---|
| `default-src` | `'self'` | bloquear origens externas por padrão |
| `script-src` | `'self'` | nenhum script inline ou remoto |
| `style-src` / `font-src` | `'self'` | design system somente local |
| `img-src` | `'self' data:` | logos locais e imagens efêmeras sem rede |
| `connect-src` | `'self' ipc: http://ipc.localhost` | IPC Tauri; APIs externas usam o backend |
| `object-src` / `base-uri` / `frame-ancestors` | `none` | reduzir vetores de embed, base URL e framing |

Não há `unsafe-eval`, wildcard remoto, iframe ou `dangerouslySetInnerHTML`.

## Arquivos e processos

Backup e restauração aceitam somente caminhos absolutos sem componente `..`, fornecidos
explicitamente pelo usuário. O backend não expõe leitura/escrita genérica de arquivos. Argumentos
de PostgreSQL são passados diretamente a `std::process::Command`, nunca por shell; a política de
processos é uma allowlist interna dos três binários oficiais.

O cancelamento usa um canal interno associado a um ID de operação e não transforma o ID ou os
argumentos em comando executável.

## Erros e payloads

Payloads vazios, IDs de operação duplicados, compressão fora de `0..=9` e caminhos relativos ou
com traversal são rejeitados antes de alcançar o driver. Erros de driver, Secret Service,
filesystem e provedores de IA são convertidos em envelopes genéricos no IPC; mensagens de
validação não ecoam caminhos privados.

## Checklist antes de habilitar o frontend

- [x] capability restrita à janela `main`, sem plugins de filesystem/processo/dialog;
- [x] CSP local, sem `unsafe-eval` e sem recursos remotos;
- [x] segredos ausentes dos DTOs e respostas de erro redigidas;
- [x] backup/restauração sem shell e com allowlist de executáveis;
- [x] caminhos absolutos sem traversal;
- [x] testes negativos para payloads malformados e vazamento de erro;
- [ ] validar o artefato empacotado em cada distribuição Linux antes de torná-lo padrão.
