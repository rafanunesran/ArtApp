import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Bug, ChevronDown, ExternalLink, RefreshCw, ShieldCheck } from "lucide-react";
import { Header, Footer, REPO } from "./Layout";

/** Gerado pelo workflow `crash-reports.yml` no ramo `relatorios-de-erros`. */
const DATA = "https://raw.githubusercontent.com/rafanunesran/ArtApp/relatorios-de-erros/relatorios.json";

type Example = { hora: number; build: number | null; sistema: string; texto: string };
type Group = {
  chave: string;
  tipo: "panico" | "fechou";
  erro: string;
  onde: string;
  total: number;
  primeira: number;
  ultima: number;
  builds: Record<string, number>;
  sistemas: Record<string, number>;
  exemplos: Example[];
  issue: number | null;
};
type Data = { atualizado: number | null; grupos: Record<string, Group> };

const periods = [
  { label: "24 horas", secs: 86400 },
  { label: "7 dias", secs: 7 * 86400 },
  { label: "30 dias", secs: 30 * 86400 },
  { label: "Tudo", secs: Infinity },
] as const;

function when(t: number) {
  return new Date(t * 1000).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" });
}

function ago(t: number) {
  const s = Date.now() / 1000 - t;
  if (s < 3600) return `há ${Math.max(1, Math.round(s / 60))} min`;
  if (s < 86400) return `há ${Math.round(s / 3600)} h`;
  return `há ${Math.round(s / 86400)} dias`;
}

function list(rec: Record<string, number>) {
  return Object.entries(rec)
    .sort((a, b) => (Number(b[0]) || 0) - (Number(a[0]) || 0))
    .map(([k, n]) => `${k} (${n}×)`)
    .join(", ");
}

function Card({ g }: { g: Group }) {
  const [open, setOpen] = useState(false);
  const newest = Math.max(0, ...Object.keys(g.builds).map(Number).filter(Number.isFinite));
  return (
    <li className="rounded-2xl border border-border bg-card p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-2 text-xs uppercase tracking-wide text-muted-foreground">
            {g.tipo === "panico" ? <Bug size={14} className="text-primary" /> : <AlertTriangle size={14} className="text-primary" />}
            {g.tipo === "panico" ? "Erro do Rust (pânico)" : "O app fechou de repente"}
          </p>
          <h2 className="mt-1 break-words font-semibold">{g.erro || "(sem mensagem)"}</h2>
          {g.onde && <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{g.onde}</p>}
        </div>
        <div className="text-right">
          <p className="font-display text-3xl font-bold">{g.total}×</p>
          <p className="text-xs text-muted-foreground">última {ago(g.ultima)}</p>
        </div>
      </div>
      <dl className="mt-4 grid gap-1 text-sm text-muted-foreground sm:grid-cols-2">
        <div><dt className="inline text-foreground">Builds: </dt><dd className="inline">{list(g.builds)}</dd></div>
        <div><dt className="inline text-foreground">Sistemas: </dt><dd className="inline">{list(g.sistemas)}</dd></div>
        <div><dt className="inline text-foreground">Primeira vez: </dt><dd className="inline">{when(g.primeira)}</dd></div>
        <div><dt className="inline text-foreground">Última: </dt><dd className="inline">{when(g.ultima)} (build {newest || "?"})</dd></div>
      </dl>
      <div className="mt-4 flex flex-wrap gap-2">
        {g.exemplos.length > 0 && <button
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground"
        >
          <ChevronDown size={16} className={open ? "rotate-180 transition" : "transition"} /> Ver relatório
        </button>}
        {g.issue && (
          <a href={`${REPO}/issues/${g.issue}`} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground">
            Issue #{g.issue} <ExternalLink size={14} />
          </a>
        )}
      </div>
      {open && (
        <div className="mt-4 space-y-3">
          {g.exemplos.map((e) => (
            <figure key={e.hora + e.texto.length}>
              <figcaption className="mb-1 text-xs text-muted-foreground">{when(e.hora)} · build {e.build ?? "?"} · {e.sistema}</figcaption>
              <pre className="max-h-80 overflow-auto rounded-xl bg-secondary p-3 text-xs leading-relaxed">{e.texto}</pre>
            </figure>
          ))}
        </div>
      )}
    </li>
  );
}

