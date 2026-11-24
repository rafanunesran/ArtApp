# Site do ArtApp

Página de apresentação publicada em https://rafanunesran.github.io/ArtApp/ pelo workflow
`.github/workflows/pages.yml`. O visual foi criado no Lovable e trazido para cá como um projeto
Vite + React + Tailwind estático.

```sh
cd site
npm install
npm run dev      # http://localhost:5173/ArtApp/
npm run build    # gera dist/
```

O botão "Baixar APK" aponta para `releases/latest/download/ArtApp.apk`, que o workflow de CI
publica a cada push na `main`.

## Página de pincéis (`/pinceis/`)

`pinceis/index.html` + `src/Brushes.tsx` lista pincéis gratuitos compatíveis (`src/brushes-data.ts`).
Para pôr ou tirar um pacote, edite só `brushes-data.ts`: o botão **Download** leva à página de
download oficial do autor (itch.io `/purchase`, Gumroad ou myPhotoshopBrushes) e **Ver prévias**
à página do pacote. O ArtApp abre essa página em **Mais pincéis…** e instala sozinho os
`.sut/.abr/.zip` que aparecerem em Downloads depois disso (`crates/artapp/src/brush_store.rs`).
Botões de download do site usam o componente `DownloadButton` (`src/Layout.tsx`).
