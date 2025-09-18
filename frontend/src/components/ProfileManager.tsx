import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './ProfileManager.css';

export interface ProfileInfo {
  name: string;
  created_at: string;
  last_used: string;
  description?: string;
  workspace_path: string;
  saves_path: string;
}

interface VirtualNode {
  name: string;
  path?: string;
  is_directory: boolean;
  children?: VirtualNode[];
  size?: number;
  modified?: string;
  source: 'Base' | 'Workspace' | 'Override';
  writable: boolean;
}

interface ProfileManagerProps {
  onOpenWorkspace?: (profileName: string) => void;
}

function ProfileManager({ onOpenWorkspace }: ProfileManagerProps) {
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<VirtualNode | null>(null);
  const [virtualTree, setVirtualTree] = useState<VirtualNode | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notification, setNotification] = useState<string | null>(null);
  const [isBuilding, setIsBuilding] = useState(false);
  const [buildProgress, setBuildProgress] = useState<any>(null);

  useEffect(() => {
    loadProfiles();
    setupEventListeners();
  }, []);

  useEffect(() => {
    if (selectedProfile) {
      loadVirtualTree(selectedProfile);
    }
  }, [selectedProfile]);

  const setupEventListeners = async () => {
    // Listen for workspace normalization events
    const unlisten = await listen<string>('workspace-normalized', (event) => {
      setNotification(`File normalized: ${event.payload}`);
      setTimeout(() => setNotification(null), 3000);
      
      // Reload virtual tree to show changes
      if (selectedProfile) {
        loadVirtualTree(selectedProfile);
      }
    });
    
    return () => {
      unlisten();
    };
  };

  const loadProfiles = async () => {
    try {
      setIsLoading(true);
      const profileList = await invoke<ProfileInfo[]>('list_profiles');
      setProfiles(profileList);
      
      if (profileList.length > 0 && !selectedProfile) {
        setSelectedProfile(profileList[0].name);
      }
    } catch (err) {
      console.error('Failed to load profiles:', err);
      setError(err as string);
    } finally {
      setIsLoading(false);
    }
  };

  const loadVirtualTree = async (profileName: string) => {
    try {
      const tree = await invoke<VirtualNode>('get_virtual_file_tree', { 
        profileName 
      });
      setVirtualTree(tree);
      // Auto-expand root folder
      if (tree.path) {
        setExpandedPaths(new Set([tree.path]));
      }
    } catch (err) {
      console.error('Failed to load virtual tree:', err);
      setError(err as string);
    }
  };

  const toggleExpanded = (path: string) => {
    setExpandedPaths(prev => {
      const newSet = new Set(prev);
      if (newSet.has(path)) {
        newSet.delete(path);
      } else {
        newSet.add(path);
      }
      return newSet;
    });
  };

  const handleCreateProfile = async (name: string) => {
    try {
      const newProfile = await invoke<ProfileInfo>('create_profile', { name });
      setProfiles(prev => [...prev, newProfile]);
      setSelectedProfile(name);
    } catch (err) {
      console.error('Failed to create profile:', err);
      setError(err as string);
    }
  };

  const handleDeleteProfile = async (name: string) => {
    if (!confirm(`Delete profile "${name}"? This cannot be undone.`)) {
      return;
    }
    
    try {
      await invoke('delete_profile', { name });
      setProfiles(prev => prev.filter(p => p.name !== name));
      
      if (selectedProfile === name) {
        const remaining = profiles.filter(p => p.name !== name);
        setSelectedProfile(remaining.length > 0 ? remaining[0].name : null);
      }
    } catch (err) {
      console.error('Failed to delete profile:', err);
      setError(err as string);
    }
  };

  const handleOpenWorkspace = async (profileName: string) => {
    try {
      await invoke('open_profile_workspace', { name: profileName });
      onOpenWorkspace?.(profileName);
    } catch (err) {
      console.error('Failed to open workspace:', err);
      setError(err as string);
    }
  };

  const handleFileAction = async (action: string, node: VirtualNode) => {
    if (!selectedProfile || !node.path) return;
    
    try {
      if (action === 'revert' && node.source === 'Override') {
        await invoke('revert_to_original', {
          profileName: selectedProfile,
          virtualPath: node.path
        });
        setNotification(`Reverted ${node.name} to original`);
        setTimeout(() => setNotification(null), 3000);
        loadVirtualTree(selectedProfile);
      } else if (action === 'delete' && node.source === 'Workspace') {
        if (confirm(`Delete ${node.name}? This cannot be undone.`)) {
          await invoke('delete_workspace_file', {
            profileName: selectedProfile,
            virtualPath: node.path
          });
          setNotification(`Deleted ${node.name} from workspace`);
          setTimeout(() => setNotification(null), 3000);
          loadVirtualTree(selectedProfile);
        }
      } else if (action === 'copy_to_workspace' && node.source === 'Base') {
        await invoke('copy_to_workspace', {
          profileName: selectedProfile,
          virtualPath: node.path
        });
        setNotification(`Copied ${node.name} to workspace for editing`);
        setTimeout(() => setNotification(null), 3000);
        loadVirtualTree(selectedProfile);
      } else if (action === 'debug' && (node.source === 'Workspace' || node.source === 'Override')) {
        try {
          const debugInfo = await invoke<string>('debug_blob_cache', {
            profileName: selectedProfile,
            virtualPath: node.path
          });
          console.log('Debug info for', node.path, ':\n', debugInfo);
          setNotification(`Debug info logged to console for ${node.name}`);
          setTimeout(() => setNotification(null), 3000);
        } catch (err) {
          console.error('Debug failed:', err);
          setError(`Debug failed: ${err}`);
        }
      }
    } catch (err) {
      console.error(`Failed to ${action}:`, err);
      setError(err as string);
    }
  };

  const handleBuildRuntime = async (profileName: string) => {
    if (isBuilding) return;
    
    setIsBuilding(true);
    setBuildProgress(null);
    setError(null);
    setNotification(`Building runtime for ${profileName}...`);

    try {
      // Listen for build progress events
      const unlisten = await listen('build_progress', (event: any) => {
        setBuildProgress(event.payload);
        
        if (event.payload.completed) {
          if (event.payload.error) {
            setError(`Build failed: ${event.payload.error}`);
            setNotification(null);
          } else {
            setNotification(`Runtime built successfully for ${profileName}!`);
            setTimeout(() => setNotification(null), 5000);
          }
          setIsBuilding(false);
          setBuildProgress(null);
          unlisten();
        }
      });

      // Start the build
      const result = await invoke('build_runtime', {
        profileName: profileName
      });

      console.log('Build result:', result);
    } catch (err) {
      console.error('Build failed:', err);
      setError(`Build failed: ${err}`);
      setIsBuilding(false);
      setBuildProgress(null);
      setNotification(null);
    }
  };

  const handleComputePlan = async (profileName: string) => {
    try {
      setNotification(`Computing runtime plan for ${profileName}...`);
      const plan = await invoke('compute_runtime_plan', {
        profileName: profileName
      });
      console.log('Runtime plan:', plan);
      setNotification(`Runtime plan computed: ${(plan as any).total_files} files`);
      setTimeout(() => setNotification(null), 3000);
    } catch (err) {
      console.error('Plan computation failed:', err);
      setError(`Plan computation failed: ${err}`);
    }
  };

  const renderFileTree = (node: VirtualNode, depth = 0): React.JSX.Element => {
    const indent = depth * 16;
    const isSelected = selectedFile?.path === node.path;
    const isExpanded = node.path ? expandedPaths.has(node.path) : true;
    
    return (
      <div key={node.path || node.name}>
        <div 
          className={`file-item ${isSelected ? 'selected' : ''} ${node.source.toLowerCase()}`}
          style={{ paddingLeft: `${indent}px` }}
          onClick={() => setSelectedFile(node)}
        >
          {node.is_directory && (
            <button 
              className="expand-button"
              onClick={(e) => {
                e.stopPropagation();
                if (node.path) toggleExpanded(node.path);
              }}
            >
              {isExpanded ? '▼' : '▶'}
            </button>
          )}
          <span className="file-icon">
            {node.is_directory ? '📁' : '📄'}
          </span>
          <span className="file-name">{node.name}</span>
          <span className={`file-source ${node.source.toLowerCase()}`}>
            {node.source}
          </span>
          {!node.is_directory && (
            <div className="file-actions">
              {node.source === 'Override' && (
                <button 
                  className="action-btn revert"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleFileAction('revert', node);
                  }}
                  title="Revert to base file"
                >
                  ↶ Revert
                </button>
              )}
              {node.source === 'Workspace' && (
                <button 
                  className="action-btn delete"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleFileAction('delete', node);
                  }}
                  title="Delete workspace file"
                >
                  🗑 Delete
                </button>
              )}
              {(node.source === 'Workspace' || node.source === 'Override') && (
                <button 
                  className="action-btn debug"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleFileAction('debug', node);
                  }}
                  title="Debug blob cache info"
                >
                  🔍 Debug
                </button>
              )}
              {node.source === 'Base' && (
                <button 
                  className="action-btn copy"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleFileAction('copy_to_workspace', node);
                  }}
                  title="Copy to workspace for editing"
                >
                  ✏️ Edit
                </button>
              )}
            </div>
          )}
        </div>
        {node.is_directory && node.children && isExpanded && (
          <div className="file-children">
            {node.children.map(child => renderFileTree(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  const selectedProfileData = profiles.find(p => p.name === selectedProfile);

  if (isLoading) {
    return (
      <div className="profile-manager loading">
        <div className="loading-spinner">⚡</div>
        <p>Loading profiles...</p>
      </div>
    );
  }

  return (
    <div className="profile-manager">
      {/* Notification */}
      {notification && (
        <div className="notification">
          <span className="notification-icon">⚡</span>
          <span>{notification}</span>
          <button onClick={() => setNotification(null)}>×</button>
        </div>
      )}

      {/* Error Display */}
      {error && (
        <div className="error-banner">
          <span className="error-icon">⚠️</span>
          <span>{error}</span>
          <button onClick={() => setError(null)}>×</button>
        </div>
      )}
      
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>Profiles</h2>
          <button 
            className="create-btn"
            onClick={() => {
              const name = prompt('Profile name:');
              if (name) handleCreateProfile(name);
            }}
          >
            +
          </button>
        </div>
        
        <div className="profile-list">
          {profiles.map(profile => (
            <div 
              key={profile.name}
              className={`profile-item ${selectedProfile === profile.name ? 'selected' : ''}`}
              onClick={() => setSelectedProfile(profile.name)}
            >
              <div className="profile-name">{profile.name}</div>
              <div className="profile-meta">
                Last used: {new Date(profile.last_used).toLocaleDateString()}
              </div>
              <button 
                className="delete-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  handleDeleteProfile(profile.name);
                }}
              >
                🗑️
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Main Content */}
      <div className="main-content">
        {selectedProfileData ? (
          <>
            <header className="profile-header">
              <h1>{selectedProfileData.name}</h1>
              <div className="header-actions">
                <button 
                  className="primary-btn"
                  onClick={() => handleOpenWorkspace(selectedProfileData.name)}
                >
                  📁 Open Workspace
                </button>
                <button 
                  className="secondary-btn"
                  onClick={() => handleComputePlan(selectedProfileData.name)}
                  disabled={isBuilding}
                >
                  📋 Compute Plan
                </button>
                <button 
                  className="secondary-btn"
                  onClick={() => handleBuildRuntime(selectedProfileData.name)}
                  disabled={isBuilding}
                >
                  {isBuilding ? '🔄 Building...' : '🔧 Build Runtime'}
                </button>
                <button className="primary-btn">🎮 Launch Game</button>
              </div>
            </header>

            {/* Build Progress */}
            {buildProgress && (
              <div className="build-progress">
                <div className="progress-header">
                  <h4>Building Runtime - {buildProgress.phase}</h4>
                  <span>{buildProgress.files_processed} / {buildProgress.total_files} files</span>
                </div>
                <div className="progress-bar">
                  <div 
                    className="progress-fill" 
                    style={{ width: `${(buildProgress.files_processed / buildProgress.total_files) * 100}%` }}
                  ></div>
                </div>
                {buildProgress.current_file && (
                  <div className="current-file">Processing: {buildProgress.current_file}</div>
                )}
              </div>
            )}
            
            <div className="content-sections">
              <section className="virtual-files-section">
                <h3>Virtual Game Folder</h3>
                <p className="section-description">
                  Combined view of base game files and workspace modifications. 
                  Base files are read-only, workspace files can be reverted.
                </p>
                
                <div className="file-tree">
                  {virtualTree ? renderFileTree(virtualTree) : (
                    <div className="loading-tree">Loading file tree...</div>
                  )}
                </div>
              </section>
              
              {selectedFile && (
                <section className="file-details-section">
                  <h3>File Details</h3>
                  <div className="file-details">
                    <div className="detail-row">
                      <label>Path:</label>
                      <code>{selectedFile.path}</code>
                    </div>
                    <div className="detail-row">
                      <label>Source:</label>
                      <span className={`source-badge ${selectedFile.source.toLowerCase()}`}>
                        {selectedFile.source}
                      </span>
                    </div>
                    <div className="detail-row">
                      <label>Type:</label>
                      <span>{selectedFile.is_directory ? 'Directory' : 'File'}</span>
                    </div>
                    {selectedFile.size && (
                      <div className="detail-row">
                        <label>Size:</label>
                        <span>{(selectedFile.size / 1024).toFixed(2)} KB</span>
                      </div>
                    )}
                    <div className="detail-row">
                      <label>Writable:</label>
                      <span className={selectedFile.writable ? 'writable' : 'readonly'}>
                        {selectedFile.writable ? 'Yes' : 'No (Base file)'}
                      </span>
                    </div>
                  </div>
                </section>
              )}
            </div>
          </>
        ) : (
          <div className="no-profile">
            <h2>No Profile Selected</h2>
            <p>Create a new profile or select an existing one to get started.</p>
          </div>
        )}
      </div>
    </div>
  );
}

export default ProfileManager;