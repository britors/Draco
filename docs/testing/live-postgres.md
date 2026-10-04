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
locais e pelo smoke do app instalado descrito abaixo. Cenários que exigem endpoints SSH e chaves reais de IA permanecem
condicionados à disponibilidade desses serviços externos.

## App instalado via WebDriver

`scripts/test-installed-app.sh` abre o pacote instalado (`/usr/bin/draco` por padrão, ou
`DRACO_E2E_APP`) pelo `tauri-driver` e percorre o fluxo principal na webview real:

1. a conexão de teste aparece na lista;
2. o botão Connect conecta usando a senha do credential store;
3. o Explorer carrega os schemas da conexão;
4. o Editor SQL executa `SELECT 42 AS answer, 'draco' AS name` e o grid mostra o resultado;
5. `Run script` cria um schema isolado `draco_e2e_<timestamp>` com uma tabela de três linhas, e
   uma query confere a contagem;
6. uma query contra tabela inexistente mostra o erro e a query seguinte volta a funcionar;
7. o Explorer recarrega a conexão, expande o schema do fixture e abre o detalhe da tabela
   (título, colunas com tipo/`NOT NULL`/PK e DDL);
8. o painel de dados do detalhe pagina as três linhas pela chave primária;
9. o Histórico lista a query de contagem executada.

Ao final, o teste remove o schema do fixture com `DROP SCHEMA ... CASCADE`, mesmo quando um
passo anterior falha; o nome é único por execução e nunca coincide com dados da aplicação.

O script cria um `XDG_CONFIG_HOME` temporário com uma única conexão cujo ID é
`DRACO_TEST_CONN_ID`; as conexões, o histórico e as preferências reais do usuário não são
tocados. A senha continua no credential store, na mesma entrada usada pelos testes acima, e não
passa pelo ambiente nem pelo arquivo temporário.

```sh
DRACO_TEST_CONN_ID=torven-local \
DRACO_TEST_HOST=localhost \
DRACO_TEST_DB=torven \
DRACO_TEST_USER=torven \
./scripts/test-installed-app.sh
```

Dependências: o pacote a testar, `tauri-driver` (`cargo install tauri-driver --locked`),
`WebKitWebDriver` (Ubuntu: `webkit2gtk-driver`; openSUSE: `webkit2gtk4-minibrowser`), Node.js 22+
e `pg_isready`. O teste fica em `frontend/tests/e2e/` e não é executado por `npm test`, porque
precisa de display, do app instalado e de um PostgreSQL real.

Na CI, o job `installed-app-e2e` de `.github/workflows/ci.yml` roda o mesmo teste no
Ubuntu 24.04 contra o binário oficial `target/release/draco`, por meio de
`scripts/ci-installed-app-e2e.sh` sob `dbus-run-session` e `xvfb-run`. O wrapper inicia um
gnome-keyring sem interface, gera uma senha aleatória, cria a role e o banco `draco_e2e` no
PostgreSQL pré-instalado do runner e grava a senha no Secret Service com os atributos usados pelo
crate `keyring` (`service=draco`, `username=password:draco-e2e-ci`). A senha passa só por stdin e
por uma variável de shell não exportada; nunca pelo ambiente, pela linha de comando ou pelos logs.

Cliques e digitação são disparados por eventos do DOM, porque o `WebKitWebDriver` recusa entrada
nativa de ponteiro e teclado em sessões Wayland ("unsupported operation"). Os listeners do próprio
app tratam cada ação; o que o teste não exercita é o roteamento de entrada do compositor.

Resultado mais recente em 03/10/2026: pacote `postgres-draco` 2.1.4 do OBS no openSUSE Leap
16.1 (Wayland), PostgreSQL 18.6 local, `tauri-driver` 2.1.0 e `WebKitWebDriver` 2.52.5 do pacote
`webkit2gtk4-minibrowser`:

```text
✔ lists the stored connection
✔ connects using the password from the credential store
✔ loads schemas in the Explorer
✔ runs a query and renders the result grid
ℹ pass 5
ℹ fail 0
```

Com um `DRACO_TEST_CONN_ID` sem senha no credential store, o teste falha na etapa de conexão
(estado `error`), como esperado.

Ainda fora da cobertura: SSH/jump host real, as APIs de IA e a execução na CI.

## Checklist transversal

| Cenário | Evidência |
|---|---|
| Nominal contra Postgres real | core e `draco-app` passaram em 04/08/2026 contra PostgreSQL 18.4 |
| Conexão ausente/perdida | fronteira Tauri recusa operação desconectada e volta a executar após reconexão; recuperação após erro SQL também coberta |
| SSH/jump host | suporte permanece coberto pelo `PostgresDriver`; não executado porque o ambiente E2E não possui endpoint SSH configurado |
| Loading, vazio e erro | estados cobertos pelos contratos frontend; vazio de `pg_cron`, activity e locks observado no teste real |
| Operação longa sem bloquear a UI | comandos Tauri são `async`; queries, scripts, `EXPLAIN`, backup e restore registram `operationId` cancelável |
| Mutação perigosa | teste usa schema isolado; UI mantém confirmações para operações destrutivas |
| Segredos e queries em logs | teste usa Secret Service e não registra senha nem conteúdo de credencial |
