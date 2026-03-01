import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import { ToastProvider } from "./hooks/useToast.js";
import { Toaster } from "./components/Toaster.js";
import { ModalProvider } from "./components/Modal.js";
import { OperationsProvider } from "./hooks/useOperations.js";

const root = document.getElementById("root");
if (!root) throw new Error("No #root element");

createRoot(root).render(
    <StrictMode>
        <ToastProvider>
            <OperationsProvider>
                <ModalProvider>
                    <App />
                    <Toaster />
                </ModalProvider>
            </OperationsProvider>
        </ToastProvider>
    </StrictMode>,
);
