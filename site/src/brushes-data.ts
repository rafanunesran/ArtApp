// Pincéis gratuitos que o ArtApp instala (Pincéis › Instalar pincéis…, ou sozinho quando
// baixados a partir do app). Lista montada em 03/11/2026 visitando a página de cada pacote.
// Só entram pacotes gratuitos (ou "pague quanto quiser" aceitando zero). Os arquivos ficam
// com os autores: o botão Download leva à página de download oficial de cada um.

export type Host = "itch" | "gumroad" | "mpb";

export type BrushPack = {
  name: string;
  author: string;
  format: "Clip Studio (.sut)" | "Photoshop (.abr)";
  count: string;
  kind: string;
  desc: string;
  license: string;
  /** Página oficial, com as prévias. */
  page: string;
  /** Onde fica o botão de download oficial. */
  download: string;
  host: Host;
};

export const hostHelp: Record<Host, { name: string; how: string }> = {
  itch: {
    name: "itch.io",
    how: "Toque em Download, deixe o valor em 0 (ou contribua) e escolha “No thanks, just take me to the downloads”.",
  },
  gumroad: {
    name: "Gumroad",
    how: "Digite 0 no preço, toque em “I want this!”, informe um e-mail e baixe o arquivo.",
  },
  mpb: {
    name: "myPhotoshopBrushes",
    how: "Toque no botão de download da página. Sem cadastro.",
  },
};

