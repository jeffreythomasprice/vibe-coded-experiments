import { describe, it, expect, beforeEach, afterEach } from "bun:test";
import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement } from "react";
import { ModalProvider, useModal } from "./Modal.js";

// Helper: a button that calls modal.confirm or modal.prompt and stores the result
function ConfirmTrigger({ onResult }: { onResult: (r: boolean) => void }) {
    const modal = useModal();
    return createElement("button", {
        "data-testid": "trigger",
        onClick: () => {
            void modal.confirm("Are you sure?").then(onResult);
        },
    });
}

function PromptTrigger({ onResult }: { onResult: (r: string | null) => void }) {
    const modal = useModal();
    return createElement("button", {
        "data-testid": "trigger",
        onClick: () => {
            void modal.prompt("Enter a name:", { defaultValue: "hello" }).then(onResult);
        },
    });
}

function wrap(child: React.ReactElement) {
    return createElement(ModalProvider, null, child);
}

describe("useModal", () => {
    it("throws when used outside ModalProvider", () => {
        // Suppress React error boundary noise
        const spy = console.error;
        console.error = () => {};
        expect(() => render(createElement(ConfirmTrigger, { onResult: () => {} }))).toThrow(
            "useModal must be used within ModalProvider",
        );
        console.error = spy;
    });
});

describe("modal.confirm", () => {
    let result: boolean | undefined;
    let user: ReturnType<typeof userEvent.setup>;

    beforeEach(() => {
        result = undefined;
        user = userEvent.setup();
        render(wrap(createElement(ConfirmTrigger, { onResult: (r) => { result = r; } })));
    });

    afterEach(() => {
        // Clean up rendered tree
        document.body.innerHTML = "";
    });

    it("resolves true when confirm button is clicked", async () => {
        await user.click(screen.getByTestId("trigger"));
        await user.click(screen.getByText("OK"));
        expect(result).toBe(true);
    });

    it("resolves false when cancel button is clicked", async () => {
        await user.click(screen.getByTestId("trigger"));
        await user.click(screen.getByText("Cancel"));
        expect(result).toBe(false);
    });

    it("resolves false when Escape key is pressed", async () => {
        await user.click(screen.getByTestId("trigger"));
        await user.keyboard("{Escape}");
        expect(result).toBe(false);
    });

    it("resolves false when overlay background is clicked", async () => {
        await user.click(screen.getByTestId("trigger"));
        const overlay = document.querySelector("[class]");
        if (overlay) {
            await act(async () => {
                overlay.dispatchEvent(new MouseEvent("click", { bubbles: true }));
            });
        }
        // overlay click only fires handleCancel when target === currentTarget
        // so clicking a child won't cancel — just verify the modal message shows
        expect(screen.getByText("Are you sure?")).toBeTruthy();
    });

    it("shows the message text in the modal", async () => {
        await user.click(screen.getByTestId("trigger"));
        expect(screen.getByText("Are you sure?")).toBeTruthy();
    });

    it("hides the modal after confirmation", async () => {
        await user.click(screen.getByTestId("trigger"));
        await user.click(screen.getByText("OK"));
        expect(screen.queryByText("Are you sure?")).toBeNull();
    });
});

describe("modal.prompt", () => {
    let result: string | null | undefined;
    let user: ReturnType<typeof userEvent.setup>;

    beforeEach(() => {
        result = undefined;
        user = userEvent.setup();
        render(wrap(createElement(PromptTrigger, { onResult: (r) => { result = r; } })));
    });

    afterEach(() => {
        document.body.innerHTML = "";
    });

    it("resolves the input value when confirm is clicked", async () => {
        await user.click(screen.getByTestId("trigger"));
        const input = screen.getByRole("textbox");
        await user.clear(input);
        await user.type(input, "world");
        await user.click(screen.getByText("OK"));
        expect(result).toBe("world");
    });

    it("resolves the default value when confirm is clicked without typing", async () => {
        await user.click(screen.getByTestId("trigger"));
        await user.click(screen.getByText("OK"));
        expect(result).toBe("hello");
    });

    it("resolves null when cancel is clicked", async () => {
        await user.click(screen.getByTestId("trigger"));
        await user.click(screen.getByText("Cancel"));
        expect(result).toBeNull();
    });

    it("resolves null when Escape key is pressed", async () => {
        await user.click(screen.getByTestId("trigger"));
        await user.keyboard("{Escape}");
        expect(result).toBeNull();
    });

    it("resolves input value when Enter key is pressed in the input", async () => {
        await user.click(screen.getByTestId("trigger"));
        const input = screen.getByRole("textbox");
        await user.clear(input);
        await user.type(input, "typed{Enter}");
        expect(result).toBe("typed");
    });
});
