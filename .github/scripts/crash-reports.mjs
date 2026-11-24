// Junta os relatórios de erro que o app posta no ntfy.sh (ver crates/artapp/src/crash_upload.rs).
//
// Roda de hora em hora no GitHub Actions (.github/workflows/crash-reports.yml):
// 1. busca as mensagens do canal (o ntfy.sh guarda por 12 h);
// 2. agrupa por tipo de erro e grava em <dados>/relatorios.json (ramo `relatorios-de-erros`),
//    que a página https://rafanunesran.github.io/ArtApp/erros/ mostra;
// 3. abre uma issue por tipo de erro (rótulo `erro-do-app`) ou comenta as ocorrências novas, e
//    reabre a issue se o erro voltar num build mais novo que o da correção.
//
// Uso: node crash-reports.mjs <pasta-dos-dados>
// Ambiente: GITHUB_TOKEN, GITHUB_REPOSITORY (sem eles só atualiza o JSON).

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const TOPIC = "artapp-relatorios-de-erros-7k2q9m";
const HEADER = "ARTAPP-CRASH 1";
const LABEL = "erro-do-app";
const MAX_NEW_PER_RUN = 200; // relatórios
const MAX_NEW_ISSUES = 5; // por execução (proteção contra lixo no canal público)
const MAX_SEEN = 3000;
const EXAMPLES = 3;

/** Lê um relatório. `null` se não for do app. */
export function parse(text) {
  if (typeof text !== "string" || text.length > 8000) return null;
  const lines = text.replace(/\r/g, "").split("\n");
  if (lines[0].trim() !== HEADER) return null;
  const f = {};
  let i = 1;
  for (; i < lines.length && lines[i] !== "---"; i++) {
    const m = /^([a-z]+): ?(.*)$/.exec(lines[i]);
    if (m) f[m[1]] = m[2].trim().slice(0, 400);
  }
  if (f.tipo !== "panico" && f.tipo !== "fechou") return null;
  return {
    build: /^\d+$/.test(f.build ?? "") ? Number(f.build) : null,
    sistema: (f.sistema ?? "?").slice(0, 60),
    tipo: f.tipo,
    erro: f.erro ?? "",
    onde: f.onde ?? "",
    corpo: lines.slice(i + 1).join("\n").slice(0, 5000),
  };
}

/** O que define "o mesmo erro": tipo, lugar no código e a mensagem sem números. */
export function signature(r) {
  const msg = r.erro
    .toLowerCase()
    .replace(/0x[0-9a-f]+/g, "0x")
    .replace(/\d+/g, "N")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 200);
  const sig = `${r.tipo}|${r.onde}|${msg}`;
  return createHash("sha1").update(sig).digest("hex").slice(0, 12);
}

export function title(g) {
  const erro = g.erro.length > 90 ? `${g.erro.slice(0, 90)}…` : g.erro;
  return g.tipo === "panico"
    ? `Erro: ${erro} (${g.onde || "lugar desconhecido"})`
    : `App fechou: ${erro}`;
}

/** Junta os relatórios novos em `data`. Devolve as chaves dos grupos que mudaram. */
export function merge(data, messages) {
  const seen = new Set(data.vistos);
  const changed = new Map();
  let added = 0;
  for (const m of messages) {
    if (m.event !== "message" || !m.id || seen.has(m.id)) continue;
    seen.add(m.id);
    data.vistos.push(m.id);
    const r = parse(m.message);
    if (!r || added >= MAX_NEW_PER_RUN) continue;
    added++;
    const key = signature(r);
    const t = Number(m.time) || Math.floor(Date.now() / 1000);
    const g = (data.grupos[key] ??= {
      chave: key,
      tipo: r.tipo,
      erro: r.erro,
      onde: r.onde,
      total: 0,
      primeira: t,
      ultima: t,
      builds: {},
      sistemas: {},
      exemplos: [],
      issue: null,
      fechada_no_build: null,
    });
    g.total++;
    g.primeira = Math.min(g.primeira, t);
    g.ultima = Math.max(g.ultima, t);
    const b = r.build ?? "?";
    g.builds[b] = (g.builds[b] ?? 0) + 1;
    g.sistemas[r.sistema] = (g.sistemas[r.sistema] ?? 0) + 1;
    g.exemplos.unshift({ hora: t, build: r.build, sistema: r.sistema, texto: r.corpo });
    g.exemplos = g.exemplos.slice(0, EXAMPLES);
    const c = changed.get(key) ?? { novas: 0, builds: new Set() };
    c.novas++;
    if (r.build != null) c.builds.add(r.build);
    changed.set(key, c);
  }
  data.vistos = data.vistos.slice(-MAX_SEEN);
  return changed;
}

