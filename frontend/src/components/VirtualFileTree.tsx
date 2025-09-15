import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './VirtualFileTree.css';

export interface VirtualNode {
  name: string;
  path: string;
  is_directory: boolean;
  size?: number;
  source: 'Base' | 'Workspace' | 'WorkspaceOverride' | 'Tombstone';
  writable: boolean;
  children?: VirtualNode[];
  modified?: string;
}

interface VirtualFileTreeProps {
  profileName: string;
  onFileSelect?: (node: VirtualNode) => void;
  onFileAction?: (action: string, node: VirtualNode) => void;
}

interface FileContextMenu {
  x: number;
  y: number;
  node: VirtualNode;
}

function VirtualFileTree({ profileName, onFileSelect, onFileAction }: VirtualFileTreeProps) {
  const [rootNode, setRootNode] = useState<VirtualNode | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<FileContextMenu | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadVirtualTree();
  }, [profileName]);

  useEffect(() => {
    const handleClickOutside = () => setContextMenu(null);
    document.addEventListener('click', handleClickOutside);
    return () => document.removeEventListener('click', handleClickOutside);
  }, []);

  const loadVirtualTree = async () => {
    if (!profileName) return;
    
    setIsLoading(true);
    setError(null);
    try {
      const tree = await invoke<VirtualNode>('get_virtual_file_tree', {
        profileName,
        virtualPath: null
      });
      setRootNode(tree);
      
      // Auto-expand root
      setExpandedPaths(new Set([tree.path]));
    } catch (err) {
      console.error('Failed to load virtual file tree:', err);
      setError(err as string);
    } finally {
      setIsLoading(false);
    }
  };

  const loadChildren = async (node: VirtualNode) => {
    if (!node.is_directory || node.children) return;

    try {
      const nodeWithChildren = await invoke<VirtualNode>('get_virtual_file_tree', {
        profileName,
        virtualPath: node.path || null
      });
      
      setRootNode(prev => {
        if (!prev) return prev;
        return updateNodeInTree(prev, node.path, { ...node, children: nodeWithChildren.children });
      });
    } catch (err) {
      console.error('Failed to load children:', err);
      setError(err as string);
    }
  };

  const updateNodeInTree = (tree: VirtualNode, targetPath: string, updatedNode: VirtualNode): VirtualNode => {
    if (tree.path === targetPath) {
      return updatedNode;
    }
    
    if (tree.children) {
      return {
        ...tree,
        children: tree.children.map(child => updateNodeInTree(child, targetPath, updatedNode))
      };
    }
    
    return tree;
  };

  const toggleExpanded = async (node: VirtualNode) => {
    if (!node.is_directory) return;

    const newExpanded = new Set(expandedPaths);
    
    if (expandedPaths.has(node.path)) {
      newExpanded.delete(node.path);
    } else {
      newExpanded.add(node.path);
      await loadChildren(node);
    }
    
    setExpandedPaths(newExpanded);
  };

  const handleNodeClick = (node: VirtualNode) => {
    setSelectedPath(node.path);
    onFileSelect?.(node);
  };

  const handleNodeDoubleClick = (node: VirtualNode) => {
    if (node.is_directory) {
      toggleExpanded(node);
    } else {
      onFileAction?.('open', node);
    }
  };

  const handleContextMenu = (e: React.MouseEvent, node: VirtualNode) => {
    e.preventDefault();
    e.stopPropagation();
    
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      node
    });
  };

  const handleContextMenuAction = async (action: string, node: VirtualNode) => {
    setContextMenu(null);
    
    try {
      switch (action) {
        case 'delete':
          await invoke('delete_virtual_file', {
            profileName,
            virtualPath: node.path
          });
          break;
        case 'copy_to_workspace':
          await invoke('copy_to_workspace', {
            profileName,
            virtualPath: node.path
          });
          break;
        case 'restore':
          await invoke('restore_deleted_file', {
            profileName,
            virtualPath: node.path
          });
          break;
      }
      
      // Reload tree after action
      await loadVirtualTree();
      onFileAction?.(action, node);
    } catch (err) {
      console.error(`Failed to ${action}:`, err);
      setError(err as string);
    }
  };

  const getSourceIcon = (source: VirtualNode['source']) => {
    switch (source) {
      case 'Base': return '📀'; // Base game file
      case 'Workspace': return '📝'; // New workspace file
      case 'WorkspaceOverride': return '🔄'; // Workspace overrides base
      case 'Tombstone': return '👻'; // Deleted base file
      default: return '📄';
    }
  };

  const getSourceTooltip = (source: VirtualNode['source']) => {
    switch (source) {
      case 'Base': return 'Base game file (read-only)';
      case 'Workspace': return 'Workspace file (editable)';
      case 'WorkspaceOverride': return 'Workspace file overriding base';
      case 'Tombstone': return 'Deleted base file';
      default: return 'Unknown source';
    }
  };

  const formatFileSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  };

  const renderNode = (node: VirtualNode, depth: number = 0) => {
    const isExpanded = expandedPaths.has(node.path);
    const isSelected = selectedPath === node.path;
    
    return (
      <div key={node.path} className="virtual-node">
        <div
          className={`node-item ${isSelected ? 'selected' : ''} ${!node.writable ? 'readonly' : ''}`}
          style={{ paddingLeft: `${depth * 20 + 8}px` }}
          onClick={() => handleNodeClick(node)}
          onDoubleClick={() => handleNodeDoubleClick(node)}
          onContextMenu={(e) => handleContextMenu(e, node)}
        >
          <div className="node-content">
            {node.is_directory && (
              <button 
                className="expand-button"
                onClick={(e) => {
                  e.stopPropagation();
                  toggleExpanded(node);
                }}
              >
                {isExpanded ? '▼' : '▶'}
              </button>
            )}
            
            <span className="node-icon">
              {node.is_directory ? '📁' : '📄'}
            </span>
            
            <span className="node-name">{node.name}</span>
            
            <span className="node-source" title={getSourceTooltip(node.source)}>
              {getSourceIcon(node.source)}
            </span>
            
            {!node.is_directory && node.size !== undefined && (
              <span className="node-size">{formatFileSize(node.size)}</span>
            )}
          </div>
        </div>
        
        {node.is_directory && isExpanded && node.children && (
          <div className="node-children">
            {node.children.map(child => renderNode(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  if (isLoading) {
    return (
      <div className="virtual-file-tree loading">
        <div className="loading-spinner"></div>
        <p>Loading virtual file tree...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="virtual-file-tree error">
        <h3>Error Loading File Tree</h3>
        <p>{error}</p>
        <button onClick={loadVirtualTree} className="retry-button">
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="virtual-file-tree">
      <div className="tree-header">
        <h3>Game Files</h3>
        <div className="legend">
          <span className="legend-item">
            📀 <small>Base</small>
          </span>
          <span className="legend-item">
            📝 <small>Workspace</small>
          </span>
          <span className="legend-item">
            🔄 <small>Override</small>
          </span>
        </div>
      </div>
      
      <div className="tree-content">
        {rootNode && renderNode(rootNode)}
      </div>
      
      {contextMenu && (
        <div
          className="context-menu"
          style={{
            position: 'fixed',
            left: contextMenu.x,
            top: contextMenu.y,
            zIndex: 1000
          }}
        >
          {contextMenu.node.source === 'Base' && !contextMenu.node.is_directory && (
            <button onClick={() => handleContextMenuAction('copy_to_workspace', contextMenu.node)}>
              📝 Make Editable
            </button>
          )}
          
          {contextMenu.node.writable && (
            <button 
              onClick={() => handleContextMenuAction('delete', contextMenu.node)}
              className="delete-action"
            >
              🗑️ Delete
            </button>
          )}
          
          {contextMenu.node.source === 'Tombstone' && (
            <button onClick={() => handleContextMenuAction('restore', contextMenu.node)}>
              ↶ Restore
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export default VirtualFileTree;