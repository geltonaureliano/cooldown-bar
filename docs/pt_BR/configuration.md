# Configuração

O Cooldown Bar lê `~/.cooldown-bar/config.json`. Durante uma atualização, `~/.notchusage/config.json` continua sendo usado quando o arquivo novo não existe e o arquivo antigo existe.

Todas as propriedades são opcionais. Valores inválidos são substituídos por padrões seguros. Um arquivo malformado não é sobrescrito ao salvar a posição.

## Exemplo

```json
{
  "edge": "right",
  "barWidth": 62,
  "concaveRadius": 31,
  "topOffset": null,
  "edgeInset": 0,
  "ringDiameter": 38,
  "ringLineWidth": 3,
  "itemGap": 28,
  "refreshSeconds": 10,
  "staleAfterSeconds": 120,
  "showClaude": true,
  "showCodex": true,
  "customCommand": null,
  "customTitle": "Custom",
  "claudeColor": "#FF5F2E",
  "codexColor": "#00E07A",
  "customColor": "#E8E80A"
}
```

## Propriedades de layout

`edge` aceita `left` ou `right`.

`topOffset` aceita um número de pontos da tela ou `null`. O padrão `null` posiciona o painel abaixo da área atual da barra de menus. O valor zero pode deixar o conteúdo por baixo da barra de menus do macOS.

`barWidth`, `concaveRadius`, `edgeInset`, `ringDiameter`, `ringLineWidth` e `itemGap` controlam a geometria. Valores inseguros são limitados.

## Propriedades de coleta

`refreshSeconds` usa 10 por padrão e fica entre 5 e 3600 segundos. Falhas aplicam um intervalo progressivo antes de tentar novamente.

`staleAfterSeconds` usa 120 por padrão e fica entre 30 e 86400 segundos. Uma leitura envelhece conforme o instante da medição, não conforme a leitura mais recente do arquivo.

`showClaude` e `showCodex` controlam os provedores integrados.

## Provedor personalizado

`customCommand` pode conter um comando shell que imprime um objeto JSON na saída padrão e termina com sucesso em até três segundos.

```json
{
  "percent": 52,
  "resets_at": 1756400000,
  "label": "Session",
  "secondary_percent": 11,
  "secondary_label": "Weekly"
}
```

`percent` é obrigatório. As outras propriedades são opcionais. O Cooldown Bar limita a saída capturada e encerra o grupo de processos depois do tempo máximo.

## Ícones

Coloque arquivos PNG em `~/.cooldown-bar/icons` com os nomes `claude.png`, `codex.png` ou `custom.png`.

Arquivos antigos em `~/.notchusage/icons` continuam disponíveis durante a atualização. Os novos arquivos têm prioridade.
