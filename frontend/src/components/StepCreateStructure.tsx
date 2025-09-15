import React, { useState } from 'react';

interface StepCreateStructureProps {
  basePath: string;
  dataRoot: string;
  onComplete: () => void;
  onPrevious: () => void;
  isProcessing: boolean;
}

const StepCreateStructure: React.FC<StepCreateStructureProps> = ({
  basePath,
  dataRoot,
  onComplete,
  onPrevious,
  isProcessing,
}) => {
  const [showAdvanced, setShowAdvanced] = useState(false);

  const structurePreview = [
    { path: `${dataRoot}\\`, description: 'Main data root directory' },
    { path: `${dataRoot}\\base\\`, description: 'Pristine GTA:SA copy (hardlinked)' },
    { path: `${dataRoot}\\profiles\\`, description: 'Individual mod profiles' },
    { path: `${dataRoot}\\saves\\`, description: 'Isolated save games per profile' },
    { path: `${dataRoot}\\logs\\`, description: 'Launcher and game logs' },
    { path: `${dataRoot}\\temp\\`, description: 'Temporary files and builds' },
  ];

  return (
    <div className="wizard-step">
      <h2>Step 3: Create Launcher Structure</h2>
      <p>
        Ready to create the mod launcher directory structure. This will set up
        all the necessary folders and create an initial hardlinked copy of your
        GTA:SA installation.
      </p>

      <div className="structure-preview">
        <h3>Directory Structure to Create:</h3>
        <div className="file-tree">
          {structurePreview.map((item, index) => (
            <div key={index} className="tree-item">
              <code className="path">{item.path}</code>
              <span className="description">{item.description}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="setup-summary">
        <h3>Setup Summary:</h3>
        <div className="summary-item">
          <strong>Source:</strong> <code>{basePath}</code>
        </div>
        <div className="summary-item">
          <strong>Destination:</strong> <code>{dataRoot}</code>
        </div>
        <div className="summary-item">
          <strong>Method:</strong> NTFS hardlinks (space-efficient file copying)
        </div>
      </div>

      {showAdvanced && (
        <div className="advanced-info">
          <h3>What happens during setup:</h3>
          <ol>
            <li>Create main directory structure</li>
            <li>Copy GTA:SA files to base directory using hardlinks</li>
            <li>Set up profile and save isolation directories</li>
            <li>Initialize launcher configuration</li>
            <li>Create desktop shortcuts (optional)</li>
          </ol>
          <p>
            <strong>Hardlinks explained:</strong> Files are not duplicated but rather
            linked at the file system level. This means multiple copies appear to exist
            but only one set of data is stored on disk, saving significant space.
          </p>
        </div>
      )}

      <div className="toggle-advanced">
        <button
          type="button"
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="link-button"
        >
          {showAdvanced ? 'Hide' : 'Show'} technical details
        </button>
      </div>

      {isProcessing && (
        <div className="processing-status">
          <div className="spinner"></div>
          <div className="processing-text">
            <strong>Creating launcher structure...</strong>
            <p>This may take a few minutes depending on your GTA:SA installation size.</p>
          </div>
        </div>
      )}

      <div className="step-actions">
        <button
          type="button"
          onClick={onPrevious}
          disabled={isProcessing}
          className="previous-button"
        >
          ← Previous: Validate Drive
        </button>
        <button
          type="button"
          onClick={onComplete}
          disabled={isProcessing}
          className="complete-button"
        >
          {isProcessing ? 'Creating...' : 'Complete Setup'}
        </button>
      </div>

      <div className="step-help">
        <h3>Before you proceed:</h3>
        <ul>
          <li>Ensure you have sufficient disk space</li>
          <li>Close any running GTA:SA instances</li>
          <li>Temporarily disable antivirus if it interferes with file operations</li>
        </ul>
        <p>
          <strong>Note:</strong> This process is safe and reversible. The original
          GTA:SA installation remains untouched.
        </p>
      </div>
    </div>
  );
};

export default StepCreateStructure;