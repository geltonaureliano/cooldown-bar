# Releases

O workflow em `.github/workflows/ci-release.yml` valida o projeto, compila um aplicativo universal para macOS, confere o pacote e pode publicar uma GitHub Release.

## Validação

Cada pull request e envio para branch executa testes JavaScript, testes Python da statusline, compilação TypeScript, build web, formatação Rust, Clippy, testes Rust e validação do workflow.

O job macOS compila binários Apple Silicon e Intel. Ele confere as duas arquiteturas, a versão do aplicativo, a assinatura, o DMG, os hashes e a procedência do build.

## Publicação

Um envio para a branch padrão pode publicar a versão dos manifestos quando a tag ainda não existe. Uma execução manual só publica a partir da branch padrão. Releases publicadas e tags existentes nunca são substituídas.

O job de release recebe somente o artefato verificado do build correspondente. Ele cria um rascunho, envia todos os arquivos, confere os hashes remotos e publica apenas depois que toda verificação termina.

## Notas da release

As notas combinam um arquivo opcional em `release-notes/<versão>.md`, notas de pull requests do GitHub e mensagens de commits classificadas. O gerador não depende de serviço externo de IA nem de credenciais de provedores.

Use mensagens Conventional Commits claras, como `feat(ui): improve liquid motion`, `fix(poller): recover after wake` ou `docs: clarify installation`.

## Atualizar a versão

```bash
npm run release:version -- patch
```

Também é possível informar uma versão exata.

```bash
npm run release:version -- 0.0.2
```

O script atualiza os manifestos npm, Cargo e Tauri em conjunto. Ele não cria commit, tag nem envia alterações.

## Assinatura Apple

Sem secrets Apple, o CI gera uma compilação de teste com assinatura ad hoc. Com `APPLE_SIGNING_ENABLED=true` e as credenciais Developer ID, a release é assinada, enviada para notarização, anexada ao ticket e verificada.

Os secrets necessários são `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` e `APPLE_TEAM_ID`.
