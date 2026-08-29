# Arquitetura

O Cooldown Bar combina uma interface React com um host Rust e AppKit por meio do Tauri.

## Interface

A camada React desenha a barra, os detalhes do provedor, o menu de contexto e o estado líquido durante o arraste. O movimento usa transformações em uma superfície pequena e respeita a preferência Reduzir Movimento do macOS.

A interface recebe snapshots versionados do Tauri. Atualizações visuais de um segundo avançam contagens regressivas e o estado de atualidade sem fingir que houve uma nova medição do provedor.

## Host Rust

A camada Rust controla configuração, geometria das telas, painel nativo, rastreamento do ponteiro, trabalhadores dos provedores, limites dos processos e observadores do ciclo de vida.

A integração AppKit cria um painel acessório sem ícone no Dock. Mudanças de tela, repouso, retorno e encerramento são tratados sem bloquear o ciclo principal da interface.

## Trabalhadores dos provedores

Cada provedor possui seu próprio trabalhador e estado de repetição. Pedidos manuais de atualização são agrupados. Resultados pertencem a uma geração de configuração para impedir que uma resposta antiga sobrescreva o estado atual.

Processos filhos têm limites de tempo e saída. Os descendentes de um provedor personalizado são encerrados como um grupo quando o prazo termina.

## Persistência

Mudanças de posição usam um arquivo temporário, sincronização e renomeação atômica. O aplicativo mantém um bloqueio na pasta atual e também bloqueia a pasta antiga quando ela existe, o que impede painéis duplicados durante uma atualização.

O repositório ignora artefatos de compilação, dependências instaladas, estado local de desenvolvimento, schemas gerados pelo Tauri, material de assinatura e metadados do sistema operacional.
