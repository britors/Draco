# Draco

Extensão VS Code para explorar bancos de dados, com integração ao Draco Parser.
GitHub: https://github.com/britors/Draco

## Stack

- **Linguagem**: TypeScript
- **Plataforma**: VS Code Extension (webview sidebar)
- **Driver**: `node-postgres` (`pg`) — sem dependência de CLI externo
- **Editor SQL**: Monaco Editor (mesma configuração do SQL Runner)
- **Build**: esbuild

## Arquitetura

Seguir o mesmo padrão do SQL Runner (OpenBase.VsCode):
- `extension.ts` — entry point, providers, commands
- Webview sidebar com IPC via `postMessage({ command, data })`
- `globalState` para histórico e preferências
- `SecretStorage` para senhas de conexão

## Regras de UI — obrigatórias

A UI deve parecer **nativa do VS Code**:
- Usar **somente** variáveis CSS `--vscode-*` para cores, fontes e bordas
- Ícones via **codicons** (`$(icon-name)` em botões, `vscode-icon` em webview)
- **Sem bibliotecas de UI externas** (Bootstrap, Tailwind, Material, etc.)
- Fonte: `var(--vscode-font-family)`, tamanho `var(--vscode-font-size)`
- Inputs: `--vscode-input-background`, `--vscode-input-foreground`, `--vscode-input-border`
- Botões primários: `--vscode-button-background`, `--vscode-button-foreground`
- Listas/grids: `--vscode-list-hoverBackground`, zebra com `--vscode-list-oddRowsBackground`
- Tema sincronizado automaticamente (light/dark via variáveis)

## Webview — regras de escape em template literals

Para evitar SyntaxError em template literals de HTML injetado na webview:
- `\\n` não `\n` dentro de strings em `<script>`
- `/\x3c/g` não `/</g` em expressões regex
- Unicode escapes para caracteres especiais em blocos `<script>`

## Issues e Milestones

Todas as issues estão em https://github.com/britors/Draco/issues

- **v0.1 Foundation** (#2–#7): boilerplate, conexões, SecretStorage, driver pg
- **v0.2 Database Explorer** (#8–#14): TreeView, preview de tabela
- **v0.3 Query Editor** (#15–#19): Monaco, execução, abas, histórico, autocomplete
- **v0.4 Schema Inspection** (#20–#23): DDL, índices, FKs, exportação
- **v0.5 Prisma Integration** (#24–#28): schema.prisma, migrations, db pull
- **v0.6 Polish** (#29–#32): EXPLAIN, configurações, notificações, README

## Diferenças em relação ao SQL Runner

| | SQL Runner | Draco |
|---|---|---|
| Banco | Multi (MSSQL, PG, Oracle) | Multi-DB focus |
| Driver | CLI externo (psql) | node-postgres nativo |
| Conexões | Lidas de appsettings.json | Gerenciadas na UI + SecretStorage |
| Foco | Scaffold + queries | Exploração + Draco Parser |
