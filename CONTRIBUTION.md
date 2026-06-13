# 🛠️ Contributing to Datacore Terminal (main)

Welcome! This document outlines the core developer standards, process management frameworks, and compilation guidelines required to maintain the advanced implementation of the Datacore Terminal.

---

## 🏛️ Core Architecture Pillars

1.  **Full-Pane DOM Interception**:
    *   The view targets the nearest `.workspace-leaf-content` ancestor and replaces standard Markdown leaves with a full-pane portal overlay.
    *   Dynamic lifecycle hooks manage mounting and cleanups edge-to-edge.
2.  **Anti-Bleed Style Isolation**:
    *   All styles must be scoped tightly under standard container class keys to avoid spilling into the Obsidian UI or interfering with active user themes.
3.  **Native Terminal Execution & Lifecycle Safety**:
    *   This component spawns a bundled `ttyd` server binary that hosts the terminal session.
    *   Ensures that the child `ttyd` server and all its subprocesses (e.g. your active shell) are killed cleanly when the component unmounts using Unix process group signals (`process.kill(-pid)`).
4.  **Sterile Zero-Dependency Flow**:
    *   The view must rely strictly on standard pre-loaded React hooks (`useState`, `useEffect`, `useRef`) provided by the `dc` host workspace compiler leaf and Node.js built-in APIs (`child_process`, `path`, `os`).

---

## 🚀 Local Compilation & Test Runner Loop

*   **Hot Reload Trigger**: During development, use the reload action menu or press the reload button inside the UI panel to invoke `dc.app.workspace.activeLeaf.rebuildView()`. This automatically flushes Obsidian's internal module cache, loading your latest React changes instantly with zero system reboots.
