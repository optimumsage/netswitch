import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { motion, AnimatePresence, Reorder } from "framer-motion";
import {
  Wifi,
  AlertCircle,
  GripVertical,
  Network,
  Cable,
  Download,
  RefreshCw,
  Shield,
  Waypoints,
} from "lucide-react";
import "./App.css";

interface InterfaceInfo {
  name: string;
  friendly_name: string;
  has_internet: boolean;
  is_primary: boolean;
}

interface DaemonState {
  version: string;
  interfaces: InterfaceInfo[];
  current_active: string | null;
  custom_order: string[];
}

type ConnStatus = "connecting" | "connected" | "missing";

const POLL_MS = 1500;
const SYNC_TIMEOUT_MS = 4000;

const ifaceKind = (iface: InterfaceInfo | undefined) => {
  if (!iface) return "other";
  const lower = (iface.friendly_name || iface.name).toLowerCase();
  if (lower.includes("wi-fi") || lower.includes("wlan") || lower.includes("wifi")) return "wifi";
  if (
    lower.includes("vpn") ||
    lower.includes("tunnel") ||
    lower.includes("utun") ||
    lower.includes("wireguard") ||
    lower.includes("tailscale")
  )
    return "vpn";
  if (
    lower.includes("ethernet") ||
    lower.includes("lan") ||
    lower.includes("thunderbolt") ||
    lower.includes("local area connection")
  )
    return "ethernet";
  return "other";
};

const formatInterfaceName = (iface: InterfaceInfo | undefined) => {
  if (!iface) return "";
  let base = iface.friendly_name || iface.name;
  switch (ifaceKind(iface)) {
    case "wifi":
      base = "Wi-Fi";
      break;
    case "ethernet":
      base = "Ethernet";
      break;
    case "vpn":
      base = "VPN";
      break;
    default:
      if (base.length > 22) base = base.substring(0, 19) + "…";
  }
  if (base === iface.name) return base;
  return `${base} · ${iface.name}`;
};

const InterfaceIcon = ({ iface, size = 20 }: { iface: InterfaceInfo | undefined; size?: number }) => {
  switch (ifaceKind(iface)) {
    case "wifi":
      return <Wifi size={size} />;
    case "vpn":
      return <Shield size={size} />;
    case "ethernet":
      return <Cable size={size} />;
    default:
      return <Network size={size} />;
  }
};

