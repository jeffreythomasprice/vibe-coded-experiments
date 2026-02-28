import { useEffect } from "react";
import styles from "./ContextMenu.module.css";

export interface MenuItem {
    label: string;
    onClick: () => void;
    disabled?: boolean;
}

interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: Props) {
    useEffect(() => {
        function handleClick() {
            onClose();
        }
        function handleKeyDown(e: KeyboardEvent) {
            if (e.key === "Escape") onClose();
        }
        document.addEventListener("click", handleClick);
        document.addEventListener("keydown", handleKeyDown);
        return () => {
            document.removeEventListener("click", handleClick);
            document.removeEventListener("keydown", handleKeyDown);
        };
    }, [onClose]);

    // Adjust position to stay in viewport
    const menuWidth = 160;
    const menuHeight = items.length * 36;
    const adjustedX = x + menuWidth > window.innerWidth ? window.innerWidth - menuWidth - 8 : x;
    const adjustedY = y + menuHeight > window.innerHeight ? window.innerHeight - menuHeight - 8 : y;

    return (
        <div
            className={styles.menu}
            style={{ left: adjustedX, top: adjustedY }}
            onContextMenu={(e) => e.preventDefault()}
        >
            <ul className={styles.list}>
                {items.map((item) => (
                    <li
                        key={item.label}
                        className={`${styles.item} ${item.disabled ? styles.disabled : ""}`}
                        onClick={(e) => {
                            if (item.disabled) {
                                e.preventDefault();
                                return;
                            }
                            e.stopPropagation();
                            item.onClick();
                            onClose();
                        }}
                    >
                        {item.label}
                    </li>
                ))}
            </ul>
        </div>
    );
}
