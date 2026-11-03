import type { ReactNode } from "react";
import { Brush, Download, Github } from "lucide-react";

export const REPO = "https://github.com/rafanunesran/ArtApp";
export const base = import.meta.env.BASE_URL;

/** O botão de download do site (o mesmo do "Baixar APK"). */
export function DownloadButton({
  href, children, size = "lg", external = false, label, className = "",
}: {
  href: string; children: ReactNode; size?: "lg" | "sm"; external?: boolean; label?: string; className?: string;
}) {
  const sz = size === "lg" ? "px-6 py-3.5" : "px-4 py-2.5 text-sm";
  return (
    <a
      href={href}
      aria-label={label}
      {...(external ? { target: "_blank", rel: "noopener noreferrer" } : {})}
      className={`inline-flex items-center justify-center gap-2 rounded-xl bg-primary ${sz} font-semibold text-primary-foreground transition hover:brightness-110 ${className}`}
    >
      <Download size={size === "lg" ? 20 : 16} /> {children}
    </a>
  );
}

/** Barra de cima. Na página inicial os links são âncoras; nas outras, voltam para ela. */
export function Header({ home = true }: { home?: boolean }) {
  const to = (id: string) => (home ? `#${id}` : `${base}#${id}`);
  return (
    <header className="fixed inset-x-0 top-0 z-50 border-b border-border bg-background/80 backdrop-blur-lg">
      <nav className="mx-auto flex h-16 max-w-6xl items-center justify-between gap-3 px-5">
        <a href={base} className="flex items-center gap-2 font-display text-lg font-bold">
          <span className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground"><Brush size={18} /></span>
          ArtApp
        </a>
        <div className="flex items-center gap-4 text-sm text-muted-foreground sm:gap-5">
          <a href={to("recursos")} className="hidden hover:text-foreground md:inline">Recursos</a>
          <a href={to("exemplos")} className="hidden hover:text-foreground md:inline">Exemplos</a>
          <a href={`${base}pinceis/`} className={home ? "hover:text-foreground" : "text-foreground"}>Pincéis</a>
          <a href={to("instalar")} className="hover:text-foreground">Instalar</a>
          <a href={REPO} className="flex items-center gap-1.5 hover:text-foreground"><Github size={16} /><span className="hidden sm:inline">Código</span></a>
        </div>
      </nav>
    </header>
  );
}

export function Footer() {
  return (
    <footer className="border-t border-border px-5 py-8 text-center text-sm text-muted-foreground">
      ArtApp · MIT ou Apache-2.0 · motores do{" "}
      <a href="https://github.com/storytold/photocraft" className="text-foreground hover:text-primary">PhotoCraft</a> e do{" "}
      <a href="https://github.com/storytold/vectorcraft" className="text-foreground hover:text-primary">VectorCraft</a>
      {" · "}<a href={REPO} className="text-foreground hover:text-primary">código</a>
    </footer>
  );
}
