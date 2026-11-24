import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import Brushes from "./Brushes";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Brushes />
  </StrictMode>,
);
