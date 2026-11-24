# photocraft-paint (cópia com mudanças do ArtApp)

Cópia do crate `photocraft-paint` do PhotoCraft (https://github.com/storytold/photocraft,
`crates/paint`, revisão 6fdd6bb, MIT OR Apache-2.0, veja `LICENSE-MIT` e `LICENSE-APACHE`). O
`Cargo.toml` da raiz usa esta pasta no lugar do crate do Git. Os exemplos ficaram de fora e o
código passou pelo `cargo fmt` do ArtApp.

Mudanças (procure por "ArtApp" nos comentários):

- `brush.rs`: campos novos em `BrushSettings`, todos opcionais no JSON (pincéis antigos abrem
  iguais): `burnt_edges` (bordas queimadas, 0..1), `glaze` (`Glaze::Normal`, `Uniform`,
  `Intense`), `extra_tips` (mais imagens de ponta) e `tip_random`.
- `render.rs`:
  - pontas múltiplas: `BrushContext::tip_for` escolhe a ponta de cada marca pelo índice dela
    (alternando, ou ao acaso com a semente do pincel);
  - vidrado: `CoverageMap::uniform` guarda o máximo em vez de acumular (Uniforme); Intenso usa
    teto 1 em vez do teto da Transferência;
  - bordas queimadas: a profundidade das marcas (a mesma das Bordas molhadas) marca a borda do
    traço, que é composta em Multiplicar sobre a tinta de baixo no `composite`. As Bordas molhadas
    só usam essa profundidade quando ligadas.
- Testes no ArtApp: `crates/artapp-core/src/engine_tests.rs`. Os 130 testes do próprio crate
  passam (rodar com uma cópia fora do workspace: `cargo test --lib`).

Para atualizar o PhotoCraft: copiar o `crates/paint` da nova revisão e refazer estas mudanças.
