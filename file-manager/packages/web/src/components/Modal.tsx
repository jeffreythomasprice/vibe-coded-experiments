import { createContext, useContext, useState, useEffect, useRef, useCallback, createElement } from "react";
import type { ReactNode } from "react";
import styles from "./Modal.module.css";

interface ConfirmOptions {
    confirmText?: string;
    cancelText?: string;
}

interface PromptOptions {
    defaultValue?: string;
    confirmText?: string;
    cancelText?: string;
}

type ConfirmModal = {
    type: "confirm";
    message: string;
    confirmText: string;
    cancelText: string;
    resolve: (r: boolean) => void;
};

type PromptModal = {
    type: "prompt";
    message: string;
    defaultValue: string;
    confirmText: string;
    cancelText: string;
    resolve: (r: string | null) => void;
};

type ModalState = ConfirmModal | PromptModal | null;

interface ModalContextValue {
    confirm: (message: string, opts?: ConfirmOptions) => Promise<boolean>;
    prompt: (message: string, opts?: PromptOptions) => Promise<string | null>;
}

const ModalContext = createContext<ModalContextValue | null>(null);

export function ModalProvider({ children }: { children: ReactNode }) {
    const [modal, setModal] = useState<ModalState>(null);

    const confirm = useCallback((message: string, opts?: ConfirmOptions): Promise<boolean> => {
        return new Promise((resolve) => {
            setModal({
                type: "confirm",
                message,
                confirmText: opts?.confirmText ?? "OK",
                cancelText: opts?.cancelText ?? "Cancel",
                resolve,
            });
        });
    }, []);

    const prompt = useCallback((message: string, opts?: PromptOptions): Promise<string | null> => {
        return new Promise((resolve) => {
            setModal({
                type: "prompt",
                message,
                defaultValue: opts?.defaultValue ?? "",
                confirmText: opts?.confirmText ?? "OK",
                cancelText: opts?.cancelText ?? "Cancel",
                resolve,
            });
        });
    }, []);

    return createElement(
        ModalContext.Provider,
        { value: { confirm, prompt } },
        children,
        modal !== null ? (
            <ModalWindow
                key="modal"
                modal={modal}
                onClose={() => setModal(null)}
            />
        ) : null,
    );
}

export function useModal(): ModalContextValue {
    const ctx = useContext(ModalContext);
    if (!ctx) throw new Error("useModal must be used within ModalProvider");
    return ctx;
}

function ModalWindow({
    modal,
    onClose,
}: {
    modal: ConfirmModal | PromptModal;
    onClose: () => void;
}) {
    const [inputValue, setInputValue] = useState(modal.type === "prompt" ? modal.defaultValue : "");
    const inputRef = useRef<HTMLInputElement>(null);

    const handleConfirm = useCallback(() => {
        if (modal.type === "confirm") {
            modal.resolve(true);
        } else {
            modal.resolve(inputValue);
        }
        onClose();
    }, [modal, inputValue, onClose]);

    const handleCancel = useCallback(() => {
        if (modal.type === "confirm") {
            modal.resolve(false);
        } else {
            modal.resolve(null);
        }
        onClose();
    }, [modal, onClose]);

    useEffect(() => {
        if (modal.type === "prompt") {
            inputRef.current?.focus();
        }
    }, [modal.type]);

    useEffect(() => {
        function handleKeyDown(e: KeyboardEvent) {
            if (e.key === "Escape") {
                handleCancel();
            }
        }
        document.addEventListener("keydown", handleKeyDown);
        return () => document.removeEventListener("keydown", handleKeyDown);
    }, [handleCancel]);

    function handleOverlayClick(e: React.MouseEvent<HTMLDivElement>) {
        if (e.target === e.currentTarget) {
            handleCancel();
        }
    }

    function handleInputKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
        if (e.key === "Enter") {
            handleConfirm();
        }
    }

    return (
        <div className={styles.overlay} onClick={handleOverlayClick}>
            <div className={styles.window}>
                <div className={styles.titleBar}>
                    {modal.type === "confirm" ? "Confirm" : "Input"}
                </div>
                <div className={styles.body}>
                    <p className={styles.message}>{modal.message}</p>
                    {modal.type === "prompt" && (
                        <input
                            ref={inputRef}
                            className={styles.input}
                            type="text"
                            value={inputValue}
                            onChange={(e) => setInputValue(e.target.value)}
                            onKeyDown={handleInputKeyDown}
                        />
                    )}
                </div>
                <div className={styles.footer}>
                    <button className={styles.confirmBtn} onClick={handleConfirm}>
                        {modal.confirmText}
                    </button>
                    <button className={styles.cancelBtn} onClick={handleCancel}>
                        {modal.cancelText}
                    </button>
                </div>
            </div>
        </div>
    );
}
