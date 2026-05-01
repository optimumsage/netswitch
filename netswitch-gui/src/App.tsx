import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import { motion, AnimatePresence, Reorder } from "framer-motion";
import { 
  Wifi, 
  Activity, 
  AlertCircle, 
  ChevronRight, 
  GripVertical,
  Network,
  Download,
  RefreshCw
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

function App() {
  const [state, setState] = useState<DaemonState | null>(null);
  const [localOrder, setLocalOrder] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isUpdatingOrder, setIsUpdatingOrder] = useState(false);
  const [isDaemonMissing, setIsDaemonMissing] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [updateAvailable, setUpdateAvailable] = useState<any>(null);
  const [isUpdatingApp, setIsUpdatingApp] = useState(false);
  const [updateProgress, setUpdateProgress] = useState(0);

  const errorCount = useRef(0);
  const isDragging = useRef(false);

  const targetPrimary = useRef<string | null>(null);
  const updateStartTime = useRef<number>(0);

  const checkForUpdates = async () => {
    try {
      const update = await check();
      if (update) {
        setUpdateAvailable(update);
      }
    } catch (e) {
      // Failed to check for updates
    }
  };

  const handleUpdate = async () => {
    if (!updateAvailable) return;
    setIsUpdatingApp(true);
    try {
      let downloaded = 0;
      let contentLength = 0;
      
      await updateAvailable.downloadAndInstall((event: any) => {
        switch (event.event) {
          case 'Started':
            contentLength = event.data.contentLength || 0;
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            if (contentLength > 0) {
                setUpdateProgress(Math.round((downloaded / contentLength) * 100));
            }
            break;
        }
      });

      await invoke("install_daemon_service");
    } catch (e) {
      setError(`Update failed: ${e}`);
      setIsUpdatingApp(false);
    }
  };

  const fetchStatus = useCallback(async () => {
    if (isDragging.current) return;

    try {
      const data = await invoke<DaemonState>("get_daemon_status");
      setState(data);
      setError(null);
      errorCount.current = 0;
      setIsDaemonMissing(false);
      
      if (isUpdatingOrder) {
          const timeElapsed = Date.now() - updateStartTime.current;
          const isTargetActive = data.current_active === targetPrimary.current;
          const isOrderSynced = JSON.stringify(data.custom_order) === JSON.stringify(localOrder);
          
          if (isTargetActive || (isOrderSynced && timeElapsed > 3000) || timeElapsed > 15000) {
              setIsUpdatingOrder(false);
              targetPrimary.current = null;
          }
      }

      if (!isUpdatingOrder && !isDragging.current) {
          const backendOrder = data.custom_order.length > 0 
              ? data.custom_order 
              : data.interfaces.map(i => i.name);
          
          if (JSON.stringify(backendOrder) !== JSON.stringify(localOrder)) {
              setLocalOrder(backendOrder);
          }
      }
    } catch (e: any) {
      errorCount.current += 1;
      if (errorCount.current > 3) {
        setIsDaemonMissing(true);
        setError("Daemon connection lost. Retrying...");
      }
    }
  }, [localOrder, isUpdatingOrder]);

  const installDaemon = async () => {
    setIsInstalling(true);
    try {
      await invoke("install_daemon_service");
      setTimeout(fetchStatus, 3000);
    } catch (e: any) {
      setError(`Installation failed: ${e}`);
    } finally {
      setIsInstalling(false);
    }
  };

  useEffect(() => {
    if (state && localOrder.length === 0 && !isDragging.current && !isUpdatingOrder) {
        const backendOrder = state.custom_order.length > 0 
            ? state.custom_order 
            : state.interfaces.map(i => i.name);
        setLocalOrder(backendOrder);
    }
  }, [state, localOrder.length]);

  useEffect(() => {
    const init = async () => {
      try {
        const data = await invoke<DaemonState>("get_daemon_status");
        setState(data);
        const initialOrder = data.custom_order.length > 0 
            ? data.custom_order 
            : data.interfaces.map(i => i.name);
        setLocalOrder(initialOrder);
      } catch (e) {
        // Initial fetch failed
      }
      checkForUpdates();
    };
    init();
  }, []); 

  useEffect(() => {
    const interval = setInterval(fetchStatus, 1500); 
    return () => clearInterval(interval);
  }, [fetchStatus]);

  useEffect(() => {
    if (state) {
      const active = state.interfaces.find(i => i.name === state.current_active);
      invoke("update_tray_status", { 
        activeInterface: state.current_active, 
        friendlyName: active?.friendly_name || active?.name || null 
      }).catch(() => {});
    }
  }, [state?.current_active, state?.interfaces]);

  const handleReorder = (newOrder: string[]) => {
    setLocalOrder(newOrder);
  };

  const syncOrder = async () => {
    isDragging.current = false;
    if (state && JSON.stringify(state.custom_order) === JSON.stringify(localOrder)) {
      return;
    }

    setIsUpdatingOrder(true);
    targetPrimary.current = localOrder[0];
    updateStartTime.current = Date.now();
    
    try {
      const newState = await invoke<DaemonState>("set_interface_order", { order: localOrder });
      setState(newState);
      if (newState.current_active === targetPrimary.current) {
          setIsUpdatingOrder(false);
          targetPrimary.current = null;
      }
    } catch (e) {
      setIsUpdatingOrder(false);
    }
  };

  const getIcon = (name: string) => {
    if (name.toLowerCase().includes("wi-fi") || name.toLowerCase().includes("wlan") || name.startsWith("en1")) {
      return <Wifi size={20} />;
    }
    return <Network size={20} />;
  };

  const activeIface = state?.interfaces?.find(i => i.name === state?.current_active);
  const displayInterfaces = (localOrder.map(name => state?.interfaces?.find(i => i.name === name)).filter(Boolean) as InterfaceInfo[]) || [];

  if (!state && !isDaemonMissing) {
    return (
        <div className="container" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <motion.div animate={{ rotate: 360 }} transition={{ repeat: Infinity, duration: 1, ease: "linear" }}>
                <RefreshCw size={32} color="#3b82f6" />
            </motion.div>
        </div>
    );
  }

  if (isDaemonMissing && !state) {
    return (
      <div className="container" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} className="card daemon-setup" style={{ textAlign: 'center', padding: '40px' }}>
          <AlertCircle size={64} color="#ef4444" style={{ margin: '0 auto 24px' }} />
          <h2 style={{ marginBottom: '16px' }}>Daemon Required</h2>
          <p style={{ color: '#64748b', marginBottom: '32px' }}>The Netswitch background service is not running.</p>
          <button className={`setup-button ${isInstalling ? 'loading' : ''}`} onClick={installDaemon} disabled={isInstalling}>
            {isInstalling ? 'Installing...' : 'Setup & Start Daemon'}
          </button>
        </motion.div>
      </div>
    );
  }

  return (
    <div className="container">
      <header>
        <div style={{ display: 'flex', flexDirection: 'column' }}>
          <motion.h1 initial={{ opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }}>Netswitch</motion.h1>
          {state?.version && (
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <span className="version-tag">v{state.version}</span>
                <AnimatePresence>
                    {updateAvailable && (
                        <motion.button initial={{ opacity: 0, x: -10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }} className={`update-pill ${isUpdatingApp ? 'updating' : ''}`} onClick={handleUpdate} disabled={isUpdatingApp}>
                            {isUpdatingApp ? <><RefreshCw size={12} className="spin" /><span>{updateProgress}%</span></> : <><Download size={12} /><span>Update to v{updateAvailable.version}</span></>}
                        </motion.button>
                    )}
                </AnimatePresence>
            </div>
          )}
        </div>
        <div className="status-indicator status-online"><div className="dot dot-pulse" />Live Monitoring</div>
      </header>

      <div className="dashboard-grid">
        <AnimatePresence mode="wait">
          {activeIface ? (
            <motion.div key={activeIface.name} initial={{ opacity: 0, x: -20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 20 }} className="active-summary">
              <div className="icon"><Activity size={24} /></div>
              <div className="details"><h3>Active Interface</h3><p>{activeIface.friendly_name || activeIface.name}</p></div>
              <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: '8px' }}><span className="status-badge">Primary</span><ChevronRight size={20} color="#3b82f6" /></div>
            </motion.div>
          ) : (
             <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="active-summary offline">
                <div className="icon"><AlertCircle size={24} /></div>
                <div className="details"><h3>System Offline</h3><p>Searching...</p></div>
             </motion.div>
          )}
        </AnimatePresence>

        <div className="card">
          <div className="section-header">
            <h2>Priority List</h2>
            {isUpdatingOrder ? <div className="saving-indicator">Syncing...</div> : <div className="hint-pill">Drag to Prioritize</div>}
          </div>
          <Reorder.Group axis="y" values={localOrder} onReorder={handleReorder} className="interface-stack">
            {displayInterfaces.map((iface) => (
              <Reorder.Item key={iface.name} value={iface.name} onDragStart={() => { isDragging.current = true; }} onDragEnd={syncOrder} whileDrag={{ scale: 1.02 }} className={`interface-item ${iface.is_primary ? 'is-active' : ''}`}>
                <div className="item-left">
                  <div className="drag-handle"><GripVertical size={20} /></div>
                  <div className="iface-icon">{getIcon(iface.friendly_name || iface.name)}</div>
                  <div className="iface-info"><h4>{iface.friendly_name || iface.name}</h4><p>{iface.name}</p></div>
                </div>
                <div className="item-right">
                  <div className={`status-pill ${iface.has_internet ? 'online' : 'offline'}`}><div className="dot" />{iface.has_internet ? 'Connected' : 'Offline'}</div>
                  <div className="priority-number">{localOrder.indexOf(iface.name) + 1}</div>
                </div>
              </Reorder.Item>
            ))}
          </Reorder.Group>
        </div>
      </div>
      {error && <div className="error-toast"><AlertCircle size={20} /><span>{error}</span></div>}
    </div>
  );
}

export default App;
