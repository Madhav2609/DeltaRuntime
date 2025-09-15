import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import ProfileSidebar from './ProfileSidebar';
import VirtualFileTree from './VirtualFileTree';
import type { VirtualNode } from './VirtualFileTree';
import './ProfileManager.css';

export interface ProfileInfo {
  name: string;
  created_at: string;
  last_used: string;
  description?: string;
  workspace_path: string;
  saves_path: string;
}

interface ProfileManagerProps {
  onOpenWorkspace?: (profileName: string) => void;
}

function ProfileManager({ onOpenWorkspace }: ProfileManagerProps) {
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<VirtualNode | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadProfiles();
  }, []);

  const loadProfiles = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const profileList = await invoke<ProfileInfo[]>('list_profiles');
      setProfiles(profileList);
      
      // Select the first profile if none selected and profiles exist
      if (!selectedProfile && profileList.length > 0) {
        setSelectedProfile(profileList[0].name);
      }
    } catch (err) {
      console.error('Failed to load profiles:', err);
      setError(err as string);
    } finally {
      setIsLoading(false);
    }
  };

  const handleCreateProfile = async (name: string) => {
    try {
      const newProfile = await invoke<ProfileInfo>('create_profile', { name });
      setProfiles(prev => [newProfile, ...prev]);
      setSelectedProfile(newProfile.name);
      return newProfile;
    } catch (err) {
      console.error('Failed to create profile:', err);
      throw err;
    }
  };

  const handleRenameProfile = async (oldName: string, newName: string) => {
    try {
      const updatedProfile = await invoke<ProfileInfo>('rename_profile', { 
        oldName, 
        newName 
      });
      
      setProfiles(prev => 
        prev.map(p => p.name === oldName ? updatedProfile : p)
      );
      
      if (selectedProfile === oldName) {
        setSelectedProfile(newName);
      }
      
      return updatedProfile;
    } catch (err) {
      console.error('Failed to rename profile:', err);
      throw err;
    }
  };

  const handleDeleteProfile = async (name: string) => {
    try {
      await invoke('delete_profile', { name });
      setProfiles(prev => prev.filter(p => p.name !== name));
      
      if (selectedProfile === name) {
        const remainingProfiles = profiles.filter(p => p.name !== name);
        setSelectedProfile(remainingProfiles.length > 0 ? remainingProfiles[0].name : null);
      }
    } catch (err) {
      console.error('Failed to delete profile:', err);
      throw err;
    }
  };

  const handleOpenWorkspace = async (profileName: string) => {
    try {
      await invoke('open_profile_workspace', { name: profileName });
      onOpenWorkspace?.(profileName);
    } catch (err) {
      console.error('Failed to open workspace:', err);
      throw err;
    }
  };

  const selectedProfileData = profiles.find(p => p.name === selectedProfile);

  if (isLoading) {
    return (
      <div className="profile-manager loading">
        <div className="loading-spinner"></div>
        <p>Loading profiles...</p>
      </div>
    );
  }

  return (
    <div className="profile-manager">
      <ProfileSidebar
        profiles={profiles}
        selectedProfile={selectedProfile}
        onSelectProfile={setSelectedProfile}
        onCreateProfile={handleCreateProfile}
        onRenameProfile={handleRenameProfile}
        onDeleteProfile={handleDeleteProfile}
        onOpenWorkspace={handleOpenWorkspace}
        error={error}
      />
      
      <div className="profile-content">
        {selectedProfileData ? (
          <div className="profile-details">
            <header className="profile-header">
              <h1>Profile: {selectedProfileData.name}</h1>
              <div className="profile-actions">
                <button 
                  className="action-button primary"
                  onClick={() => handleOpenWorkspace(selectedProfileData.name)}
                >
                  Open Workspace
                </button>
                <button className="action-button">Launch Game</button>
              </div>
            </header>
            
            <div className="profile-info">
              <div className="info-section">
                <h3>Profile Information</h3>
                <div className="info-grid">
                  <div className="info-item">
                    <label>Created:</label>
                    <span>{selectedProfileData.created_at}</span>
                  </div>
                  <div className="info-item">
                    <label>Last Used:</label>
                    <span>{selectedProfileData.last_used}</span>
                  </div>
                  <div className="info-item">
                    <label>Workspace:</label>
                    <code>{selectedProfileData.workspace_path}</code>
                  </div>
                  <div className="info-item">
                    <label>Saves:</label>
                    <code>{selectedProfileData.saves_path}</code>
                  </div>
                </div>
              </div>
              
              <div className="info-section">
                <h3>Virtual Game Folder</h3>
                <p className="section-description">
                  This shows your game files with workspace modifications overlaid. 
                  Base files are read-only, workspace files are editable.
                </p>
                <div className="virtual-tree-container">
                  <VirtualFileTree 
                    profileName={selectedProfileData.name}
                    onFileSelect={setSelectedFile}
                    onFileAction={(action, node) => {
                      console.log(`File ${action}:`, node);
                      // Could show notifications or update UI based on action
                    }}
                  />
                </div>
              </div>
              
              {selectedFile && (
                <div className="info-section">
                  <h3>Selected File Details</h3>
                  <div className="file-details">
                    <div className="detail-item">
                      <label>Path:</label>
                      <code>{selectedFile.path || '/'}</code>
                    </div>
                    <div className="detail-item">
                      <label>Source:</label>
                      <span className={`source-badge ${selectedFile.source.toLowerCase()}`}>
                        {selectedFile.source}
                      </span>
                    </div>
                    <div className="detail-item">
                      <label>Type:</label>
                      <span>{selectedFile.is_directory ? 'Directory' : 'File'}</span>
                    </div>
                    {selectedFile.size !== undefined && (
                      <div className="detail-item">
                        <label>Size:</label>
                        <span>{(selectedFile.size / 1024).toFixed(2)} KB</span>
                      </div>
                    )}
                    {selectedFile.modified && (
                      <div className="detail-item">
                        <label>Modified:</label>
                        <span>{selectedFile.modified}</span>
                      </div>
                    )}
                    <div className="detail-item">
                      <label>Writable:</label>
                      <span className={selectedFile.writable ? 'writable' : 'readonly'}>
                        {selectedFile.writable ? 'Yes' : 'No (Base file)'}
                      </span>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="no-profile-selected">
            <h2>No Profile Selected</h2>
            <p>Create a new profile or select an existing one to get started.</p>
          </div>
        )}
      </div>
    </div>
  );
}

export default ProfileManager;