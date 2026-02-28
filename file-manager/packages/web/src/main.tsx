import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import { ToastProvider } from "./hooks/useToast.js";
import { Toaster } from "./components/Toaster.js";

const root = document.getElementById("root");
if (!root) throw new Error("No #root element");

createRoot(root).render(
    <StrictMode>
        <ToastProvider>
            <App />
            <Toaster />
        </ToastProvider>
    </StrictMode>,
);
