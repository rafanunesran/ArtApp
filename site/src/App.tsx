import { useEffect, useState } from "react";
import {
  Brush, Layers, PenTool, PaintBucket, Undo2, ImageDown, Download,
  Github, Smartphone, FolderOpen, ShieldCheck, Sparkles, Terminal, Info, Maximize2,
  Waves, Blend, SquareDashed, SlidersHorizontal, Monitor, Laptop, Images,
} from "lucide-react";

const REPO = "https://github.com/rafanunesran/ArtApp";
const LATEST = `${REPO}/releases/latest/download`;
const APK = `${LATEST}/ArtApp.apk`;
const base = import.meta.env.BASE_URL;

type Rel = { name: string; date: string; size?: number };

function useRelease() {
  const [r, setR] = useState<Rel | null>(null);
  useEffect(() => {
    fetch("https://api.github.com/repos/rafanunesran/ArtApp/releases/latest")
      .then((res) => (res.ok ? res.json() : Promise.reject()))
      .then((d) => {
        const apk = (d.assets ?? []).find((a: { name: string }) => a.name === "ArtApp.apk");
        setR({ name: d.name || d.tag_name, date: d.published_at, size: apk?.size });
      })
      .catch(() => {});
  }, []);
  return r;
}

const features = [
  { icon: Brush, t: "Pincéis profissionais", d: "Caneta G, lápis, marcador, aquarela, aerógrafo e os presets do PhotoCraft, com pressão, estabilizador e editor completo para criar os seus." },
  { icon: Blend, t: "Misturar e retocar", d: "Desfocar, dedo, clarear, escurecer, esponja e carimbo, para fundir cores e acertar detalhes." },
  { icon: Waves, t: "Liquefazer", d: "Empurrar, deslocar, torcer, contrair, inflar, suavizar e restaurar, direto na camada." },
  { icon: PenTool, t: "Vetor pro", d: "Todas as ferramentas do VectorCraft: caneta Bézier, edição de pontos, texto, formas, espiral, grades e seleção direta." },
  { icon: Layers, t: "Camadas como no Clip Studio", d: "26 modos de mesclagem, bloquear transparência, recorte na camada abaixo, camada de referência e de rascunho." },
  { icon: Sparkles, t: "Simetria e degradê", d: "Régua de simetria vertical, horizontal, de quatro lados ou diagonal, e degradê linear ou radial." },
  { icon: SquareDashed, t: "Seleção", d: "Retangular, elíptica, laço e varinha mágica. Pincel, balde e filtros só agem dentro dela." },
  { icon: SlidersHorizontal, t: "Filtros e correção de cor", d: "Quase 30 filtros do PhotoCraft (desfoques, nitidez, mosaico, pintura a óleo, ondulação…), transformar camada e correção de cor." },
  { icon: PaintBucket, t: "Balde, conta-gotas e borracha", d: "O balde enxerga todas as camadas ou só a de referência, com ajuste de tolerância." },
  { icon: Undo2, t: "Desfazer, zoom e giro", d: "100 passos de desfazer, pinça com dois dedos, girar e espelhar a vista." },
  { icon: ImageDown, t: "Exportar PNG", d: "Papel branco ou transparente, salvo em Pictures/ArtApp no Android." },
];

const gallery = [
  { img: "exemplo-vetor.png", t: "Vetor pro", d: "Seleção direta mostrando os pontos das montanhas, feitas com a caneta Bézier." },
  { img: "exemplo-liquefazer.png", t: "Liquefazer", d: "As nuvens em aquarela empurradas e torcidas numa camada raster." },
  { img: "exemplo-simetria.png", t: "Simetria", d: "Mandala desenhada com a régua de quatro lados sobre um degradê radial." },
];

