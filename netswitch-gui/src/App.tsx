import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence, Reorder } from "framer-motion";
import { 
  Wifi, 
  Activity, 
  AlertCircle, 
  ChevronRight, 
  GripVertical,
  Network
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

  const errorCount = useRef(0);
  const isDragging = useRef(false);

  const targetPrimary = useRef<string | null>(null);
  const updateStartTime = useRef<number>(0);

  const fetchStatus = useCallback(async () => {
    // Skip polling if the user is currently dragging an item
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
          
          console.log("Sync Check:", {
            target: targetPrimary.current,
            current: data.current_active,
            isTargetActive,
            isOrderSynced,
            timeElapsed
          });

          if (isTargetActive || (isOrderSynced && timeElapsed > 3000) || timeElapsed > 15000) {
              console.log("Sync Clearing Condition Met");
              setIsUpdatingOrder(false);
              targetPrimary.current = null;
          }
      }

      if (!isUpdatingOrder && !isDragging.current) {
          const backendOrder = data.custom_order.length > 0 
              ? data.custom_order 
              : data.interfaces.map(i => i.name);
          
          // Only update local order from backend if we aren't currently "Saving..."
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
      // Wait a bit for service to start
      setTimeout(fetchStatus, 3000);
    } catch (e: any) {
      console.error("Installation failed", e);
      setError(`Installation failed: ${e}`);
    } finally {
      setIsInstalling(false);
    }
  };

  // Initial Load - Only once
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
        console.error("Initial fetch failed", e);
      }
    };
    init();
  }, []); 

  // Polling Interval
  useEffect(() => {
    const interval = setInterval(fetchStatus, 1500); 
    return () => clearInterval(interval);
  }, [fetchStatus]);

  const handleReorder = (newOrder: string[]) => {
    setLocalOrder(newOrder);
  };

  const syncOrder = async () => {
    isDragging.current = false;
    
    // Check if order actually changed compared to backend
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
      console.error("Failed to update order", e);
      setIsUpdatingOrder(false);
    }
  };

  const getIcon = (name: string) => {
    if (name.toLowerCase().includes("wi-fi") || name.toLowerCase().includes("wlan") || name.startsWith("en1")) {
      return <Wifi size={20} />;
    }
    return <Network size={20} />;
  };

  const activeIface = state?.interfaces.find(i => i.name === state.current_active);
  const displayInterfaces = localOrder.map(name => state?.interfaces.find(i => i.name === name)).filter(Boolean) as InterfaceInfo[];

  if (isDaemonMissing && !state) {
    return (
      <div className="container" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <motion.div 
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="card daemon-setup"
          style={{ textAlign: 'center', padding: '40px' }}
        >
          <div className="setup-icon" style={{ marginBottom: '24px' }}>
            <AlertCircle size={64} color="#ef4444" style={{ margin: '0 auto' }} />
          </div>
          <h2 style={{ marginBottom: '16px' }}>Daemon Required</h2>
          <p style={{ color: '#64748b', marginBottom: '32px' }}>
            The Netswitch background service is not running. It is required to monitor network interfaces and manage routing.
          </p>
          
          <button 
            className={`setup-button ${isInstalling ? 'loading' : ''}`}
            onClick={installDaemon}
            disabled={isInstalling}
          >
            {isInstalling ? 'Installing Service...' : 'Setup & Start Daemon'}
          </button>
          
          <p style={{ fontSize: '0.75rem', color: '#94a3b8', marginTop: '24px' }}>
            This requires administrator privileges.
          </p>
        </motion.div>
      </div>
    );
  }

  return (
    <div className="container">
      <header>
        <div style={{ display: 'flex', flexDirection: 'column' }}>
          <motion.h1 
            initial={{ opacity: 0, scale: 0.9 }}
            animate={{ opacity: 1, scale: 1 }}
          >
            Netswitch
          </motion.h1>
          {state?.version && (
            <motion.span 
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              className="version-tag"
            >
              v{state.version}
            </motion.span>
          )}
        </div>
        <motion.div 
          initial={{ opacity: 0, x: 20 }}
          animate={{ opacity: 1, x: 0 }}
          className="status-indicator status-online"
        >
          <div className="dot dot-pulse" />
          Live Monitoring
        </motion.div>
      </header>

      <div className="dashboard-grid">
        <AnimatePresence mode="wait">
          {activeIface ? (
            <motion.div 
              key={activeIface.name}
              initial={{ opacity: 0, x: -20 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 20 }}
              className="active-summary"
            >
              <div className="icon">
                <Activity size={24} />
              </div>
              <div className="details">
                <h3>Active Interface</h3>
                <p>{activeIface.friendly_name || activeIface.name}</p>
              </div>
              <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: '8px' }}>
                 <span className="status-badge">Primary</span>
                 <ChevronRight size={20} color="#3b82f6" />
              </div>
            </motion.div>
          ) : (
             <motion.div 
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              className="active-summary offline"
             >
                <div className="icon">
                    <AlertCircle size={24} />
                </div>
                <div className="details">
                    <h3>System Offline</h3>
                    <p>Searching for connectivity...</p>
                </div>
             </motion.div>
          )}
        </AnimatePresence>

        <div className="card">
          <div className="section-header">
            <h2>Priority List</h2>
            <AnimatePresence>
              {isUpdatingOrder ? (
                <motion.div 
                  initial={{ opacity: 0, scale: 0.8 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.8 }}
                  className="saving-indicator"
                >
                  <motion.div 
                    animate={{ rotate: 360 }}
                    transition={{ repeat: Infinity, duration: 1, ease: "linear" }}
                  >
                    <Network size={14} />
                  </motion.div>
                  Syncing Priority...
                </motion.div>
              ) : (
                <div className="hint-pill">Drag to Prioritize</div>
              )}
            </AnimatePresence>
          </div>

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
                onDragStart={() => { isDragging.current = true; }}
                onDragEnd={syncOrder}
                whileDrag={{ scale: 1.02, boxShadow: "0 20px 25px -5px rgb(0 0 0 / 0.1)" }}
                className={`interface-item ${iface.is_primary ? 'is-active' : ''} ${isUpdatingOrder ? 'lazy-loading' : ''}`}
              >
                {isUpdatingOrder && (
                  <div className="updating-overlay">
                    Saving...
                  </div>
                )}
                <div className="item-left">
                  <div className="drag-handle">
                    <GripVertical size={20} />
                  </div>
                  <div className="iface-icon">
                    {getIcon(iface.friendly_name || iface.name)}
                  </div>
                  <div className="iface-info">
                    <h4>{iface.friendly_name || iface.name}</h4>
                    <p>{iface.name}</p>
                  </div>
                </div>

                <div className="item-right">
                  <div className={`status-pill ${iface.has_internet ? 'online' : 'offline'}`}>
                    <div className="dot" />
                    {iface.has_internet ? 'Connected' : 'Offline'}
                  </div>
                  <div className="priority-number">
                    {localOrder.indexOf(iface.name) + 1}
                  </div>
                </div>
              </Reorder.Item>
            ))}
          </Reorder.Group>
        </div>
      </div>

      <AnimatePresence>
        {error && (
          <motion.div 
            initial={{ opacity: 0, y: 50 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 50 }}
            className="error-toast"
          >
            <AlertCircle size={20} />
            <span>{error}</span>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export default App;
