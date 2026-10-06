# Contrato inicial da camada de aplicação

O crate `draco-app` é a fronteira entre qualquer frontend e o motor `draco-core`. O frontend não
deve importar `draco-core`, acessar o `ConnectionManager` ou serializar tipos do driver PostgreSQL.

## Responsabilidades

- carregar e persistir somente metadados de conexões;
- sincronizar o ciclo de vida das conexões;
- aceitar senhas apenas como argumentos transitórios de `connect`;
- executar queries e scripts por IDs de conexão;
- executar backup/restauração pelos binários oficiais do PostgreSQL, com cancelamento por operação;
- mediar o Assistente read-only, seu histórico e suas chaves no Secret Service;
- devolver DTOs serializáveis, sem credenciais;
- manter a mesma fonte de verdade para GTK e Tauri durante a transição.

## Invariantes de segurança

- senhas de banco, senhas SSH, passphrases e chaves de IA não fazem parte de `ConnectionInput`,
  `ConnectionView` ou `QueryResult`;
- senhas não são persistidas em `store`;
- SQL e resultados não são registrados em logs pela camada;
- uma operação só pode executar quando a conexão correspondente está no estado `connected`;
- a aplicação não concede acesso genérico a filesystem, shell ou URLs remotas.

## Contrato de health check

```json
{
  "service": "draco-app",
  "ready": true,
  "updates_managed": false
}
```

Esse contrato é deliberadamente pequeno para ser usado pelo smoke test do shell Tauri.
`updates_managed` é `true` dentro de um Flatpak, onde a loja entrega as atualizações; nesse caso
o frontend não oferece nem roda a checagem de release do GitHub.

## Contrato de query

`execute_query(connection_id, sql)` executa uma única query preparada. Para múltiplas instruções,
o frontend deve chamar `execute_script(connection_id, sql)`. A instrução é preparada antes
(protocolo estendido, que rejeita instruções extras e fornece as colunas mesmo sem linhas) e então
executada pelo protocolo simples, para que todo tipo chegue no formato de texto do próprio
PostgreSQL. `bool`, inteiros e floats finitos viram valores JSON nativos; `numeric`, datas,
`uuid`, `json`, arrays e os demais tipos chegam como string, sem perda de precisão; `NULL` vira
`null`. O resultado tem uma forma estável:

```json
{
  "columns": ["one"],
  "rows": [{"one": 1}],
  "duration_ms": 4
}
```

O estado da conexão (`disconnected`, `connecting`, `connected` ou `error`) acompanha cada
`ConnectionView`, permitindo que a UI represente loading, vazio, erro e retry sem inferir estado a
partir de mensagens técnicas.

Em conexões `read_only`, o PostgreSQL recusa a escrita (SQLSTATE `25006`) e a mensagem chega ao
editor como diagnóstico sem tradução; SQL que tente reabilitar escrita na sessão é recusado antes
com `connection_read_only`. Os demais comandos de escrita (os métodos de `Application` que usam
`writable_driver`) falham com `connection_read_only` sem tocar o banco. O frontend espelha essa
lista em `WRITE_COMMANDS`, e o teste de contrato exige que as duas fiquem iguais. Em conexões de
`production`, esses comandos e SQL com possível escrita pedem o nome da conexão antes do `invoke`.

## Contrato de operações longas

Queries, scripts e backup/restauração podem receber um `operationId` gerado pela UI. Enquanto a
operação está em andamento, `cancel_query` ou `cancel_operation` recebe esse mesmo ID; o backend
mantém o registro somente durante a execução e o remove ao concluir, falhar ou cancelar.

Exemplo de request:

```json
{
  "id": "connection-id",
  "sql": "SELECT pg_sleep(30)",
  "operationId": "ui-1720000000000-ab12"
}
```

O resultado continua usando o DTO estável de query. Cancelamento ou falha retorna o envelope de
erro da bridge, sem expor SQL, credenciais ou detalhes internos do driver. Operações sem
`operationId` continuam aceitas para chamadas não interativas e preservam compatibilidade. No
PostgreSQL, o backend registra o PID da sessão da operação e usa `pg_cancel_backend` direcionado,
evitando cancelar uma query concorrente da mesma conexão.

## Segurança da bridge

A bridge Tauri expõe comandos finos que chamam estes métodos, com envelope de erro serializável,
capability restrita à janela `main` e nenhum plugin de filesystem/processo habilitado. Nenhum
comando devolve o objeto interno `PostgresDriver`; o threat model está em
[`docs/security/threat-model.md`](../security/threat-model.md).