const desktop = [
  { icon: Monitor, t: "Windows 10/11", file: "ArtApp-Windows-x64.zip", d: <>Extraia e abra <b>ArtApp.exe</b>. No aviso do SmartScreen: Mais informações › Executar assim mesmo.</> },
  { icon: Terminal, t: "Linux x64", file: "ArtApp-Linux-x64.tar.gz", d: <>Extraia e rode <b>./install.sh</b> (põe no menu) ou <b>./artapp</b>.</> },
  { icon: Laptop, t: "macOS (Apple Silicon)", file: "ArtApp-macOS-AppleSilicon.zip", d: <>Para M1, M2, M3… Arraste o ArtApp para Aplicativos e, na primeira vez, abra com botão direito › Abrir.</> },
  { icon: Laptop, t: "macOS (Intel)", file: "ArtApp-macOS-Intel.zip", d: <>Igual ao de cima, para Macs com processador Intel.</> },
];

const steps = [
  { icon: Download, t: "Baixe o APK no celular", d: <>Toque em <a className="text-primary underline" href={APK}>baixar ArtApp.apk</a> ou veja todas as versões em <a className="text-primary underline" href={`${REPO}/releases`}>Releases</a>.</> },
  { icon: FolderOpen, t: "Abra o arquivo baixado", d: <>Pelas notificações ou no app <b>Arquivos › Downloads</b>.</> },
  { icon: ShieldCheck, t: "Permita instalar apps desconhecidos", d: <><b>Configurações › Apps › Acesso especial › Instalar apps desconhecidos</b>, e libere o navegador ou o app de arquivos.</> },
  { icon: Sparkles, t: "Instale e desenhe", d: <>Toque em Instalar, abra o ArtApp e comece. Para atualizar, baixe o APK de novo e toque em <b>Atualizar</b>: instala por cima, sem desinstalar.</> },
];

