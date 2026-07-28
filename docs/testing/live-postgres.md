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

O `DRACO_TEST_CONN_ID` é usado somente para buscar a senha no Secret Service. Senhas e conteúdo
sensível não são impressos pelo teste.

## Checklist automatizado

O cenário executado contra PostgreSQL 18.4 cobre:

- autenticação válida e rejeição de senha inválida;
- schemas, tabelas, colunas, funções, DDL, índices, constraints, FKs e completion data;
- criação/alteração de tabela, importação, browse, update, delete, `ANALYZE` e estatísticas;
- criação, validação, introspecção e chamada de função;
- criação, leitura e `nextval`/`setval` de sequences, além de triggers;
- dashboard, estatísticas do banco, roles, activity, locks e extensões;
- jobs `pg_cron` quando a extensão estiver instalada; no banco de validação ela estava ausente;
- `EXPLAIN` sem `ANALYZE`, execução de query, erro seguido de recuperação e busca global;
- ERD e relações de FK.

DDL de teste é criado em um schema com prefixo `draco_live_`. O schema é removido com
`CASCADE` ao final e sobras de uma execução interrompida são removidas no início da próxima;
nenhum objeto da aplicação é usado para mutação.

Resultado em 28/07/2026:

```text
test connects_and_introspects_the_real_database ... ok
test result: ok. 1 passed; 0 failed
```

## Checklist transversal

| Cenário | Evidência |
|---|---|
| Nominal contra Postgres real | teste E2E acima passou |
| Conexão ausente/perdida | `ConnectionManager`, editor e explorer exibem erro; recuperação após erro SQL coberta pelo teste |
| SSH/jump host | suporte permanece coberto pelo `PostgresDriver`; não executado porque o ambiente E2E não possui endpoint SSH configurado |
| Loading, vazio e erro | estados implementados nas views; vazio de `pg_cron`, activity e locks observado no teste |
| Operação longa fora da thread GTK | chamadas GTK usam `runtime.spawn` + `MainContext::spawn_local` |
| Mutação perigosa | teste usa schema isolado; UI mantém confirmações para operações destrutivas |
| Segredos e queries em logs | teste usa Secret Service e não registra senha nem conteúdo de credencial |
