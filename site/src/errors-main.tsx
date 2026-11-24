import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import Errors from "./Errors";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Errors />
  </StrictMode>,
);
