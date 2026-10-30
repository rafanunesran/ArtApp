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