## Envelope de erro

Toda falha de comando chega ao frontend como:

```json
{ "code": "invalid_input", "key": "validation.objectNameRequired", "params": { "label": "label.schema" }, "message": "Schema name is required and cannot contain control characters" }
```

- `code` é a categoria estável: `invalid_input`, `connection_not_found`, `connection_not_active`,
  `connection_read_only`, `assistant_error`, `operation_error`, `github_error`, `backend_error` ou `filesystem_error`.
- `key`, quando presente, é uma chave estável dos catálogos em `frontend/dist/locales`; o
  frontend traduz por ela (`errorMessage` em `i18n.js`). Carregam chave: todas as validações
  (`invalid_input`, chaves `validation.*`, montadas com `draco_app::Validation`), conexão não
  encontrada/inativa/somente leitura, falhas do Assistente (`assistant_error`, chaves `assistant.error.*`: chave
  vazia ou ausente, credential store indisponível, chave recusada, limite de requisições, outro
  status HTTP, nenhum modelo, rede, resposta inválida), falha genérica de operação, falha
  não-PostgreSQL do core e seletor de arquivos indisponível. Nas falhas do Assistente só cruzam
  o IPC o status HTTP e o nome do provedor; o texto de erro do provedor é descartado no core,
  porque alguns provedores ecoam parte da chave de API.
- `params`, quando presente, traz os valores nomeados da mensagem. Valores que começam com
  `label.` são chaves de catálogo e também são traduzidos; os demais (nomes de objetos,
  constraints) entram como estão.
- `message` é o texto em inglês, usado como fallback e em logs. Diagnósticos do PostgreSQL e
  respostas da API do GitHub nunca recebem `key` e são exibidos sem tradução.

## Bridge implementada

O shell inicial em `src-tauri` expõe:

| Comando | Finalidade |
|---|---|
| `health` | smoke check do backend Rust |
| `preferences` / `save_preferences` | ler e persistir tema, cor de destaque, checagem automática de atualização e `notify_long_operations` |
| `check_for_updates` | consultar a última release pública no GitHub e comparar com a versão instalada |
| `operation_finished` | receber só tipo, resultado e duração de uma operação longa; com 10 s ou mais, a janela fora de foco e `notify_long_operations` ligado, mostrar uma notificação do sistema com texto do catálogo fixo do backend |
| `github_status` / `connect_github` / `disconnect_github` | configurar repositório e manter o token exclusivamente no Secret Service |
| `github_branches` / `github_file` | listar branches e ler a definição SQL versionada em uma branch |
| `github_commit_file` | criar ou atualizar o arquivo da definição na branch selecionada |
| `github_compare` | obter o diff nativo entre duas branches do repositório |
| `github_create_pull_request` | abrir um pull request da branch de trabalho para a base selecionada |
| `list_connections` | listar metadados e estados |
| `save_connection` / `delete_connection` | persistir metadados sem senha, incluindo `environment` (`development`/`staging`/`production` ou ausente) e `read_only`; mudar `read_only` fecha a sessão ativa |
| `connect_stored` / `disconnect` | ciclo de vida usando o Secret Service no backend |
| `test_connection` | testar draft e credenciais antes de persistir |
| `execute_query` / `execute_script` | executar SQL pela conexão ativa |
| `execute_explain` | gerar plano JSON com `EXPLAIN` puro, sem `ANALYZE` |
| `cancel_query` | cancelar a operação ativa via `pg_cancel_backend` |
| `list_history` / `delete_history_entry` / `clear_history` | histórico limitado a 50 itens |
| `list_snippets` / `save_snippet` / `rename_snippet` / `delete_snippet` | snippets nomeados e vinculados à conexão |
| `list_schemas` / `list_tables` | introspecção lazy do Explorer |
| `list_schema_objects` | funções/procedures com assinatura, sequences com definição editável e triggers por schema |
| `completion_data` | schemas/tabelas/colunas/funções do banco inteiro num round trip, para o autocomplete do Editor SQL |
| `dashboard` | KPIs, saúde do banco e maiores tabelas |
| `table_detail` | colunas, constraints, índices, FKs, DDL e estatísticas de coluna |
| `save_view_definition` | salvar somente `CREATE OR REPLACE VIEW` para a view selecionada |
| `save_sequence_definition` | salvar somente `ALTER SEQUENCE` para a sequence selecionada |
| `index_definition` | obter `pg_get_indexdef` de um índice comum pertencente à tabela selecionada |
| `save_index_definition` | recriar um índice comum com `DROP` + `CREATE INDEX` na mesma transação |
| `erd` | tabelas e relações de um schema |
| `admin` | activity e locks |
| `cancel_activity` | cancelar somente a query de um PID explícito, preservando a sessão |
| `list_cron_jobs` / `set_cron_job_active` / `delete_cron_job` | detectar pg_cron, listar, pausar/retomar e excluir jobs |
| `list_extensions` / `install_extension` / `drop_extension` | listar e gerenciar extensões por nome validado; `plpgsql` é protegida |
| `query_stats` / `reset_query_stats` | métricas tipadas de pg_stat_statements e reset explícito dos contadores |
| `run_table_maintenance` | executar somente VACUUM/ANALYZE allowlisted em tabela identificada |
| `list_roles` | listar roles e atributos administrativos |
| `create_role` / `delete_role` | criar role sem senha e excluir após confirmação nominal na UI |
| `choose_backup_output` / `choose_restore_input` | abrir seletor nativo e emitir autorização de caminho com finalidade e uso único |
| `run_backup` / `run_restore` | backup/restauração via `pg_dump`, `pg_restore` ou `psql`, somente após autorização nativa do caminho |
| `cancel_operation` | cancelar uma operação de backup/restauração em andamento |
| `assistant_settings` / `save_assistant_settings` | configurações não secretas do Assistente |
| `assistant_models` | listar modelos compatíveis do provedor usando a chave do Secret Service |
| `save_assistant_key` / `clear_assistant_key` | gerenciar chaves no Secret Service |
| `assistant_history` / `clear_assistant_history` | histórico por conexão, como `AssistantMessageView` |
| `assistant_send` | conversar e executar somente ferramentas de inspeção read-only |

