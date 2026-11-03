import { useId, useMemo, useState } from "react";
import { ExternalLink, Info, Smartphone, FolderOpen, Sparkles, ShieldCheck } from "lucide-react";
import { Header, Footer, DownloadButton } from "./Layout";
import { packs, hostHelp, type BrushPack } from "./brushes-data";

const filters = ["Todos", "Clip Studio (.sut)", "Photoshop (.abr)"] as const;

const WAVE = "M14 70 C 60 20, 100 20, 140 52 S 220 96, 262 46 S 330 20, 346 40";

/** Traço de exemplo desenhado aqui (quando o autor não tem imagem de prévia, ou ela não carrega). */
function SampleStroke({ kind }: { kind: string }) {
  const id = useId().replace(/:/g, "");
  const k = kind.toLowerCase();
  let stroke;
  if (k.includes("aquarela")) {
    stroke = (
      <g filter={`url(#${id}w)`} opacity={0.75}>
        <path d={WAVE} stroke="var(--primary)" strokeWidth={26} fill="none" strokeLinecap="round" />
        <circle cx={300} cy={80} r={6} fill="var(--primary)" /><circle cx={318} cy={70} r={3} fill="var(--primary)" />
      </g>
    );
  } else if (k.includes("esboço") || k.includes("arte-final")) {
    stroke = (
      <g filter={`url(#${id}g)`} fill="none" strokeLinecap="round" stroke="var(--foreground)">
        <path d={WAVE} strokeWidth={3} />
        <path d={WAVE} strokeWidth={1.5} transform="translate(2 4)" opacity={0.6} />
        <path d={WAVE} strokeWidth={1} transform="translate(-3 9)" opacity={0.35} />
      </g>
    );
  } else if (k.includes("quadrinhos")) {
    stroke = (
      <>
        <pattern id={`${id}d`} width={8} height={8} patternUnits="userSpaceOnUse"><circle cx={4} cy={4} r={2.4} fill="var(--foreground)" /></pattern>
        <path d={WAVE} stroke={`url(#${id}d)`} strokeWidth={30} fill="none" strokeLinecap="round" filter={`url(#${id}g)`} />
      </>
    );
  } else if (k.includes("textura")) {
    stroke = <path d={WAVE} stroke="#a0652d" strokeWidth={30} strokeDasharray="2 3 6 2" fill="none" filter={`url(#${id}g)`} />;
  } else {
    stroke = <path d={WAVE} stroke="var(--primary)" strokeWidth={22} fill="none" strokeLinecap="round" filter={`url(#${id}g)`} />;
  }
  return (
    <svg viewBox="0 0 360 110" className="h-full w-full" aria-hidden="true">
      <defs>
        <filter id={`${id}g`}><feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves={2} /><feDisplacementMap in="SourceGraphic" scale={6} /></filter>
        <filter id={`${id}w`}><feTurbulence type="fractalNoise" baseFrequency="0.04" numOctaves={3} /><feDisplacementMap in="SourceGraphic" scale={18} /><feGaussianBlur stdDeviation={1.5} /></filter>
      </defs>
      {stroke}
    </svg>
  );
}

function Preview({ p }: { p: BrushPack }) {
  const [failed, setFailed] = useState(false);
  const official = p.preview && !failed;
  return (
    <figure className="-mx-5 -mt-5 mb-4 overflow-hidden rounded-t-2xl border-b border-border">
      <a href={p.page} target="_blank" rel="noopener noreferrer" className="block aspect-[2/1] bg-secondary">
        {official ? (
          <img
            src={p.preview}
            alt={`Prévia de ${p.name}, da página do autor`}
            loading="lazy"
            referrerPolicy="no-referrer"
            onError={() => setFailed(true)}
            className="h-full w-full object-cover"
          />
        ) : (
          <div className="grid h-full place-items-center p-4"><SampleStroke kind={p.kind} /></div>
        )}
      </a>
      <figcaption className="px-5 pt-2 text-[11px] text-muted-foreground">
        {official ? <>Prévia oficial · {p.author}</> : <>Ilustração aproximada · veja as prévias reais na página</>}
      </figcaption>
    </figure>
  );
}