export default function App() {
  const rel = useRelease();
  return (
    <div className="min-h-screen font-sans">
      <header className="fixed inset-x-0 top-0 z-50 border-b border-border bg-background/80 backdrop-blur-lg">
        <nav className="mx-auto flex h-16 max-w-6xl items-center justify-between px-5">
          <a href="#" className="flex items-center gap-2 font-display text-lg font-bold">
            <span className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground"><Brush size={18} /></span>
            ArtApp
          </a>
          <div className="flex items-center gap-5 text-sm text-muted-foreground">
            <a href="#recursos" className="hidden hover:text-foreground sm:inline">Recursos</a>
            <a href="#exemplos" className="hidden hover:text-foreground sm:inline">Exemplos</a>
            <a href="#instalar" className="hover:text-foreground">Instalar</a>
            <a href={REPO} className="flex items-center gap-1.5 hover:text-foreground"><Github size={16} />Código</a>
          </div>
        </nav>
      </header>

      <section className="bg-glow relative overflow-hidden px-5 pt-32 pb-20">
        <div className="mx-auto max-w-3xl text-center">
          <span className="inline-flex items-center gap-2 rounded-full border border-border bg-card px-3 py-1 text-xs text-muted-foreground">
            <Sparkles size={14} className="text-primary" /> Raster e vetorial, num só app
          </span>
          <h1 className="mt-6 font-display text-4xl font-bold leading-tight tracking-tight sm:text-6xl">
            Desenhe e pinte no <span className="text-primary">celular</span> e no computador
          </h1>
          <p className="mx-auto mt-5 max-w-xl text-lg text-muted-foreground">
            Pincéis com pressão, camadas, vetor, liquefazer e simetria numa tela limpa, feita para a caneta e para os dedos.
          </p>
          <div className="mt-8 flex flex-col justify-center gap-3 sm:flex-row">
            <a href={APK} className="inline-flex items-center justify-center gap-2 rounded-xl bg-primary px-6 py-3.5 font-semibold text-primary-foreground transition hover:brightness-110">
              <Download size={20} /> Baixar APK
            </a>
            <a href="#computador" className="inline-flex items-center justify-center gap-2 rounded-xl border border-border bg-card px-6 py-3.5 font-semibold transition hover:bg-secondary">
              <Monitor size={20} /> Windows, Linux e macOS
            </a>
          </div>
          <p className="mt-5 text-sm text-muted-foreground">Android 8.0 ou mais novo · Windows · Linux · macOS · gratuito · código aberto</p>
          {rel && (
            <p className="mt-2 text-xs text-muted-foreground">
              <span className="text-accent-foreground">{rel.name}</span>
              {" · "}{new Date(rel.date).toLocaleDateString("pt-BR")}
              {rel.size ? ` · ${(rel.size / 1048576).toFixed(1).replace(".", ",")} MB` : ""}
            </p>
          )}
        </div>

        <div className="relative mx-auto mt-16 max-w-5xl pb-10 sm:pb-16">
          <img src={`${base}screenshot-ui.png`} alt="ArtApp em modo paisagem: pôr do sol com degradê, nuvens em aquarela e montanhas vetoriais, com o painel de pincéis e o de camadas abertos" className="shadow-float w-full rounded-2xl" />
          <div className="shadow-float absolute -bottom-2 right-2 w-[28%] max-w-[230px] rounded-[1.8rem] border-4 border-secondary bg-secondary p-1 sm:right-[-1rem]">
            <img src={`${base}screenshot-celular.png`} alt="ArtApp no celular em pé: céu noturno com lua e estrelas, barra de ferramentas embaixo" className="w-full rounded-[1.4rem]" />
          </div>
        </div>
      </section>

      <section id="recursos" className="scroll-mt-20 px-5 py-20">
        <div className="mx-auto max-w-6xl">
          <h2 className="font-display text-3xl font-bold sm:text-4xl">Recursos</h2>
          <p className="mt-2 text-muted-foreground">Tudo o que você precisa para ilustrar, sem assinatura.</p>
          <div className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {features.map(({ icon: I, t, d }) => (
              <div key={t} className="rounded-2xl border border-border bg-card p-6 transition hover:border-primary/40">
                <span className="grid size-11 place-items-center rounded-xl bg-accent text-primary"><I size={22} /></span>
                <h3 className="mt-4 font-display text-lg font-semibold">{t}</h3>
                <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{d}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section id="exemplos" className="scroll-mt-20 px-5 pb-20">
        <div className="mx-auto max-w-6xl">
          <h2 className="flex items-center gap-3 font-display text-3xl font-bold sm:text-4xl"><Images className="text-primary" />Exemplos</h2>
          <p className="mt-2 text-muted-foreground">Capturas da versão atual, feitas no próprio ArtApp.</p>
          <div className="mt-10 grid gap-6 lg:grid-cols-3">
            {gallery.map(({ img, t, d }) => (
              <figure key={t} className="overflow-hidden rounded-2xl border border-border bg-card">
                <a href={`${base}${img}`}><img src={`${base}${img}`} alt={`${t}: ${d}`} loading="lazy" className="w-full" /></a>
                <figcaption className="p-5">
                  <h3 className="font-display text-lg font-semibold">{t}</h3>
                  <p className="mt-1 text-sm text-muted-foreground">{d}</p>
                </figcaption>
              </figure>
            ))}
          </div>
        </div>
      </section>

      <section className="px-5 pb-20">
        <div className="mx-auto grid max-w-6xl gap-4 md:grid-cols-2">
          <div className="rounded-2xl border border-border bg-card p-8">
            <Maximize2 className="text-primary" />
            <h3 className="mt-4 font-display text-2xl font-bold">A tela é toda sua</h3>
            <p className="mt-3 text-muted-foreground">A área de desenho ocupa tudo e os controles flutuam por cima, sem atrapalhar o traço.</p>
          </div>
          <div className="rounded-2xl border border-border bg-card p-8">
            <Smartphone className="text-primary" />
            <h3 className="mt-4 font-display text-2xl font-bold">Pensado para o celular</h3>
            <p className="mt-3 text-muted-foreground">Com o celular em pé, as ferramentas vão para baixo e os painéis viram folhas que sobem.</p>
          </div>
        </div>
      </section>

      <section id="instalar" className="scroll-mt-20 px-5 py-20">
        <div className="mx-auto max-w-3xl">
          <h2 className="font-display text-3xl font-bold sm:text-4xl">Como instalar</h2>
          <ol className="mt-10 space-y-4">
            {steps.map(({ icon: I, t, d }, i) => (
              <li key={t} className="flex gap-4 rounded-2xl border border-border bg-card p-5">
                <span className="grid size-10 shrink-0 place-items-center rounded-full bg-primary font-display font-bold text-primary-foreground">{i + 1}</span>
                <div>
                  <h3 className="flex items-center gap-2 font-semibold"><I size={18} className="text-primary" />{t}</h3>
                  <p className="mt-1 text-sm text-muted-foreground">{d}</p>
                </div>
              </li>
            ))}
          </ol>
          <p className="mt-6 flex gap-3 rounded-2xl border border-primary/30 bg-accent p-4 text-sm text-accent-foreground">
            <Info size={18} className="mt-0.5 shrink-0" />
            Se o Android disser que conflita com uma versão já instalada, desinstale a anterior e instale de novo.
          </p>

          <h2 id="computador" className="mt-20 flex scroll-mt-20 items-center gap-3 font-display text-3xl font-bold sm:text-4xl"><Monitor className="text-primary" />No computador</h2>
          <p className="mt-2 text-muted-foreground">A mesma release traz versões para Windows, Linux e macOS.</p>
          <ul className="mt-8 grid gap-4 sm:grid-cols-2">
            {desktop.map(({ icon: I, t, file, d }) => (
              <li key={file} className="flex flex-col rounded-2xl border border-border bg-card p-5">
                <h3 className="flex items-center gap-2 font-semibold"><I size={18} className="text-primary" />{t}</h3>
                <p className="mt-1 flex-1 text-sm text-muted-foreground">{d}</p>
                <a href={`${LATEST}/${file}`} className="mt-4 inline-flex items-center justify-center gap-2 rounded-xl bg-primary px-4 py-2.5 text-sm font-semibold text-primary-foreground transition hover:brightness-110">
                  <Download size={16} /> {file}
                </a>
              </li>
            ))}
          </ul>
          <p className="mt-6 flex gap-3 rounded-2xl border border-primary/30 bg-accent p-4 text-sm text-accent-foreground">
            <Info size={18} className="mt-0.5 shrink-0" />
            As versões de computador ainda não têm assinatura paga da Microsoft ou da Apple, por isso o aviso na primeira abertura.
          </p>

          <h2 className="mt-20 flex items-center gap-3 font-display text-2xl font-bold"><Terminal className="text-primary" />Compilar do código</h2>
          <pre className="mt-5 overflow-x-auto rounded-2xl border border-border bg-card p-5 font-mono text-sm leading-7">
            <code>
              <span className="text-primary">$</span> git clone {REPO}{"\n"}
              <span className="text-primary">$</span> cd ArtApp{"\n"}
              <span className="text-primary">$</span> cargo run -p artapp --release
            </code>
          </pre>
        </div>
      </section>

      <footer className="border-t border-border px-5 py-8 text-center text-sm text-muted-foreground">
        ArtApp · MIT ou Apache-2.0 · motores do{" "}
        <a href="https://github.com/storytold/photocraft" className="text-foreground hover:text-primary">PhotoCraft</a> e do{" "}
        <a href="https://github.com/storytold/vectorcraft" className="text-foreground hover:text-primary">VectorCraft</a>
        {" · "}<a href={REPO} className="text-foreground hover:text-primary">código</a>
      </footer>
    </div>
  );
}
