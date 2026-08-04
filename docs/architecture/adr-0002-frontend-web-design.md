# ADR-0002: arquitetura do frontend web e design system

- Status: aceito para o primeiro corte Tauri
- Data: 2026-08-03
- Escopo: shell visual, navegação e componentes compartilhados do frontend

## Decisão

O primeiro corte navegável usa HTML semântico, CSS local e módulos JavaScript sem runtime
externo. O artefato fica em `frontend/dist` porque o Tauri carrega diretamente essa árvore e
os testes de contrato podem validá-la sem instalar uma cadeia de dependências no pacote final.

Se o produto precisar de dezenas de telas com estado compartilhado, a evolução aprovada é
TypeScript + Vite + React, com componentes ainda emitidos como assets locais. Essa migração
fica isolada no frontend e não altera os comandos nem os DTOs de `draco-app`. CodeMirror 6 é a
opção preferida para o editor SQL nessa segunda etapa; Monaco só será escolhido se a análise de
bundle e acessibilidade justificar o peso adicional.

## Trade-offs

| Decisão | Benefício | Custo aceito |
|---|---|---|
| HTML/CSS/JS no protótipo | zero CDN, bundle mínimo e inspeção simples | menos abstrações reutilizáveis |
| Tauri `invoke` como única ponte | superfície local e contrato explícito | cada comando precisa de DTO correspondente |
| estado local por tela | menor acoplamento e inicialização rápida | sincronização futura exigirá uma store |
| CSS tokens sem biblioteca visual | controle preciso do dark-first | componentes precisam manter consistência manual |
| CodeMirror 6 como próximo editor | modular e menor que Monaco | requer pipeline Vite/TypeScript |

## Layout aprovado

```text
shell
├── sidebar fixa/responsiva: marca, navegação e saúde do backend
└── workspace
    ├── topbar: breadcrumb/título, contagem e command palette
    └── view
        ├── abas de query quando aplicável
        └── painéis de conteúdo, resultado ou estado
```

O tema escuro é o padrão e continua sendo o que abre no primeiro uso. O fundo usa `#08090d`,
superfícies em camadas `#12141c`/`#1c202b`, ação coral `#d96558` e destaque `#ffb09a`. Um tema
claro (`html[data-theme="light"]`) e cinco cores de destaque selecionáveis (coral, azul, verde,
roxo, âmbar) foram adicionados na tela de Preferências e persistidos em `AppSettings`; a paleta
clara reusa os mesmos tokens semânticos (`--color-canvas`, `--color-surface`, …), então nenhum
componente precisou de uma segunda implementação.

## Componentes do contrato visual

`Button`, `Dialog`, `CommandPalette`, `DataGrid`, `Tabs`, `Toast`, `Tree`, `SplitPane`,
`EmptyState` e `ConfirmDestructive` são os nomes de referência. O protótipo já materializa
`Button`, `CommandPalette`, `DataGrid`, `Tabs`, `Tree` e `EmptyState`; confirmações destrutivas
usam o diálogo nativo enquanto o componente dedicado não for necessário.

Todos os componentes devem:

- expor foco visível e estado `disabled` sem depender apenas de cor;
- manter texto de ação e erro compreensível sem consultar o console;
- aceitar teclado e preservar a ordem natural de tabulação;
- usar somente assets e fontes disponíveis localmente.

## Acessibilidade e validação

- [x] documento declara idioma `pt-BR` e usa headings hierárquicos;
- [x] command palette expõe `role=dialog`, foco inicial, Escape e atalho `Ctrl/⌘+K`;
- [x] navegação, abas, árvores e ações têm elementos nativos focáveis;
- [x] loading, vazio, sucesso e erro têm superfície textual explícita;
- [x] foco, hover e disabled têm estados CSS distintos;
- [x] layout se adapta abaixo de 860px e 560px;
- [x] testes de contrato verificam tokens, estados e ausência de recursos remotos;
- [ ] revisão com leitor de tela e contraste automatizado em uma build empacotada.

## Fora de escopo

Anexos no Assistente, sincronização cross-window e biblioteca de componentes publicada ficam para
depois da paridade funcional. A referência Kraken Studio é somente estética; nenhum fluxo,
identidade ou código externo é copiado.

Tema claro saiu da lista de "fora de escopo" em 2026-08-04: ver tela de Preferências
(`view-preferences` em `frontend/dist/index.html`) e a superfície correspondente na matriz de
paridade (`docs/migration/rust-gtk-parity.md`, milestone M8).
