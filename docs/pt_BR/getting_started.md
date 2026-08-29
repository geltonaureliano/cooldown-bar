# Primeiros passos

## Requisitos

O Cooldown Bar requer macOS 13 ou mais recente. O aplicativo é universal e funciona em Macs Apple Silicon e Intel.

As leituras do Claude Code e do Codex exigem que a ferramenta de linha de comando ou o aplicativo correspondente esteja instalado e autenticado. Um provedor indisponível não aparece no mostrador principal.

## Instalar uma release

1. Abra a página de [GitHub Releases](https://github.com/geltonaureliano/cooldown-bar/releases).

2. Baixe `Cooldown_Bar_<versão>_universal.dmg`.

3. Abra a imagem e mova o Cooldown Bar para Aplicativos.

4. Inicie o Cooldown Bar por Aplicativos.

Uma compilação com assinatura ad hoc pode acionar o Gatekeeper. Abra Ajustes do Sistema, depois Privacidade e Segurança, somente quando confiar na release e no repositório de origem.

## Compilar o projeto

Instale Node.js 24, Rust 1.92, Xcode Command Line Tools e os alvos Apple usados na compilação universal.

```bash
npm ci
npm test
npm run build
npm run tauri dev
```

Para gerar e validar os pacotes locais:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
SIGNING_ENABLED=false node scripts/ci/macos.mjs build /tmp/cooldown-bar-distribution
```

## Mover a barra

Pressione e arraste o painel. Depois do limite mínimo de movimento, ele se contrai e vira uma esfera líquida. Solte perto de uma lateral para encaixar. Solte longe das laterais para manter a esfera flutuante.

A coleta de uso pausa enquanto a esfera está solta. Os provedores continuam contabilizando o uso normalmente. Apenas as consultas do Cooldown Bar ficam pausadas.

## Usar o menu de contexto

Clique com o botão direito para atualizar o uso, recarregar a configuração, encaixar na lateral mais próxima ou sair.