function App() {
  const [state, setState] = useState<DaemonState | null>(null);
  const [localOrder, setLocalOrder] = useState<string[]>([]);
  const [conn, setConn] = useState<ConnStatus>("connecting");
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [updateAvailable, setUpdateAvailable] = useState<Update | null>(null);
  const [isUpdatingApp, setIsUpdatingApp] = useState(false);
  const [updateProgress, setUpdateProgress] = useState(0);

  // Refs let the polling loop read the latest values without being torn down
  // and recreated on every state change.
  const errorCount = useRef(0);
  const isDragging = useRef(false);
  const localOrderRef = useRef<string[]>([]);
  const syncingRef = useRef(false);
  const syncTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    localOrderRef.current = localOrder;
  }, [localOrder]);
  useEffect(() => {
    syncingRef.current = syncing;
  }, [syncing]);

  const orderFromState = (data: DaemonState) =>
    data.custom_order.length > 0 ? data.custom_order : data.interfaces.map((i) => i.name);

  const finishSync = useCallback(() => {
    if (syncTimer.current) {
      clearTimeout(syncTimer.current);
      syncTimer.current = null;
    }
    setSyncing(false);
  }, []);

  const fetchStatus = useCallback(async () => {
    try {
      const data = await invoke<DaemonState>("get_daemon_status");
      setState(data);
      setError(null);
      errorCount.current = 0;
      setConn("connected");

      const backendOrder = orderFromState(data);

      // While the user is actively dragging we never overwrite their list.
      if (isDragging.current) return;

      if (syncingRef.current) {
        // Clear the "syncing" state once the daemon has echoed our order back,
        // regardless of which interface ended up active (prioritizing an
        // offline interface legitimately won't change the active one).
        if (JSON.stringify(data.custom_order) === JSON.stringify(localOrderRef.current)) {
          finishSync();
        }
        return; // don't fight the optimistic local order mid-sync
      }

      if (JSON.stringify(backendOrder) !== JSON.stringify(localOrderRef.current)) {
        setLocalOrder(backendOrder);
      }
    } catch {
      errorCount.current += 1;
      if (errorCount.current >= 3) {
        setConn("missing");
      }
    }
  }, [finishSync]);

  const checkForUpdates = useCallback(async () => {
    try {
      const update = await check();
      if (update) setUpdateAvailable(update);
    } catch (e) {
      console.error("Update check failed:", e);
    }
  }, []);

  // Initial load + update check.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const data = await invoke<DaemonState>("get_daemon_status");
        if (cancelled) return;
        setState(data);
        setLocalOrder(orderFromState(data));
        setConn("connected");
      } catch {
        // Leave as "connecting"; the poll loop will flip to "missing".
      }
      checkForUpdates();
    })();
    return () => {
      cancelled = true;
    };
  }, [checkForUpdates]);

  // Stable polling loop — set up once.
  useEffect(() => {
    const id = setInterval(fetchStatus, POLL_MS);
    return () => clearInterval(id);
  }, [fetchStatus]);

  // Keep the tray label in sync with the active interface.
  useEffect(() => {
    if (!state) return;
    const active = state.interfaces.find((i) => i.name === state.current_active);
    invoke("update_tray_status", {
      activeInterface: state.current_active,
      friendlyName: formatInterfaceName(active) || null,
    }).catch(() => {});
  }, [state?.current_active, state?.interfaces]);

  const handleReorder = (newOrder: string[]) => setLocalOrder(newOrder);

  const syncOrder = async () => {
    isDragging.current = false;
    if (state && JSON.stringify(state.custom_order) === JSON.stringify(localOrder)) return;

    setSyncing(true);
    // Safety net: never let the syncing UI hang if the daemon is slow/quiet.
    if (syncTimer.current) clearTimeout(syncTimer.current);
    syncTimer.current = setTimeout(() => finishSync(), SYNC_TIMEOUT_MS);

    try {
      const newState = await invoke<DaemonState>("set_interface_order", { order: localOrder });
      setState(newState);
      if (JSON.stringify(newState.custom_order) === JSON.stringify(localOrder)) {
        finishSync();
      }
    } catch (e) {
      setError(`Could not save order: ${e}`);
      finishSync();
    }
  };

  const installDaemon = async () => {
    setIsInstalling(true);
    setError(null);
    try {
      await invoke("install_daemon_service");
      errorCount.current = 0;
      setConn("connecting");
      setTimeout(fetchStatus, 3000);
    } catch (e) {
      setError(`Installation failed: ${e}`);
    } finally {
      setIsInstalling(false);
    }
  };

  const handleUpdate = async () => {
    if (!updateAvailable) return;
    setIsUpdatingApp(true);
    try {
      let downloaded = 0;
      let contentLength = 0;
      await updateAvailable.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength || 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            if (contentLength > 0) setUpdateProgress(Math.round((downloaded / contentLength) * 100));
            break;
        }
      });
      // The updater relaunches the app after install.
    } catch (e) {
      setError(`Update failed: ${e}`);
      setIsUpdatingApp(false);
    }
  };

  const activeIface = state?.interfaces?.find((i) => i.name === state?.current_active);
  const displayInterfaces = localOrder
    .map((name) => state?.interfaces?.find((i) => i.name === name))
    .filter(Boolean) as InterfaceInfo[];
  const anyOnline = !!state?.interfaces?.some((i) => i.has_internet);

  // ---- Loading state -----------------------------------------------------
  if (conn === "connecting" && !state) {
    return (
      <div className="container center">
        <motion.div
          className="boot"
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
        >
          <div className="boot-mark">
            <Waypoints size={28} />
          </div>
          <motion.div
            animate={{ rotate: 360 }}
            transition={{ repeat: Infinity, duration: 1, ease: "linear" }}
            className="boot-spinner"
          >
            <RefreshCw size={18} />
          </motion.div>
          <span>Connecting to daemon…</span>
        </motion.div>
      </div>
    );
  }

  // ---- Daemon missing state ---------------------------------------------
  if (conn === "missing" && !state) {
    return (
      <div className="container center">
        <motion.div
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          className="card setup-card"
        >
          <div className="setup-icon">
            <AlertCircle size={36} />
          </div>
          <h2>Background service required</h2>
          <p>
            The Netswitch daemon isn’t running yet. It manages your network routing in the
            background and needs to be installed once.
          </p>
          <button
            className={`btn-primary ${isInstalling ? "loading" : ""}`}
            onClick={installDaemon}
            disabled={isInstalling}
          >
            {isInstalling ? (
              <>
                <RefreshCw size={16} className="spin" /> Installing…
              </>
            ) : (
              "Install & start service"
            )}
          </button>
          {error && <p className="setup-error">{error}</p>}
        </motion.div>
      </div>
    );
  }

  const statusMeta =
    conn === "missing"
      ? { cls: "danger", label: "Disconnected" }
      : anyOnline
      ? { cls: "online", label: "Live monitoring" }
      : { cls: "warn", label: "Searching…" };

  return (
    <div className="container">
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">
            <Waypoints size={20} />
          </div>
          <div className="brand-text">
            <h1>Netswitch</h1>
            <div className="brand-sub">
              {state?.version && <span className="version-tag">v{state.version}</span>}
              <AnimatePresence>
                {updateAvailable && (
                  <motion.button
                    initial={{ opacity: 0, x: -8 }}
                    animate={{ opacity: 1, x: 0 }}
                    exit={{ opacity: 0, x: -8 }}
                    className={`update-pill ${isUpdatingApp ? "updating" : ""}`}
                    onClick={handleUpdate}
                    disabled={isUpdatingApp}
                  >
                    {isUpdatingApp ? (
                      <>
                        <RefreshCw size={12} className="spin" />
                        <span>{updateProgress}%</span>
                      </>
                    ) : (
                      <>
                        <Download size={12} />
                        <span>Update {updateAvailable.version}</span>
                      </>
                    )}
                  </motion.button>
                )}
              </AnimatePresence>
            </div>
          </div>
        </div>
        <div className={`status-indicator status-${statusMeta.cls}`}>
          <span className={`dot ${statusMeta.cls === "online" ? "dot-pulse" : ""}`} />
          {statusMeta.label}
        </div>
      </header>

      <div className="dashboard-grid">
        <AnimatePresence mode="wait">
          {activeIface ? (
            <motion.div
              key={activeIface.name}
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -12 }}
              className="hero"
            >
              <div className="hero-glow" />
              <div className="hero-icon">
                <InterfaceIcon iface={activeIface} size={30} />
              </div>
              <div className="hero-label">Active connection</div>
              <div className="hero-name">{formatInterfaceName(activeIface)}</div>
              <div className="hero-badge">
                {syncing ? (
                  <>
                    <RefreshCw size={12} className="spin" /> Switching…
                  </>
                ) : (
                  <>
                    <span className="dot" /> Primary route
                  </>
                )}
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="offline"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              className="hero hero-offline"
            >
              <div className="hero-icon">
                <AlertCircle size={30} />
              </div>
              <div className="hero-label">No connection</div>
              <div className="hero-name">Searching…</div>
              <div className="hero-badge muted">
                <span className="dot" /> Waiting for a route
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <div className="card list-card">
          <div className="section-header">
            <h2>Priority</h2>
            {syncing ? (
              <div className="saving-indicator">
                <RefreshCw size={12} className="spin" /> Saving
              </div>
            ) : (
              <div className="hint-pill">Drag to reorder</div>
            )}
          </div>

          {displayInterfaces.length === 0 ? (
            <div className="empty-list">
              <Network size={28} />
              <span>No network interfaces detected</span>
            </div>
          ) : (
            <Reorder.Group
              axis="y"
              values={localOrder}
              onReorder={handleReorder}
              className="interface-stack"
            >
              {displayInterfaces.map((iface) => (
                <Reorder.Item
                  key={iface.name}
                  value={iface.name}
                  onDragStart={() => {
                    isDragging.current = true;
                  }}
                  onDragEnd={syncOrder}
                  whileDrag={{ scale: 1.03 }}
                  className={`interface-item ${iface.is_primary ? "is-active" : ""}`}
                >
                  <div className="item-left">
                    <div className="drag-handle">
                      <GripVertical size={18} />
                    </div>
                    <div className="priority-number">{localOrder.indexOf(iface.name) + 1}</div>
                    <div className="iface-icon">
                      <InterfaceIcon iface={iface} />
                    </div>
                    <div className="iface-info">
                      <h4>{formatInterfaceName(iface)}</h4>
                    </div>
                  </div>
                  <div className="item-right">
                    <div className={`status-pill ${iface.has_internet ? "online" : "offline"}`}>
                      <span className="dot" />
                      {iface.has_internet ? "Online" : "Offline"}
                    </div>
                  </div>
                </Reorder.Item>
              ))}
            </Reorder.Group>
          )}
        </div>
      </div>

      <AnimatePresence>
        {error && (
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 20 }}
            className="error-toast"
          >
            <AlertCircle size={18} />
            <span>{error}</span>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export default App;