function Card({ p }: { p: BrushPack }) {
  return (
    <li className="flex flex-col overflow-hidden rounded-2xl border border-border bg-card p-5 transition hover:border-primary/40">
      <Preview p={p} />
      <div className="flex flex-wrap gap-2 text-xs">
        <span className="rounded-full bg-accent px-2.5 py-1 text-accent-foreground">{p.format}</span>
        <span className="rounded-full border border-border px-2.5 py-1 text-muted-foreground">{p.kind}</span>
      </div>
      <h3 className="mt-3 font-display text-lg font-semibold leading-snug">{p.name}</h3>
      <p className="text-sm text-muted-foreground">por <span className="text-foreground">{p.author}</span> · {p.count}</p>
      <p className="mt-3 flex-1 text-sm leading-relaxed text-muted-foreground">{p.desc}</p>
      <p className="mt-3 text-xs text-muted-foreground"><b className="text-foreground">Licença:</b> {p.license}</p>
      <div className="mt-4 flex flex-col gap-2 sm:flex-row">
        <DownloadButton href={p.download} size="sm" className="flex-1" label={`Download de ${p.name} em ${hostHelp[p.host].name}`}>Download</DownloadButton>
        <a href={p.page} target="_blank" rel="noopener noreferrer" className="inline-flex flex-1 items-center justify-center gap-2 rounded-xl border border-border bg-card px-4 py-2.5 text-sm font-semibold transition hover:bg-secondary">
          <ExternalLink size={16} /> Ver prévias
        </a>
      </div>
      <p className="mt-2 text-xs text-muted-foreground">em {hostHelp[p.host].name}</p>
    </li>
  );
}

const steps = [
  { icon: Smartphone, t: "Abra pelo ArtApp", d: <>No app, toque em <b>Mais pincéis…</b> (Pincéis) ou <b>☰ › Camada › Mais materiais</b>. Esta página abre no navegador.</> },
  { icon: Sparkles, t: "Baixe o pacote", d: <>Toque em <b>Download</b> e siga a página do autor. O arquivo vai para <b>Downloads</b>.</> },
  { icon: FolderOpen, t: "Volte ao ArtApp", d: <>Ele instala sozinho os <b>.sut</b>, <b>.abr</b> e <b>.zip</b> baixados, numa pasta em <b>Meus pincéis</b>. Também dá para instalar depois em <b>Pincéis › Instalar pincéis…</b></> },
];

export default function Brushes() {
  const [f, setF] = useState<(typeof filters)[number]>("Todos");
  const list = useMemo(() => (f === "Todos" ? packs : packs.filter((p) => p.format === f)), [f]);
  return (
    <div className="min-h-screen font-sans">
      <Header home={false} />
      <section className="bg-glow px-5 pt-32 pb-12">
        <div className="mx-auto max-w-3xl text-center">
          <h1 className="font-display text-4xl font-bold leading-tight tracking-tight sm:text-5xl">
            Pincéis e brushes <span className="text-primary">compatíveis</span>
          </h1>
          <p className="mx-auto mt-5 max-w-xl text-lg text-muted-foreground">
            Pacotes gratuitos e populares do Clip Studio Paint e do Photoshop que funcionam no ArtApp. O download acontece na página oficial de cada autor.
          </p>
        </div>
      </section>

      <section className="px-5 pb-16">
        <div className="mx-auto max-w-6xl">
          <ol className="grid gap-4 md:grid-cols-3">
            {steps.map(({ icon: I, t, d }, i) => (
              <li key={t} className="flex gap-4 rounded-2xl border border-border bg-card p-5">
                <span className="grid size-10 shrink-0 place-items-center rounded-full bg-primary font-display font-bold text-primary-foreground">{i + 1}</span>
                <div>
                  <h2 className="flex items-center gap-2 font-semibold"><I size={18} className="text-primary" />{t}</h2>
                  <p className="mt-1 text-sm text-muted-foreground">{d}</p>
                </div>
              </li>
            ))}
          </ol>

          <div className="mt-12 flex flex-wrap gap-2" role="tablist" aria-label="Formato">
            {filters.map((x) => (
              <button
                key={x}
                role="tab"
                aria-selected={f === x}
                onClick={() => setF(x)}
                className={`rounded-full border px-4 py-2 text-sm transition ${f === x ? "border-primary bg-primary text-primary-foreground" : "border-border bg-card text-muted-foreground hover:text-foreground"}`}
              >
                {x}
              </button>
            ))}
          </div>

          <ul className="mt-6 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {list.map((p) => <Card key={p.page} p={p} />)}
          </ul>

          <div className="mt-10 grid gap-4 md:grid-cols-3">
            {Object.values(hostHelp).map((h) => (
              <p key={h.name} className="rounded-2xl border border-border bg-card p-4 text-sm text-muted-foreground">
                <b className="text-foreground">Baixar no {h.name}:</b> {h.how}
              </p>
            ))}
          </div>

          <p className="mt-6 flex gap-3 rounded-2xl border border-primary/30 bg-accent p-4 text-sm text-accent-foreground">
            <ShieldCheck size={18} className="mt-0.5 shrink-0" />
            Os pincéis pertencem aos autores e seguem a licença de cada um. O ArtApp não hospeda nem altera os arquivos: esta página só aponta para as páginas oficiais.
          </p>
          <p className="mt-4 flex gap-3 rounded-2xl border border-border bg-card p-4 text-sm text-muted-foreground">
            <Info size={18} className="mt-0.5 shrink-0 text-primary" />
            Pincéis do Procreate (.brushset) e do Krita (.bundle) ainda não são compatíveis. Lista conferida em novembro de 2026; preços e termos podem mudar na página do autor.
          </p>
        </div>
      </section>
      <Footer />
    </div>
  );
}
