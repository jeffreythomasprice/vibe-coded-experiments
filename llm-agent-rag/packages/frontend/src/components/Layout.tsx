import { useState } from "react";
import { DocumentsPage } from "./DocumentsPage";
import { QueryPage } from "./QueryPage";
import styles from "./Layout.module.css";

type Tab = "documents" | "query";

export function Layout() {
  const [tab, setTab] = useState<Tab>("documents");

  return (
    <div>
      <nav className={styles.nav}>
        <h1 className={styles.title}>RAG System</h1>
        <div className={styles.tabs}>
          <button
            className={`${styles.tab} ${tab === "documents" ? styles.active : ""}`}
            onClick={() => setTab("documents")}
          >
            Documents
          </button>
          <button
            className={`${styles.tab} ${tab === "query" ? styles.active : ""}`}
            onClick={() => setTab("query")}
          >
            Query
          </button>
        </div>
      </nav>
      <main>
        {tab === "documents" ? <DocumentsPage /> : <QueryPage />}
      </main>
    </div>
  );
}