O histórico guardado em `ai-history.toml` mantém, nas mensagens de ferramenta, o invólucro de
dado não confiável enviado ao modelo. `assistant_history` e `assistant_send` devolvem
`AssistantMessageView`: nessas mensagens, `content` é só a saída da ferramenta, e `tool_failed`
marca uma chamada recusada ou com erro, cujo texto é dirigido ao modelo e não é exibido.

O painel Query Stats pode iniciar `assistant_send` com a SQL e suas métricas de
`pg_stat_statements`; o fluxo continua limitado às ferramentas read-only do Assistente e nunca
ganha uma operação de escrita por causa desse atalho.

`check_for_updates` é o único comando que sai para a internet fora do Assistente de IA: ele faz uma
requisição `GET` somente leitura para `api.github.com/repos/britors/Draco/releases/latest` a partir
do backend (nunca do frontend) e nunca baixa nem instala nada — só devolve a tag mais recente e o
link da release para o usuário decidir.

Erros de driver, Secret Service e operações externas são convertidos para um envelope sem detalhes
sensíveis antes de serem enviados pelo IPC. Backup/restore não transportam stdout/stderr das
ferramentas: o resultado contém apenas sucesso, cancelamento e exit code.

## Contrato dos editores de objetos

O frontend oferece Programming como área principal de desenvolvimento do banco, com seleção de
conexão/schema e acesso direto a views, functions, procedures e triggers. Cada objeto abre uma
tela dedicada com botão Voltar e o mesmo editor com realce usado no SQL Editor; definições de
programação não são editadas em modal. Quando o GitHub está conectado em Preferências, a tela
também oferece branches, leitura/commit do arquivo versionado, diff contra a definição implantada,
comparação entre branches e criação de pull request. Explorer e
Administration continuam cobrindo descoberta estrutural e operações de DBA, enquanto o SQL
Editor permanece responsável pela execução de consultas e scripts.

Os editores de views, sequences e índices não expõem execução SQL genérica. Cada método recebe o
schema, o objeto selecionado e uma única definição, valida o prefixo e confirma que os
identificadores do DDL ainda apontam para esse mesmo objeto. O protocolo estendido do PostgreSQL
rejeita instruções adicionais no mesmo payload.

Índices pertencentes a constraints (`PRIMARY KEY`, `UNIQUE` ou `EXCLUDE`) carregam
`constraint_name` no detalhe da tabela e não podem ser recriados pelo editor de índices. Para os
demais, o backend executa o `DROP INDEX` gerado pelo Draco e o `CREATE INDEX` revisado pelo usuário
na mesma transação; qualquer falha restaura o índice original.
