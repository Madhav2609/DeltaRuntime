import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Wizard from './components/Wizard';
import ProfileManager from './components/ProfileManager';
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
        <p>Loading DeltaRuntime...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="app-error">
        <h1>Failed to Load DeltaRuntime</h1>
        <p>Error: {error}</p>
        <button onClick={loadSettings}>Retry</button>
      </div>
    );
  }

  // Show wizard if not completed
  if (!settings?.wizard.completed) {
    return <Wizard />;
  }

  // Main application interface with profile management
  return <ProfileManager />;
}

export default App;
