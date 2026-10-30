# winit 0.30.13 (cópia com correção)

Cópia do `winit` 0.30.13 publicado no crates.io (https://github.com/rust-windowing/winit,
Apache-2.0, veja `LICENSE`) usada pelo `[patch.crates-io]` do `Cargo.toml` da raiz. Os exemplos,
testes e a pasta `docs/` foram tirados (não entram na compilação).

Única mudança, só no Android: o winit transforma todo `MotionEvent` em `Touch` e joga fora o tipo
de ferramenta e os botões, então o app não tinha como saber do botão lateral nem da ponta de
borracha da caneta.

- `src/platform/android.rs`: módulo `stylus` com `pointer_info(id)`, que diz se o toque com aquele
  id veio de caneta, da ponta de borracha, se o botão lateral estava apertado e se o sistema
  marcou o toque como palma.
- `src/platform_impl/android/mod.rs`: grava essas informações a cada `MotionEvent` e trata as
  ações 211/212/213 que aparelhos Samsung antigos usam para a S Pen com o botão apertado.

Remover esta pasta e o patch quando o winit publicado expuser o tipo de ferramenta e os botões
da caneta nos eventos de toque.
