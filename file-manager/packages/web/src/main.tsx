import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import { ToastProvider } from "./hooks/useToast.js";
import { Toaster } from "./components/Toaster.js";
import { ModalProvider } from "./components/Modal.js";

const root = document.getElementById("root");
if (!root) throw new Error("No #root element");

createRoot(root).render(
    <StrictMode>
        <ToastProvider>
            <ModalProvider>
                <App />
                <Toaster />
            </ModalProvider>
        </ToastProvider>
    </StrictMode>,
);