/** Texto não confiável (o canal é público) dentro de um bloco de código. */
function code(text) {
  return "```text\n" + text.replace(/`/g, "ˋ").slice(0, 3500) + "\n```";
}

function fmtTime(t) {
  return new Date(t * 1000).toISOString().replace("T", " ").slice(0, 16) + " UTC";
}

async function gh(method, path, body) {
  const res = await fetch(`https://api.github.com/repos/${process.env.GITHUB_REPOSITORY}${path}`, {
    method,
    headers: {
      authorization: `Bearer ${process.env.GITHUB_TOKEN}`,
      accept: "application/vnd.github+json",
      "content-type": "application/json",
      "x-github-api-version": "2022-11-28",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok && !(method === "POST" && path === "/labels" && res.status === 422)) {
    throw new Error(`${method} ${path}: ${res.status} ${await res.text()}`);
  }
  return res.status === 204 ? null : res.json();
}

async function syncIssues(data, changed) {
  if (!process.env.GITHUB_TOKEN || !process.env.GITHUB_REPOSITORY || changed.size === 0) return;
  await gh("POST", "/labels", { name: LABEL, color: "d73a4a", description: "Relatório de erro enviado pelo app" });
  const pages = `https://${process.env.GITHUB_REPOSITORY.split("/")[0]}.github.io/${process.env.GITHUB_REPOSITORY.split("/")[1]}/erros/`;
  let created = 0;
  for (const [key, c] of changed) {
    const g = data.grupos[key];
    const ex = g.exemplos[0];
    const detail = `${c.novas} ocorrência(s) nova(s) · builds ${[...c.builds].sort((a, b) => a - b).join(", ") || "?"} · última ${fmtTime(g.ultima)}\n\n${code(ex?.texto ?? "")}`;
    try {
      if (!g.issue) {
        if (created >= MAX_NEW_ISSUES) continue;
        const issue = await gh("POST", "/issues", {
          title: title(g),
          labels: [LABEL],
          body:
            `Relatório enviado automaticamente pelo app (agrupado como \`${key}\`). Lista completa: ${pages}\n\n` +
            `- Tipo: ${g.tipo === "panico" ? "pânico do Rust (o app contornou ou fechou)" : "o app fechou de repente"}\n` +
            `- Onde: \`${(g.onde || "?").replace(/`/g, "")}\`\n- Sistema: ${ex?.sistema ?? "?"}\n\n${detail}\n\n` +
            `Feche esta issue quando corrigir: se o erro voltar num build mais novo, ela é reaberta sozinha.`,
        });
        g.issue = issue.number;
        created++;
        continue;
      }
      const issue = await gh("GET", `/issues/${g.issue}`);
      const newest = Math.max(0, ...c.builds);
      let reopen = false;
      if (issue.state === "closed") {
        // Fechada = corrigida no build seguinte ao maior visto até então.
        g.fechada_no_build ??= newestBefore(g, c);
        reopen = newest > g.fechada_no_build;
      } else {
        g.fechada_no_build = null;
      }
      if (issue.state === "closed" && !reopen) continue;
      if (reopen) {
        await gh("PATCH", `/issues/${g.issue}`, { state: "open" });
        g.fechada_no_build = null;
      }
      await gh("POST", `/issues/${g.issue}/comments`, {
        body: (reopen ? "**O erro voltou depois da correção.**\n\n" : "") + detail,
      });
    } catch (e) {
      console.error(`issue de ${key}: ${e.message}`);
    }
  }
}

/** O maior build visto antes destas ocorrências novas. */
function newestBefore(g, c) {
  const before = { ...g.builds };
  for (const b of c.builds) before[b] = (before[b] ?? 0) - 1;
  return Math.max(0, ...Object.entries(before).filter(([b, n]) => n > 0 && /^\d+$/.test(b)).map(([b]) => Number(b)));
}

async function main() {
  const dir = process.argv[2] ?? ".";
  const file = join(dir, "relatorios.json");
  const data = existsSync(file)
    ? JSON.parse(readFileSync(file, "utf8"))
    : { versao: 1, atualizado: null, vistos: [], grupos: {} };
  const res = await fetch(`https://ntfy.sh/${TOPIC}/json?poll=1&since=all`);
  if (!res.ok) throw new Error(`ntfy.sh: ${res.status}`);
  const messages = (await res.text())
    .split("\n")
    .filter(Boolean)
    .map((l) => {
      try {
        return JSON.parse(l);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
  const before = JSON.stringify(data);
  const changed = merge(data, messages);
  await syncIssues(data, changed);
  // Só grava (e cria commit) quando algo mudou.
  if (JSON.stringify(data) !== before || !existsSync(file)) {
    data.atualizado = Math.floor(Date.now() / 1000);
    writeFileSync(file, JSON.stringify(data, null, 1) + "\n");
  }
  console.log(`${messages.length} mensagem(ns) no canal, ${changed.size} tipo(s) de erro com novidade.`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
