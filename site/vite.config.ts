import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// O GitHub Pages serve o site em https://rafanunesran.github.io/ArtApp/.
export default defineConfig({
  base: "/ArtApp/",
  plugins: [react(), tailwindcss()],
});
