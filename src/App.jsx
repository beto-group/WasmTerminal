const { useState, useEffect, useRef } = dc;

function ProcessManager_Standalone(props) {
    const { domUtils: { findNearestAncestorWithClass, findDirectChildByClass }, folderPath } = props;
    const uniqueWrapperClass = "terminal-wrapper-" + useRef(Math.random().toString(36).substr(2, 9)).current;
    const canvasId = "datacore-terminal-canvas-" + useRef(Math.random().toString(36).substr(2, 9)).current;
    const containerRef = useRef(null);
    const stateRefs = useRef({}).current;

    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

    // --- Full-Tab Mode Effect ---
    useEffect(() => {
        const FULLTAB_ID = 'fulltab-503-datacoreterminal';
        let styleEl = document.getElementById(FULLTAB_ID);
        if (!styleEl) {
            styleEl = document.createElement('style');
            styleEl.id = FULLTAB_ID;
            styleEl.innerHTML = `
                body > .app-container .status-bar,
                .status-bar, .inline-title, .view-footer,
                .workspace-leaf-content-footer, .mod-footer,
                .embedded-backlinks { display: none !important; }
                .workspace-leaf-content { padding: 0 !important; margin: 0 !important; }
                .markdown-preview-view, .markdown-preview-section { padding: 0 !important; max-width: 100% !important; }
                .markdown-preview-sizer { padding: 0 !important; margin: 0 !important; min-height: unset !important; }
            `;
            document.head.appendChild(styleEl);
        }

        const container = containerRef.current;
        if (!container || !container.parentNode) return;
        
        const targetPaneContent = findNearestAncestorWithClass(container, 'workspace-leaf-content');
        if (!targetPaneContent) return;
        
        const contentWrapper = findDirectChildByClass(targetPaneContent, 'view-content') || targetPaneContent;
        stateRefs.originalParent = container.parentNode;
        stateRefs.placeholder = document.createElement('div');
        stateRefs.placeholder.style.display = 'none';
        container.parentNode.insertBefore(stateRefs.placeholder, container);
        
        const computedParentPosition = window.getComputedStyle(contentWrapper).position;
        stateRefs.parentPositionInfo = {
            element: contentWrapper,
            originalInlinePosition: contentWrapper.style.position
        };
        
        if (computedParentPosition === 'static') {
            contentWrapper.style.position = "relative";
        }
        
        contentWrapper.appendChild(container);
        Object.assign(container.style, {
            position: "absolute",
            top: "0px",
            left: "0px",
            width: "100%",
            height: "100%",
            zIndex: "9998",
            overflow: "hidden",
            backgroundColor: "var(--background-primary)"
        });
        
        return () => {
            const el = document.getElementById(FULLTAB_ID);
            if (el) el.remove();

            if (!stateRefs.originalParent) return;
            if (stateRefs.placeholder?.parentNode) {
                stateRefs.placeholder.parentNode.replaceChild(container, stateRefs.placeholder);
            } else {
                stateRefs.originalParent.appendChild(container);
            }
            if (stateRefs.parentPositionInfo?.element) {
                stateRefs.parentPositionInfo.element.style.position = stateRefs.parentPositionInfo.originalInlinePosition || '';
            }
            container.removeAttribute("style");
            Object.keys(stateRefs).forEach(key => stateRefs[key] = null);
        };
    }, []);

    // --- WASM Boot Effect ---
    useEffect(() => {
        let isMounted = true;
        let ptyProcess = null;

        async function bootWasm() {
            try {
                // 1. Try to spawn Native PTY (ttyd) on Desktop
                let ptyPort = null;
                try {
                    // This will throw if running on Mobile/Web without NodeJS integration
                    const cp = require('child_process');
                    const os = require('os');
                    const path = require('path');
                    
                    const vaultPath = dc.app.vault.adapter.getBasePath();
                    const absoluteFolderPath = path.isAbsolute(folderPath) ? folderPath : path.join(vaultPath, folderPath);
                    const ttydPath = path.join(absoluteFolderPath, 'bin', 'ttyd');
                    
                    const shell = process.env.SHELL || (os.platform() === 'win32' ? 'powershell.exe' : '/bin/sh');
                    const shellArgs = (shell.endsWith('zsh') || shell.endsWith('bash')) ? ['-l'] : [];
                    
                    console.log("[WasmTerminal] Spawning native ttyd server...");
                    const child = cp.spawn(ttydPath, ['-p', '0', '-W', '-w', vaultPath, shell, ...shellArgs], {
                        detached: true,
                        stdio: ['pipe', 'pipe', 'pipe'],
                        env: { ...process.env, TERM: 'xterm-256color' }
                    });
                    
                    ptyProcess = child;
                    
                    child.stdout.on('data', (data) => console.log("[ttyd stdout]", data.toString()));
                    
                    ptyPort = await new Promise((resolve, reject) => {
                        child.stderr.on('data', (data) => {
                            const output = data.toString();
                            console.log("[ttyd stderr]", output);
                            const match = output.match(/listening on port:? (\d+)/i);
                            if (match && match[1]) {
                                resolve(parseInt(match[1]));
                            }
                        });
                        child.on('error', reject);
                        setTimeout(() => reject(new Error("Timeout waiting for ttyd port")), 3000);
                    });
                    
                    console.log("[WasmTerminal] Native PTY listening on port:", ptyPort);
                    window.datacore_pty_url = `ws://127.0.0.1:${ptyPort}/ws`;
                } catch (e) {
                    console.log("[WasmTerminal] Native PTY unavailable (Mobile mode fallback active):", e.message);
                    window.datacore_pty_url = null;
                }

                // 2. Setup Virtual Filesystem Fallback
                const adapter = dc.app.vault.adapter;
                window.datacore_fs = {
                    list: async (path) => {
                        try {
                            const res = await adapter.list(path === '/' ? '' : path);
                            return JSON.stringify({ files: res.files, folders: res.folders, error: null });
                        } catch (e) { return JSON.stringify({ error: e.message }); }
                    },
                    read: async (path) => {
                        try {
                            const content = await adapter.read(path);
                            return JSON.stringify({ content, error: null });
                        } catch (e) { return JSON.stringify({ error: e.message }); }
                    },
                    stat: async (path) => {
                        try {
                            const stat = await adapter.stat(path === '/' ? '' : path);
                            return JSON.stringify({ stat: stat || null, error: null });
                        } catch (e) { return JSON.stringify({ error: e.message }); }
                    }
                };

                // 3. Load WASM
                const jsUrl = adapter.getResourcePath(folderPath + "/dist/datacore_terminal_core.js");
                console.log("[WasmTerminal] Importing WASM module from:", jsUrl);
                const wasmModule = await import(/* @vite-ignore */ jsUrl);
                
                console.log("[WasmTerminal] Initializing memory...");
                await wasmModule.default();

                if (!isMounted) return;
                setLoading(false);

                setTimeout(() => {
                    wasmModule.start_terminal(canvasId);
                }, 50);

            } catch (err) {
                console.error("[WasmTerminal] Boot failed:", err);
                if (isMounted) setError(err.message);
            }
        }

        bootWasm();

        return () => {
            isMounted = false;
            window.datacore_pty_url = null;
            if (ptyProcess && ptyProcess.pid) {
                try {
                    process.kill(-ptyProcess.pid, 'SIGTERM');
                    setTimeout(() => { try { process.kill(-ptyProcess.pid, 'SIGKILL'); } catch(e) {} }, 500);
                } catch(e) {
                    try { process.kill(ptyProcess.pid, 'SIGKILL'); } catch(e2) {}
                }
            }
        };
    }, [folderPath, canvasId]);

    const styles = {
        container: {
            width: '100%',
            height: '100%',
            display: 'flex',
            flexDirection: 'column',
            backgroundColor: 'var(--background-primary)',
            position: 'relative'
        },
        canvas: {
            width: '100%',
            height: '100%',
            display: 'block',
            position: 'absolute',
            top: 0,
            left: 0
        },
        loading: {
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            height: '100%',
            color: 'var(--text-muted)',
            fontFamily: 'monospace',
            flexDirection: 'column',
            gap: '12px',
            position: 'absolute',
            width: '100%',
            zIndex: 10
        },
        error: {
            color: '#FF5555',
            padding: '20px',
            fontFamily: 'monospace',
            border: '1px solid #FF5555',
            margin: '20px',
            borderRadius: '8px',
            backgroundColor: 'rgba(255, 85, 85, 0.1)',
            position: 'absolute',
            zIndex: 10
        }
    };

    // --- Native Key Event Capture ---
    useEffect(() => {
        const handleNativeKeyEvent = (e) => {
            e.stopPropagation();
        };

        const container = containerRef.current;
        if (container) {
            container.addEventListener('keydown', handleNativeKeyEvent, true);
            container.addEventListener('keyup', handleNativeKeyEvent, true);
            container.addEventListener('keypress', handleNativeKeyEvent, true);
        }

        return () => {
            if (container) {
                container.removeEventListener('keydown', handleNativeKeyEvent, true);
                container.removeEventListener('keyup', handleNativeKeyEvent, true);
                container.removeEventListener('keypress', handleNativeKeyEvent, true);
            }
        };
    }, []);

    return (
        <div ref={containerRef} className={uniqueWrapperClass} style={styles.container}>
            {error && (
                <div style={styles.error}>
                    <h3>WASM Terminal Error</h3>
                    <p>{error}</p>
                </div>
            )}
            
            {loading && !error && (
                <div style={styles.loading}>
                    <dc.Icon icon="loader" style={{ animation: 'spin 1s linear infinite', fontSize: '24px' }} />
                    <p>Initializing WebAssembly Engine...</p>
                    <style>{`@keyframes spin { 100% { transform: rotate(360deg); } }`}</style>
                </div>
            )}

            <canvas id={canvasId} style={styles.canvas}></canvas>
        </div>
    );
}

return { ProcessManager_Standalone };
