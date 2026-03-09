import { useState, useEffect, useRef } from "react";
import type { Conversation, ConversationMessage } from "@rag/shared";
import { api } from "../api";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Spinner } from "./Spinner";
import styles from "./AgentPage.module.css";

export function AgentPage() {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [conversationId, setConversationId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [renameModalOpen, setRenameModalOpen] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    api.listConversations().then(setConversations).catch(() => {});
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  async function loadConversation(id: number) {
    try {
      const conv = await api.getConversation(id);
      setConversationId(id);
      setMessages(conv.messages);
      setError("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  function handleNewChat() {
    setConversationId(null);
    setMessages([]);
    setError("");
    setInput("");
    textareaRef.current?.focus();
  }

  async function handleDeleteConversation(e: React.MouseEvent, id: number) {
    e.stopPropagation();
    try {
      await api.deleteConversation(id);
      setConversations((prev) => prev.filter((c) => c.id !== id));
      if (conversationId === id) {
        handleNewChat();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  const currentConversation = conversations.find((c) => c.id === conversationId);

  function openRenameModal() {
    setRenameValue(currentConversation?.title || "");
    setRenameModalOpen(true);
  }

  async function handleRename() {
    if (!conversationId || !renameValue.trim()) return;
    try {
      const updated = await api.renameConversation(conversationId, renameValue.trim());
      setConversations((prev) =>
        prev.map((c) => (c.id === updated.id ? { ...c, title: updated.title } : c))
      );
      setRenameModalOpen(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleSend() {
    const text = input.trim();
    if (!text || loading) return;

    setInput("");
    setLoading(true);
    setError("");

    // Optimistically show user message
    const tempUserMsg: ConversationMessage = {
      id: -Date.now(),
      role: "user",
      content: text,
      created_at: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, tempUserMsg]);

    try {
      const result = await api.agent(text, conversationId);
      setConversationId(result.conversation_id);

      // Replace temp user message with real messages from server
      setMessages((prev) => {
        const withoutTemp = prev.filter((m) => m.id !== tempUserMsg.id);
        return [...withoutTemp, ...result.messages];
      });

      // Refresh conversation list
      const convs = await api.listConversations();
      setConversations(convs);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      // Remove optimistic message on error
      setMessages((prev) => prev.filter((m) => m.id !== tempUserMsg.id));
    } finally {
      setLoading(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function renderToolMessage(msg: ConversationMessage) {
    const info = msg.tool_info as { name?: string; args?: Record<string, unknown>; result_count?: number } | undefined;
    let label = msg.content;
    if (msg.role === "tool_call" && info?.args) {
      const query = info.args.query ?? info.args.q;
      label = `Searching: ${query ?? JSON.stringify(info.args)}`;
    } else if (msg.role === "tool_result" && info?.result_count !== undefined) {
      label = msg.content;
    }
    return (
      <div key={msg.id} className={styles.msgTool} style={{ whiteSpace: "pre-line" }}>
        <span className={styles.toolIcon}>{msg.role === "tool_call" ? "\u{1F50D}" : "\u{1F4CB}"}</span>
        {label}
      </div>
    );
  }

  return (
    <div className={styles.wrapper}>
      <aside className={styles.sidebar}>
        <div className={styles.sidebarHeader}>
          <button className={styles.newChatBtn} onClick={handleNewChat}>
            + New Chat
          </button>
        </div>
        <div className={styles.convList}>
          {conversations.map((conv) => (
            <button
              key={conv.id}
              className={`${styles.convItem} ${conv.id === conversationId ? styles.convItemActive : ""}`}
              onClick={() => loadConversation(conv.id)}
            >
              <span className={styles.convTitle}>{conv.title || "Untitled"}</span>
              <span
                className={styles.convDelete}
                onClick={(e) => handleDeleteConversation(e, conv.id)}
              >
                &times;
              </span>
            </button>
          ))}
        </div>
      </aside>

      <div className={styles.main}>
        {conversationId && currentConversation && (
          <div className={styles.convHeader}>
            <span className={styles.convHeaderTitle}>
              {currentConversation.title || "Untitled"}
            </span>
            <button
              className={styles.renameBtn}
              onClick={openRenameModal}
              title="Rename conversation"
            >
              &#9998;
            </button>
          </div>
        )}

        {renameModalOpen && (
          <div className={styles.modalOverlay} onClick={() => setRenameModalOpen(false)}>
            <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
              <h3 className={styles.modalTitle}>Rename Conversation</h3>
              <input
                className={styles.modalInput}
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleRename();
                  if (e.key === "Escape") setRenameModalOpen(false);
                }}
                autoFocus
              />
              <div className={styles.modalActions}>
                <button
                  className={styles.modalCancel}
                  onClick={() => setRenameModalOpen(false)}
                >
                  Cancel
                </button>
                <button
                  className={styles.modalSave}
                  onClick={handleRename}
                  disabled={!renameValue.trim()}
                >
                  Save
                </button>
              </div>
            </div>
          </div>
        )}

        <div className={styles.messages}>
          {messages.length === 0 && !loading && (
            <div className={styles.emptyState}>
              Send a message to start a conversation
            </div>
          )}
          {messages.map((msg) => {
            if (msg.role === "user") {
              return (
                <div key={msg.id} className={styles.msgUser}>
                  {msg.content}
                </div>
              );
            }
            if (msg.role === "assistant") {
              return (
                <div key={msg.id} className={styles.msgAssistant}>
                  <Markdown remarkPlugins={[remarkGfm]}>{msg.content}</Markdown>
                </div>
              );
            }
            if (msg.role === "tool_call" || msg.role === "tool_result") {
              return renderToolMessage(msg);
            }
            return null;
          })}
          {loading && (
            <div className={styles.msgAssistant}>
              <Spinner />
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        {error && <p className={styles.error}>{error}</p>}

        <div className={styles.inputBar}>
          <textarea
            ref={textareaRef}
            className={styles.input}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
            rows={1}
            disabled={loading}
          />
          <button
            className={styles.sendBtn}
            onClick={handleSend}
            disabled={loading || !input.trim()}
          >
            {loading ? "..." : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}
