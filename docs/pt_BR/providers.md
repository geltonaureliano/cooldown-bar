# Provedores e fontes de dados

O Cooldown Bar trata cada leitura como uma medição com fonte, horário, estado de verificação e possível horário de renovação.

## Claude Code

O Cooldown Bar primeiro consulta as informações compatíveis no Claude CLI instalado. A captura opcional da statusline pode gravar um registro limitado em `~/.cooldown-bar/claude.json`.

O script recebe JSON pela entrada padrão, grava de forma atômica com permissões privadas e não interrompe a statusline do terminal quando a captura falha.

Para ativar a captura, adicione um comando de statusline em `~/.claude/settings.json` e substitua o caminho do repositório pelo caminho absoluto no seu Mac.

```json
{
  "statusLine": {
    "type": "command",
    "command": "/caminho/absoluto/para/cooldown-bar/scripts/claude-statusline.sh"
  }
}
```

O Claude Code aceita um comando de statusline. Se você já usa um, chame `scripts/claude-statusline.sh --capture-only` pelo wrapper existente e mantenha seu renderizador atual como comando final.

## Codex

O Cooldown Bar mantém uma conexão JSON RPC limitada com o app server do Codex e solicita os limites da conta. A conexão permanece aberta para conciliar notificações e novas consultas.

Quando o acesso ao vivo não funciona, registros recentes das sessões locais do Codex podem fornecer um valor não verificado. Um registro não comprova qual conta está ativa, então a leitura alternativa não vira um valor principal confiável.

## Comando personalizado

Um provedor personalizado executa o comando definido pelo usuário. O comando é responsável por autenticação, acesso à rede, precisão da saída e proteção de segredos.

## Atualidade e confiabilidade

1. Cada provedor trabalha de forma independente.

2. As consultas têm limites de tempo e tamanho.

3. Falhas aumentam o intervalo de repetição.

4. Um valor armazenado mantém o horário original da medição.

5. Horários desconhecidos ou vencidos tornam a leitura antiga.

6. Um valor antigo ou não verificado não aparece como percentual atual confiável.

7. A coleta pausa durante o movimento e resultados ainda em processamento são descartados.

Os serviços dos provedores podem atrasar ou armazenar dados em cache. O Cooldown Bar não garante precisão instantânea de cobrança.
