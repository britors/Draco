# Validação E2E contra PostgreSQL real

O teste `draco-core/tests/live_postgres.rs` é ignorado por padrão porque precisa de um
PostgreSQL acessível e de uma senha armazenada no Secret Service. Ele não contém credenciais:

```sh
DRACO_TEST_CONN_ID=torven-local \
DRACO_TEST_HOST=localhost \
DRACO_TEST_DB=torven \
DRACO_TEST_USER=torven \
cargo test -p draco-core --test live_postgres -- --ignored --nocapture
```

Para validar a ponte usada pelos comandos Tauri, rode também:

```sh
DRACO_TEST_CONN_ID=torven-local \
DRACO_TEST_HOST=localhost \
DRACO_TEST_DB=torven \
DRACO_TEST_USER=torven \
cargo test -p draco-app --test live_postgres -- --ignored --nocapture
```

O `DRACO_TEST_CONN_ID` é usado somente para buscar a senha no Secret Service. Senhas e conteúdo
sensível não são impressos pelo teste.

Para executar os dois testes em sequência, com a mesma configuração e uma checagem prévia de
disponibilidade do servidor:

```sh
./scripts/test-live-postgres.sh
```

O script aceita `DRACO_TEST_CONN_ID`, `DRACO_TEST_HOST`, `DRACO_TEST_DB` e `DRACO_TEST_USER` já
definidos no ambiente. A senha continua exclusivamente no Secret Service.

## Checklist automatizado

O cenário executado contra PostgreSQL 18.4 cobre:

- autenticação válida e rejeição de senha inválida;
- recuperação, pela fronteira da aplicação Tauri, após senha inválida e após desconexão explícita;
- schemas, tabelas, colunas, funções, DDL, índices, constraints, FKs e completion data;
- criação/alteração de tabela, importação, browse, update, delete, `ANALYZE` e estatísticas;
- criação, validação, introspecção e chamada de função;
- criação, leitura e `nextval`/`setval` de sequences, além de triggers;
- dashboard, estatísticas do banco, roles, activity, locks, extensões e query stats
  (`pg_stat_statements`, quando instalada);
- jobs `pg_cron` quando a extensão estiver instalada; no banco de validação ela estava ausente;
- `EXPLAIN` sem `ANALYZE`, execução de query, erro seguido de recuperação e busca global;
- cancelamento de uma query longa pelo PID de `pg_stat_activity`, seguido de query válida na
  mesma conexão de aplicação;
- ERD e relações de FK.
- contrato de aplicação consumido pelo Tauri (conexão, schemas, funções/sequences/triggers,
  detalhe de tabela com DDL/estatísticas, dashboard, query, EXPLAIN, administração e listagem de
  roles); quando a conexão fonte é superuser, também cria, relê e exclui uma role temporária sem
  login.

DDL de teste é criado em um schema com prefixo `draco_live_`. O schema é removido com
`CASCADE` ao final, inclusive quando uma asserção falha; sobras de uma execução interrompida
são removidas no início da próxima. O teste de aplicação também remove sua conexão temporária
em caso de falha. A role temporária usa o prefixo `draco_live_role_`, nunca recebe login ou senha
e é excluída antes das asserções finais. Nenhum objeto da aplicação é usado para mutação.

Resultado conjunto mais recente em 04/08/2026, contra PostgreSQL 18.4:

```text
test connects_and_introspects_the_real_database ... ok
test result: ok. 1 passed; 0 failed
test application_boundary_reaches_postgres_for_tauri_views ... ok
test result: ok. 1 passed; 0 failed
```

O cenário da aplicação inclui o comando de `EXPLAIN` puro, rejeição de autenticação inválida,
desconexão/reconexão e o ciclo administrativo de role consumidos pelo frontend Tauri. Essa
execução valida o backend e a fronteira `draco-app`; a webview Tauri é coberta pelos contratos
locais e pelo smoke manual. Cenários que exigem endpoints SSH e chaves reais de IA permanecem
condicionados à disponibilidade desses serviços externos.

## Checklist transversal

| Cenário | Evidência |
|---|---|
| Nominal contra Postgres real | core e `draco-app` passaram em 04/08/2026 contra PostgreSQL 18.4 |
| Conexão ausente/perdida | fronteira Tauri recusa operação desconectada e volta a executar após reconexão; recuperação após erro SQL também coberta |
| SSH/jump host | suporte permanece coberto pelo `PostgresDriver`; não executado porque o ambiente E2E não possui endpoint SSH configurado |
| Loading, vazio e erro | estados cobertos pelos contratos frontend; vazio de `pg_cron`, activity e locks observado no teste real |
| Operação longa fora da thread GTK | chamadas GTK usam `runtime.spawn` + `MainContext::spawn_local` |
| Mutação perigosa | teste usa schema isolado; UI mantém confirmações para operações destrutivas |
| Segredos e queries em logs | teste usa Secret Service e não registra senha nem conteúdo de credencial |
