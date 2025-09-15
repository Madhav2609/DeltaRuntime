import { useState } from 'react';
import type { ProfileInfo } from './ProfileManager';
import './ProfileSidebar.css';

interface ProfileSidebarProps {
  profiles: ProfileInfo[];
  selectedProfile: string | null;
  onSelectProfile: (profileName: string) => void;
  onCreateProfile: (name: string) => Promise<ProfileInfo>;
  onRenameProfile: (oldName: string, newName: string) => Promise<ProfileInfo>;
  onDeleteProfile: (name: string) => Promise<void>;
  onOpenWorkspace: (profileName: string) => Promise<void>;
  error: string | null;
}

function ProfileSidebar({
  profiles,
  selectedProfile,
  onSelectProfile,
  onCreateProfile,
  onRenameProfile,
  onDeleteProfile,
  onOpenWorkspace,
  error
}: ProfileSidebarProps) {
  const [isCreating, setIsCreating] = useState(false);
  const [newProfileName, setNewProfileName] = useState('');
  const [editingProfile, setEditingProfile] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [actionError, setActionError] = useState<string | null>(null);

  const handleCreateProfile = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newProfileName.trim()) return;

    setActionError(null);
    try {
      await onCreateProfile(newProfileName.trim());
      setNewProfileName('');
      setIsCreating(false);
    } catch (err) {
      setActionError(err as string);
    }
  };

  const handleRenameProfile = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editName.trim() || !editingProfile) return;

    setActionError(null);
    try {
      await onRenameProfile(editingProfile, editName.trim());
      setEditingProfile(null);
      setEditName('');
    } catch (err) {
      setActionError(err as string);
    }
  };

  const handleDeleteProfile = async (profileName: string) => {
    if (!confirm(`Are you sure you want to delete the profile "${profileName}"? This action cannot be undone.`)) {
      return;
    }

    setActionError(null);
    try {
      await onDeleteProfile(profileName);
    } catch (err) {
      setActionError(err as string);
    }
  };

  const startEditing = (profileName: string) => {
    setEditingProfile(profileName);
    setEditName(profileName);
    setActionError(null);
  };

  const cancelEditing = () => {
    setEditingProfile(null);
    setEditName('');
    setActionError(null);
  };

  return (
    <div className="profile-sidebar">
      <div className="sidebar-header">
        <h2>Profiles</h2>
        <button 
          className="create-button"
          onClick={() => {
            setIsCreating(true);
            setActionError(null);
          }}
          disabled={isCreating}
        >
          + New
        </button>
      </div>

      {(error || actionError) && (
        <div className="error-message">
          {error || actionError}
        </div>
      )}

      {isCreating && (
        <form className="profile-form" onSubmit={handleCreateProfile}>
          <input
            type="text"
            value={newProfileName}
            onChange={(e) => setNewProfileName(e.target.value)}
            placeholder="Profile name"
            autoFocus
            maxLength={50}
          />
          <div className="form-actions">
            <button type="submit" className="save-button">Create</button>
            <button 
              type="button" 
              className="cancel-button"
              onClick={() => {
                setIsCreating(false);
                setNewProfileName('');
                setActionError(null);
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      )}

      <div className="profiles-list">
        {profiles.length === 0 ? (
          <div className="empty-state">
            <p>No profiles yet.</p>
            <p>Create your first profile to get started!</p>
          </div>
        ) : (
          profiles.map((profile) => (
            <div
              key={profile.name}
              className={`profile-item ${selectedProfile === profile.name ? 'selected' : ''}`}
            >
              {editingProfile === profile.name ? (
                <form className="profile-form inline" onSubmit={handleRenameProfile}>
                  <input
                    type="text"
                    value={editName}
                    onChange={(e) => setEditName(e.target.value)}
                    autoFocus
                    maxLength={50}
                  />
                  <div className="form-actions">
                    <button type="submit" className="save-button">Save</button>
                    <button type="button" className="cancel-button" onClick={cancelEditing}>
                      Cancel
                    </button>
                  </div>
                </form>
              ) : (
                <>
                  <div 
                    className="profile-info"
                    onClick={() => onSelectProfile(profile.name)}
                  >
                    <div className="profile-name">{profile.name}</div>
                    <div className="profile-meta">
                      Last used: {new Date(profile.last_used).toLocaleDateString()}
                    </div>
                  </div>
                  
                  <div className="profile-actions">
                    <button 
                      className="action-btn"
                      onClick={() => onOpenWorkspace(profile.name)}
                      title="Open workspace in Explorer"
                    >
                      📁
                    </button>
                    <button 
                      className="action-btn"
                      onClick={() => startEditing(profile.name)}
                      title="Rename profile"
                    >
                      ✏️
                    </button>
                    <button 
                      className="action-btn delete"
                      onClick={() => handleDeleteProfile(profile.name)}
                      title="Delete profile"
                    >
                      🗑️
                    </button>
                  </div>
                </>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

export default ProfileSidebar;