import { useState, useEffect } from "react";

export function usePersistentState<T>(key: string, defaultValue: T): [T, React.Dispatch<React.SetStateAction<T>>] {
    const [state, setState] = useState<T>(() => {
        try {
            const raw = localStorage.getItem(key);
            return raw !== null ? (JSON.parse(raw) as T) : defaultValue;
        } catch {
            return defaultValue;
        }
    });

    useEffect(() => {
        try {
            localStorage.setItem(key, JSON.stringify(state));
        } catch {
            // storage quota exceeded or private mode — silently ignore
        }
    }, [key, state]);

    return [state, setState];
}
