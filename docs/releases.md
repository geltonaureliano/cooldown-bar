# CI e releases do NotchUsage

Um único workflow, `.github/workflows/ci-release.yml`, cuida dos testes,
build macOS e publicação no GitHub. A primeira versão é **0.0.1** e a tag é
**v0.0.1**. A interface e a coleta de consumo não são alteradas pelo CI.

## Quando executa

| Evento | Valida e gera instalador | Publica release |
| --- | --- | --- |
| Pull request | Sim, assinatura ad hoc | Nunca |
| Push em outra branch | Sim, assinatura ad hoc | Nunca |
| Push na branch padrão | Sim | Se a versão ainda não foi publicada |
| Execução manual com `publish` desmarcado | Sim, assinatura ad hoc | Nunca |
| Execução manual com `publish` marcado | Sim | Somente na branch padrão e para versão inédita |
| Push de tag | Não inicia outro workflow | A tag é criada pelo próprio CI |

A branch padrão vem da configuração do repositório; não precisa se chamar
`main`. Não há publicação automática a cada commit com o mesmo número:
depois de publicar `v0.0.1`, novos pushes continuam validando, mas só uma nova
versão gera outra release. Executar novamente uma release publicada não troca
seus arquivos.

## Primeiro teste no GitHub

1. Envie o projeto completo ao seu repositório, incluindo `package-lock.json`,
   `src-tauri/Cargo.lock`, os ícones e a pasta `.github`.
2. Se quiser validar antes de publicar, envie primeiro para uma branch que não
   seja a padrão. O CI gera os pacotes como artefatos da execução.
3. Ao enviar/mesclar na branch padrão, o workflow publica `v0.0.1` depois de
   todos os checks. Se a política da organização exigir, autorize GitHub
   Actions e a permissão `contents: write` **do job de release**. Nenhum PAT
   precisa ser cadastrado; o workflow usa o `GITHUB_TOKEN` automático.
4. Também é possível ir a **Actions → CI & Release → Run workflow**, selecionar
   a branch padrão e marcar `publish`. O botão aparece depois que o arquivo do
   workflow estiver na branch padrão.
5. Acompanhe os resumos dos três jobs e baixe o DMG em **Releases**.

O workflow não configura permissões do repositório nem ignora regras de
proteção. Um ruleset que bloqueie criação de tags `v*` pelo GitHub Actions
precisa ser ajustado por um administrador. Recomenda-se proteger a branch
padrão com revisão de PR e exigir os checks **Testes e configuração** e
**macOS · testes e instalador universal**.

## O que é entregue

| Arquivo | Conteúdo |
| --- | --- |
| `NotchUsage_0.0.1_universal.dmg` | Instalador para Apple Silicon e Intel |
| `NotchUsage_0.0.1_universal.app.zip` | Mesmo `.app`, comprimido com `ditto` para preservar o bundle |
| `build-info.json` | Versão, commit, arquiteturas, macOS mínimo, assinatura e hashes dos binários |
| `SHA256SUMS.txt` | SHA-256 dos dois pacotes e dos metadados |

O requisito permanece **macOS 13+**. O CI confere as duas arquiteturas com
`lipo`, a versão no `Info.plist`, a assinatura com `codesign` e a imagem com
`hdiutil verify`. Os testes Rust executam nativamente no runner ARM; o binário
Intel é compilado, mas não é executado nesse runner. Isso não substitui um
teste de instalação/uso em um Mac Intel ou no macOS mínimo suportado.

Os artefatos de cada execução ficam disponíveis por 14 dias. Os arquivos de
uma GitHub Release não dependem dessa retenção. O ZIP externo do artefato de
Actions é apenas um contêiner; dentro dele estão os quatro arquivos acima.

## Próximas versões

Use Node.js 24 (`.node-version`). Execute um dos comandos:

```bash
npm run release:version -- patch
# ou um número explícito:
npm run release:version -- 0.0.2
```