export const packs: BrushPack[] = [
  {
    name: "Brush Pack 2: the painting pack",
    author: "crvptozoology",
    format: "Clip Studio (.sut)",
    count: "13 pincéis",
    kind: "Pintura",
    desc: "Guache, liners de detalhe, pincéis de rastelo e redondos para renderizar.",
    license: "Livre para usar; a autora pede uma contribuição em trabalhos comerciais.",
    page: "https://crvptozoology.itch.io/brush-pack-2",
    download: "https://crvptozoology.itch.io/brush-pack-2/purchase",
    host: "itch",
  },
  {
    name: "Amika's Clip Studio Pencilbox",
    author: "amika",
    format: "Clip Studio (.sut)",
    count: "cerca de 13 pincéis",
    kind: "Esboço e arte-final",
    desc: "Lapiseira, lápis de desenho e de cor, fineliners, marcadores, blender de álcool e canetas de nanquim.",
    license: "Uso comercial permitido; não pode revender os pincéis.",
    page: "https://amika.itch.io/amikas-clip-studio-pencilbox",
    download: "https://amika.itch.io/amikas-clip-studio-pencilbox/purchase",
    host: "itch",
  },
  {
    name: "5x free brushes for Clip Studio Paint",
    author: "ED_of_all",
    format: "Clip Studio (.sut)",
    count: "5 pincéis",
    kind: "Pintura",
    desc: "Cinco pincéis de pintura num .zip pequeno (3,9 MB).",
    license: "Uso pessoal e comercial, pode modificar; não pode redistribuir nem vender.",
    page: "https://ed-of-all.itch.io/5x-free-brushes-for-clip-studio-paint",
    download: "https://ed-of-all.itch.io/5x-free-brushes-for-clip-studio-paint/purchase",
    host: "itch",
  },
  {
    name: "Natural wood and burned brush pack",
    author: "Axe Swing",
    format: "Clip Studio (.sut)",
    count: "5 pincéis",
    kind: "Texturas",
    desc: "Veios de madeira (2 versões) e três texturas de madeira queimada (yakisugi).",
    license: "Gratuito; a página não informa a licença.",
    page: "https://axeswing.itch.io/natural-wood-and-burned-brush-pack",
    download: "https://axeswing.itch.io/natural-wood-and-burned-brush-pack/purchase",
    host: "itch",
  },
  {
    name: "Basic Brush Pack (free)",
    author: "Lonelyshootingstar",
    format: "Clip Studio (.sut)",
    count: "pacote básico",
    kind: "Esboço e arte-final",
    desc: "Pincéis básicos para esboço, arte-final e detalhes.",
    license: "Gratuito; confira os termos na página.",
    page: "https://lonelyshootingstar.gumroad.com/l/FREEBRUSHPACK",
    download: "https://lonelyshootingstar.gumroad.com/l/FREEBRUSHPACK",
    host: "gumroad",
  },
  {
    name: "eXpressure NT (free)",
    author: "zhillustrator",
    format: "Clip Studio (.sut)",
    count: "pacote",
    kind: "Pintura",
    desc: "Versões gratuitas dos pincéis eXpressure que não dependem da inclinação da caneta.",
    license: "Gratuito; confira os termos na página.",
    page: "https://zhillustrator.gumroad.com/l/expressureNTfree",
    download: "https://zhillustrator.gumroad.com/l/expressureNTfree",
    host: "gumroad",
  },
  {
    name: "Ahmed Aldoori Photoshop Brushes",
    author: "Ahmed Aldoori",
    format: "Photoshop (.abr)",
    count: "pacote",
    kind: "Pintura",
    desc: "Os pincéis que o artista usa nas aulas de pintura digital. Grátis: é só pôr 0 no preço.",
    license: "Gratuito; confira os termos na página.",
    page: "https://medders.gumroad.com/l/YUDSX",
    download: "https://medders.gumroad.com/l/YUDSX",
    host: "gumroad",
  },
  {
    name: "Free Concept Art Brushes",
    author: "velgatheobserver",
    format: "Photoshop (.abr)",
    count: "14 pincéis",
    kind: "Pintura",
    desc: "Pincéis de concept art feitos pelo próprio autor.",
    license: "Gratuito; confira os termos na página.",
    page: "https://velgatheobserver.gumroad.com/l/conceptartfree",
    download: "https://velgatheobserver.gumroad.com/l/conceptartfree",
    host: "gumroad",
  },
  {
    name: "Free Brushes vol. 1",
    author: "Oleg Kalaida",
    format: "Photoshop (.abr)",
    count: "7 pincéis e variações",
    kind: "Pintura",
    desc: "Sete pincéis com variações, em .abr e .png (o .png também vira ponta de pincel no ArtApp).",
    license: "Gratuito para uso pessoal; confira o uso comercial na página.",
    page: "https://vailf.gumroad.com/l/FreeBrushesvol1",
    download: "https://vailf.gumroad.com/l/FreeBrushesvol1",
    host: "gumroad",
  },
  {
    name: "64 Free Watercolor Splatter Brushes",
    author: "myPhotoshopBrushes",
    format: "Photoshop (.abr)",
    count: "64 carimbos",
    kind: "Aquarela",
    desc: "Respingos, escorridos e manchas de aquarela e nanquim. Arquivo grande (194 MB).",
    license: "Livre para uso comercial; não pode redistribuir os arquivos.",
    page: "https://myphotoshopbrushes.com/resources/4203/64-free-watercolor-splatter-brushes",
    download: "https://myphotoshopbrushes.com/resources/4203/64-free-watercolor-splatter-brushes",
    host: "mpb",
  },
  {
    name: "16 Free Graphite & Pencil Brushes",
    author: "myPhotoshopBrushes",
    format: "Photoshop (.abr)",
    count: "16 pincéis",
    kind: "Esboço",
    desc: "Grafite e lápis para esboço.",
    license: "Gratuito; a licença de cada pacote está na página.",
    page: "https://myphotoshopbrushes.com/resources/4275/16-free-graphite-pencil-photoshop-brushes",
    download: "https://myphotoshopbrushes.com/resources/4275/16-free-graphite-pencil-photoshop-brushes",
    host: "mpb",
  },
  {
    name: "24 Free Gouache Brushes",
    author: "myPhotoshopBrushes",
    format: "Photoshop (.abr)",
    count: "24 pincéis",
    kind: "Pintura",
    desc: "Texturas de guache para pintura.",
    license: "Gratuito; a licença de cada pacote está na página.",
    page: "https://myphotoshopbrushes.com/resources/4274/24-free-gouache-photoshop-brushes",
    download: "https://myphotoshopbrushes.com/resources/4274/24-free-gouache-photoshop-brushes",
    host: "mpb",
  },
  {
    name: "12 Free Distressed Halftone Brushes",
    author: "myPhotoshopBrushes",
    format: "Photoshop (.abr)",
    count: "12 carimbos",
    kind: "Quadrinhos",
    desc: "Retículas gastas para quadrinhos e pôsteres.",
    license: "Gratuito; a licença de cada pacote está na página.",
    page: "https://myphotoshopbrushes.com/resources/4278/12-free-distressed-halftone-photoshop-brushes",
    download: "https://myphotoshopbrushes.com/resources/4278/12-free-distressed-halftone-photoshop-brushes",
    host: "mpb",
  },
];
