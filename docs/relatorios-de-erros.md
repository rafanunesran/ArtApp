# Relatórios de erro e volta ao projeto

## Como o relatório chega até nós

1. Quando o ArtApp tem um erro (pânico do Rust, contornado ou não) ou fecha de repente, o relatório
   é gravado no aparelho (`crash/`, ver `crates/artapp/src/crash.rs`) e entra na fila `crash/fila/`.
2. Na primeira vez, a janela **Relatório de erros** pergunta se pode enviar automaticamente
   (fica em `prefs.json`, chave `crash_upload`; dá para mudar na mesma janela, menu
   Arquivo › Relatório de erros). Com "Sim", a fila é enviada na hora e de novo a cada 5 min se
   faltar internet (`crates/artapp/src/crash_upload.rs`).
3. O envio é um POST HTTPS para o canal público do ntfy.sh `artapp-relatorios-de-erros-7k2q9m`
   (gratuito, sem conta; guarda 12 h). Vai só: build, sistema (ex. `android aarch64`), tipo,
   mensagem do erro, lugar no código e a pilha. Caminhos de pastas e endereços de arquivos viram
   `<caminho>`; o desenho, o nome do projeto e dados da pessoa não vão.
4. O workflow **Relatórios de erro** (`.github/workflows/crash-reports.yml`, de hora em hora,
   minuto 17) busca o canal, agrupa por tipo de erro (tipo + lugar + mensagem sem números) e:
   - grava `relatorios.json` no ramo `relatorios-de-erros`;
   - abre uma issue por tipo de erro com o rótulo `erro-do-app`, ou comenta as ocorrências novas.
     Feche a issue quando corrigir; se o erro aparecer de novo num build mais novo, ela reabre.

## Como consultar (todo dia)

- Página: https://rafanunesran.github.io/ArtApp/erros/ (filtro por período e tipo; "Ver
  relatório" mostra os 3 últimos completos).
- Issues: https://github.com/rafanunesran/ArtApp/issues?q=label%3Aerro-do-app
- Dados brutos: https://raw.githubusercontent.com/rafanunesran/ArtApp/relatorios-de-erros/relatorios.json
- Para buscar na hora (sem esperar a próxima hora): Actions › Relatórios de erro › Run workflow.

O canal do ntfy.sh é público (o nome está no código), então qualquer um pode postar nele: o
workflow só aceita mensagens que começam com `ARTAPP-CRASH 1`, mostra o texto sempre dentro de
bloco de código e abre no máximo 5 issues novas por execução.

## Voltar ao projeto depois de um erro

`crates/artapp/src/resume.rs`: o app guarda em `sessao.json` qual projeto está aberto e o reabre
ao abrir o app (o salvamento automático o mantém em dia). Se o arquivo sumiu ou não abre, abre o
backup mais recente dele (pasta de backup, inclusive Drive). Se a última tentativa de reabrir
fechou o app (marcador `crash/reabrindo`), pergunta: abrir o backup, tentar de novo ou começar em
branco. Desenhos sem nome continuam voltando pelo arquivo de recuperação.