O comando altera somente a versão da aplicação em cinco arquivos:
`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
`src-tauri/Cargo.lock` e `src-tauri/tauri.conf.json`. As versões das dependências
ficam intactas. Também aceita `minor` e `major`. Não cria commit, tag nem faz
push. O protocolo do Codex usa a versão compilada do Cargo, sem texto fixo.

Revise os arquivos, acrescente um resumo opcional em `release-notes/0.0.2.md`,
faça commit (por exemplo, `chore(release): 0.0.2`) e envie por PR. Ao mesclar,
o workflow valida, compila o commit exato e cria a tag correspondente. **Não
use apenas `npm version`**, pois ele não sincroniza Rust e Tauri.

O fluxo cobre versões estáveis `MAJOR.MINOR.PATCH`, sem prefixo `v` nos
manifestos. Pré-releases (`-rc.1`) e incremento automático inferido dos commits
não estão ativados: o número da release continua sendo uma decisão revisável.

## Release notes automáticas

As notas combinam três fontes, sem serviço de IA, chave externa ou acesso a
contas de provedores:

1. **Destaques editoriais opcionais:** `release-notes/<versão>.md`. A versão
   0.0.1 já tem um resumo dos recursos existentes e das limitações dos dados.
2. **Pull requests e colaboradores:** API de notas automáticas do GitHub,
   organizada pelos labels de `.github/release.yml`. Labels são opcionais;
   PRs sem label aparecem em “Outras mudanças”.
3. **Commits:** histórico desde a tag estável anterior alcançável pelo commit.
   Funciona também sem PRs e na primeira release. Se a API de notas falhar,
   este histórico e os destaques continuam disponíveis.

Use mensagens descritivas para obter categorias úteis:

| Mensagem | Categoria |
| --- | --- |
| `feat(ui): adicionar detalhes ao indicador` | Novidades |
| `fix(poller): recuperar consultas após suspensão` | Correções |
| `perf: reduzir leituras em disco` | Desempenho |
| `security: validar caminhos` | Segurança |
| `chore(deps): atualizar Tauri` | Dependências |
| `ci: validar instaladores` | Build e automação |
| `feat!: mudar formato da configuração` | Mudanças que exigem atenção |

`BREAKING CHANGE:` no corpo também destaca mudanças incompatíveis. Isso é
classificação de mensagens/labels, **não uma análise semântica dos diffs**.
Use o resumo editorial para explicar impacto e migrações. O detalhamento por
commit fica recolhido e limita a exibição a 200 entradas, com link ao histórico
completo. `skip-changelog` exclui PRs da seção gerada pelo GitHub, não remove
commits do histórico técnico. Não coloque segredos nas mensagens ou notas.

## Assinatura e notarização Apple

### Sem conta Apple Developer

O CI funciona sem secrets Apple: cria pacotes com **assinatura ad hoc** e
avisa nas notas que **não são notarizados**. Isso serve para testes, mas o
Gatekeeper pode bloquear a abertura. Não há promessa de instalação sem avisos.
Siga as opções normais de Privacidade e Segurança somente se confiar na origem;
o projeto não desativa proteções do macOS.

### Distribuição com Developer ID

Em **Settings → Secrets and variables → Actions**, crie a variável de
repositório **`APPLE_SIGNING_ENABLED=true`** e os seis secrets:

| Secret | Valor |
| --- | --- |
| `APPLE_CERTIFICATE` | Certificado **Developer ID Application** em `.p12`, codificado em base64 |
| `APPLE_CERTIFICATE_PASSWORD` | Senha definida ao exportar o `.p12` |
| `APPLE_SIGNING_IDENTITY` | Identidade completa: `Developer ID Application: Seu Nome (TEAMID)` |
| `APPLE_ID` | E-mail da conta Apple |
| `APPLE_PASSWORD` | **Senha específica de aplicativo**, nunca a senha principal da conta |
| `APPLE_TEAM_ID` | Team ID da conta de desenvolvedor |

Para preparar o certificado, em uma pasta privada fora do repositório:

```bash
openssl base64 -A -in certificado.p12 -out certificado-base64.txt
```

Copie o conteúdo para o secret e mantenha esses arquivos fora do Git. O Tauri
importa o certificado para assinatura e submete a notarização; depois o CI
confere o ticket com `stapler` e a aceitação com `spctl`. Não há fallback
silencioso para ad hoc se a assinatura foi habilitada e alguma credencial
está ausente/inválida. A primeira aprovação da Apple pode demorar além do
timeout de 90 minutos; nesse caso, aguarde a aprovação e execute novamente.

Somente builds destinados a uma release na branch padrão recebem esses
secrets. PRs, outras branches e execuções manuais sem publicação continuam
ad hoc. O aplicativo usa APIs privadas do macOS e este fluxo **não publica na
Mac App Store**. Assinatura Apple e notarização não foram exercitadas sem as
suas credenciais.

Referência: [assinatura e notarização no Tauri](https://v2.tauri.app/distribute/sign/macos/).

## Proteções e recuperação de falhas

- Actions fixadas por SHA completo; Dependabot propõe atualizações semanais.
- `npm ci`, `cargo --locked` e Rust 1.92.0 fixado evitam resolução acidental de
  novas dependências. Node 24 e Python 3.13 recebem atualizações de patch.
- `actionlint` é baixado em versão fixa e seu SHA-256 é verificado antes de executar.
- O cache Cargo é salvo somente na branch padrão; pacotes universais, certificados
  e secrets não são incluídos nele. O cache npm guarda downloads, não `node_modules`.
- Testes, TypeScript, rustfmt e Clippy bloqueiam a publicação. O teste opt-in
  que usa uma conta real do Codex fica ignorado no CI.
- Somente o job final tem `contents: write`; ele não instala dependências npm
  nem executa scripts de terceiros do projeto para publicar.
- Antes de qualquer escrita remota, os quatro arquivos precisam passar na
  conferência de tamanho, nomes, commit, arquiteturas e hashes.
- A tag aponta para o SHA testado e nunca é movida. Uploads acontecem em um
  rascunho identificado pelo CI; a release só é publicada após verificar os
  hashes retornados pelo GitHub. Compatível com releases imutáveis.
- Uma falha de upload preserva tag e rascunho. Use **Re-run failed jobs** na
  mesma execução/commit para retomar. Arquivos iguais são reutilizados;
  uploads incompletos podem ser substituídos **apenas nesse rascunho**.
- Rascunhos criados manualmente não são sobrescritos. Após publicar, qualquer
  correção de binário, notas ou assinatura deve seguir uma nova versão.

Se surgir `Resource not accessible by integration`, confira a política de
permissões da organização/repositório. Se uma tag já aponta para outro commit,
suba a versão; não force a movimentação da tag. Se o artefato expirou, execute
o workflow inteiro novamente no mesmo commit, quando ainda houver rascunho.

Referências: [notas automáticas do GitHub](https://docs.github.com/en/repositories/releasing-projects-on-github/automatically-generated-release-notes),
[API de releases](https://docs.github.com/en/rest/releases/releases),
[runners macOS](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
e [builds Tauri no GitHub](https://v2.tauri.app/distribute/pipelines/github/).

## Verificação local

```bash
npm ci
npm run release:check
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

No macOS, com Xcode Command Line Tools e o projeto em um checkout Git limpo:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
SIGNING_ENABLED=false node scripts/ci/macos.mjs build /tmp/notchusage-distribution
```

O diretório de distribuição precisa estar vazio. Este comando apenas gera e
confere os arquivos locais; não abre o app nem publica nada. Os testes dos
scripts usam repositórios temporários, fixtures e uma API simulada; nunca
criam tags/releases remotas. A execução real no GitHub e a instalação final
continuam sendo verificações necessárias antes de anunciar uma versão.