export default function Errors() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [period, setPeriod] = useState<(typeof periods)[number]["label"]>("7 dias");
  const [kind, setKind] = useState<"todos" | "panico" | "fechou">("todos");
  const [reload, setReload] = useState(0);

  useEffect(() => {
    setError(null);
    fetch(`${DATA}?t=${Date.now()}`, { cache: "no-store" })
      .then((r) => {
        if (r.status === 404) return { atualizado: null, grupos: {} };
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then(setData)
      .catch((e) => setError(String(e)));
  }, [reload]);

  const groups = useMemo(() => {
    if (!data) return [];
    const secs = periods.find((p) => p.label === period)?.secs ?? Infinity;
    const from = Date.now() / 1000 - secs;
    return Object.values(data.grupos)
      .filter((g) => g.ultima >= from && (kind === "todos" || g.tipo === kind))
      .sort((a, b) => b.ultima - a.ultima);
  }, [data, period, kind]);
  const total = groups.reduce((n, g) => n + g.total, 0);

  return (
    <div className="min-h-screen bg-background text-foreground">
      <Header home={false} />
      <section className="px-5 pb-8 pt-32">
        <div className="mx-auto max-w-4xl">
          <h1 className="font-display text-4xl font-bold sm:text-5xl">Relatórios de erro</h1>
          <p className="mt-4 max-w-2xl text-muted-foreground">
            Erros que o ArtApp enviou sozinho (quando a pessoa deixou), agrupados por tipo. A lista é atualizada a cada hora; cada tipo de erro também vira uma issue no GitHub para ser corrigido.
          </p>
          <p className="mt-4 flex gap-3 rounded-2xl border border-border bg-card p-4 text-sm text-muted-foreground">
            <ShieldCheck size={18} className="mt-0.5 shrink-0 text-primary" />
            Os relatórios têm só a parte técnica (versão, sistema, onde o erro aconteceu). Desenhos, nomes de arquivos e caminhos de pastas não são enviados.
          </p>
        </div>
      </section>
      <section className="px-5 pb-16">
        <div className="mx-auto max-w-4xl">
          <div className="flex flex-wrap items-center gap-2">
            {periods.map((p) => (
              <button
                key={p.label}
                onClick={() => setPeriod(p.label)}
                aria-pressed={period === p.label}
                className={`rounded-full border px-4 py-2 text-sm transition ${period === p.label ? "border-primary bg-primary text-primary-foreground" : "border-border bg-card text-muted-foreground hover:text-foreground"}`}
              >
                {p.label}
              </button>
            ))}
            <select
              value={kind}
              onChange={(e) => setKind(e.target.value as typeof kind)}
              aria-label="Tipo de erro"
              className="rounded-full border border-border bg-card px-4 py-2 text-sm text-foreground"
            >
              <option value="todos">Todos os tipos</option>
              <option value="panico">Erros do Rust</option>
              <option value="fechou">Fechou de repente</option>
            </select>
            <button onClick={() => setReload(reload + 1)} className="ml-auto inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground">
              <RefreshCw size={14} /> Atualizar
            </button>
          </div>

          <p className="mt-6 text-sm text-muted-foreground">
            {error
              ? `Não deu para carregar os relatórios (${error}).`
              : !data
                ? "Carregando…"
                : `${groups.length} tipo(s) de erro, ${total} ocorrência(s) no período.` +
                  (data.atualizado ? ` Última novidade ${ago(data.atualizado)}.` : " Nenhum relatório recebido ainda.")}
          </p>

          <ul className="mt-4 space-y-4">
            {groups.map((g) => <Card key={g.chave} g={g} />)}
          </ul>
        </div>
      </section>
      <Footer />
    </div>
  );
}
