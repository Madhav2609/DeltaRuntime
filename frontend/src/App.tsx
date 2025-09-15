import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Wizard from './components/Wizard';
import type { Settings } from './components/Wizard';
import './App.css';

function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const loadedSettings = await invoke<Settings>('load_settings');
      setSettings(loadedSettings);
    } catch (err) {
      console.error('Failed to load settings:', err);
      setError(err as string);
    } finally {
      setIsLoading(false);
    }
  };

  if (isLoading) {
    return (
      <div className="app-loading">
        <div className="loading-spinner"></div>
        <p>Loading GTA:SA Mod Launcher...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="app-error">
        <h1>Failed to Load Launcher</h1>
        <p>Error: {error}</p>
        <button onClick={loadSettings}>Retry</button>
      </div>
    );
  }

  // Show wizard if not completed
  if (!settings?.wizard.completed) {
    return <Wizard />;
  }

  // Main application interface (placeholder for now)
  return (
    <div className="app">
      <header className="app-header">
        <h1>GTA:SA Mod Launcher</h1>
        <p>Welcome to your mod launcher!</p>
      </header>

      <main className="app-main">
        <div className="status-panel">
          <h2>Launcher Status</h2>
          <div className="status-item">
            <strong>Base Path:</strong> <code>{settings.base_path}</code>
          </div>
          <div className="status-item">
            <strong>Data Root:</strong> <code>{settings.data_root}</code>
          </div>
          <div className="status-item">
            <strong>Wizard Completed:</strong> {settings.wizard.completed_at || 'Unknown'}
          </div>
        </div>

        <div className="actions-panel">
          <h2>Quick Actions</h2>
          <button className="action-button">Create New Profile</button>
          <button className="action-button">Manage Profiles</button>
          <button className="action-button">Open Data Root</button>
          <button className="action-button">Settings</button>
        </div>
      </main>
    </div>
  );
}

export default App;
