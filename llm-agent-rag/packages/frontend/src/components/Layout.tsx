import { NavLink, Routes, Route, Navigate } from "react-router";
import { DocumentsPage } from "./DocumentsPage";
import { QueryPage } from "./QueryPage";
import { AgentPage } from "./AgentPage";
import styles from "./Layout.module.css";

export function Layout() {
  return (
    <div>
      <nav className={styles.nav}>
        <h1 className={styles.title}>RAG System</h1>
        <div className={styles.tabs}>
          <NavLink
            to="/documents"
            className={({ isActive }) =>
              `${styles.tab} ${isActive ? styles.active : ""}`
            }
          >
            Documents
          </NavLink>
          <NavLink
            to="/query"
            className={({ isActive }) =>
              `${styles.tab} ${isActive ? styles.active : ""}`
            }
          >
            Query
          </NavLink>
          <NavLink
            to="/agent"
            className={({ isActive }) =>
              `${styles.tab} ${isActive ? styles.active : ""}`
            }
          >
            Agent
          </NavLink>
        </div>
      </nav>
      <main>
        <Routes>
          <Route path="/documents" element={<DocumentsPage />} />
          <Route path="/query" element={<QueryPage />} />
          <Route path="/agent" element={<AgentPage />} />
          <Route path="*" element={<Navigate to="/documents" replace />} />
        </Routes>
      </main>
    </div>
  );
}
