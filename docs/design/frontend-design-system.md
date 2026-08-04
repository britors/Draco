# Design system do frontend Draco

Este documento é o catálogo curto do protótipo dark-first. Os tokens vivem no bloco `:root` de
[`styles.css`](../../frontend/dist/styles.css) e devem ser alterados antes de adicionar cores
isoladas a uma tela.

## Tokens

| Grupo | Tokens principais |
|---|---|
| Canvas | `--color-canvas`, `--color-surface`, `--color-surface-raised`, `--color-surface-input` |
| Texto | `--color-text`, `--color-text-muted`, `--color-text-subtle` |
| Ação/estado | `--color-action`, `--color-action-soft`, `--color-success`, `--color-warning`, `--color-danger` |
| Geometria | `--radius-sm`, `--radius-md`, `--radius-lg`, `--space-1`…`--space-4` |
| Foco | `--focus-ring` — nunca remover sem alternativa equivalente |

## Estados obrigatórios

Cada view que espera dados apresenta uma destas superfícies: loading, vazio, sucesso ou erro.
Mutação destrutiva usa a classe `danger` e confirmação textual. O foco usa coral com halo
visível; disabled reduz contraste e remove a affordance de clique sem esconder o controle.

## Componentes e exemplos

- Botões: `.button`, `.button.primary`, `.button.danger`, `.button:disabled`.
- Navegação: `.nav-item` e `.tree-item`, sempre como `button` nativo.
- Abas: `.query-tabs` com `role=tablist` e `aria-selected`.
- Dados: `.result-grid` limita a primeira viewport e informa quando há mais linhas.
- Command palette: `#command-palette`, pesquisa local e Escape para fechar.
- Estado: `errorState()` cria título e explicação sem inserir HTML não confiável.

## Checklist de revisão visual

1. Verificar normal, hover, foco por teclado, disabled e erro em cada componente novo.
2. Verificar a mesma tela em viewport larga, 860px e 560px.
3. Confirmar que textos permanecem legíveis sem depender do estado-dot.
4. Rodar `npm test` e `npm run check`; não usar CDN, `fetch`, storage web ou HTML dinâmico
   não sanitizado.
