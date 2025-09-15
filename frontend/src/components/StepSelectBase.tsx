import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { PathValidation } from './Wizard';

interface StepSelectBaseProps {
  basePath: string;
  onBasePathChange: (path: string) => void;
  onNext: () => void;
}

const StepSelectBase: React.FC<StepSelectBaseProps> = ({
  basePath,
  onBasePathChange,
  onNext,
}) => {
  const [validation, setValidation] = useState<PathValidation | null>(null);
  const [isValidating, setIsValidating] = useState(false);

  const handleBrowse = async () => {
    try {
      const selected = await invoke<string | null>('pick_directory', {
        title: 'Select your clean GTA:SA installation folder',
      });

      if (selected) {
        onBasePathChange(selected);
        validatePath(selected);
      }
    } catch (err) {
      console.error('Error opening folder dialog:', err);
      // For now, just show the error message which tells user to input manually
      alert(err as string);
    }
  };

  const validatePath = async (path: string) => {
    if (!path) {
      setValidation(null);
      return;
    }

    setIsValidating(true);
    try {
      const result = await invoke<PathValidation>('validate_gta_base_path', { path });
      setValidation(result);
    } catch (err) {
      setValidation({
        is_valid: false,
        exists: false,
        is_directory: false,
        has_gta_exe: false,
        error_message: err as string,
      });
    } finally {
      setIsValidating(false);
    }
  };

  const handlePathInput = (value: string) => {
    onBasePathChange(value);
    if (value) {
      validatePath(value);
    } else {
      setValidation(null);
    }
  };

  const canProceed = validation?.is_valid === true;

  return (
    <div className="wizard-step">
      <h2>Step 1: Select GTA:SA Base Installation</h2>
      <p>
        Choose your clean, unmodded GTA:SA installation folder. This should contain
        <code>gta_sa.exe</code> and will serve as the pristine base for all mod profiles.
      </p>

      <div className="form-group">
        <label htmlFor="base-path">GTA:SA Installation Path:</label>
        <div className="path-input-group">
          <input
            id="base-path"
            type="text"
            value={basePath}
            onChange={(e) => handlePathInput(e.target.value)}
            placeholder="C:\Program Files (x86)\Rockstar Games\GTA San Andreas"
            className={`path-input ${
              validation
                ? validation.is_valid
                  ? 'valid'
                  : 'invalid'
                : ''
            }`}
          />
          <button type="button" onClick={handleBrowse} className="browse-button">
            Browse...
          </button>
        </div>
      </div>

      {isValidating && (
        <div className="validation-status validating">
          <div className="spinner"></div>
          Validating path...
        </div>
      )}

      {validation && (
        <div className={`validation-status ${validation.is_valid ? 'valid' : 'invalid'}`}>
          {validation.is_valid ? (
            <div className="validation-success">
              <span className="checkmark">✓</span>
              Valid GTA:SA installation found!
              <ul>
                <li>Directory exists and is accessible</li>
                <li>gta_sa.exe found</li>
                <li>Ready to use as base installation</li>
              </ul>
            </div>
          ) : (
            <div className="validation-error">
              <span className="error-mark">✗</span>
              Invalid path:
              <ul>
                {!validation.exists && <li>Path does not exist</li>}
                {validation.exists && !validation.is_directory && <li>Path is not a directory</li>}
                {validation.exists && validation.is_directory && !validation.has_gta_exe && (
                  <li>gta_sa.exe not found in this directory</li>
                )}
                {validation.error_message && <li>{validation.error_message}</li>}
              </ul>
            </div>
          )}
        </div>
      )}

      <div className="step-actions">
        <button
          type="button"
          onClick={onNext}
          disabled={!canProceed}
          className="next-button"
        >
          Next: Validate Drive →
        </button>
      </div>

      <div className="step-help">
        <h3>What we're looking for:</h3>
        <ul>
          <li><strong>Clean installation:</strong> Unmodded GTA:SA with original files</li>
          <li><strong>Contains gta_sa.exe:</strong> The main game executable</li>
          <li><strong>Complete game files:</strong> All original data files and folders</li>
        </ul>
        <p>
          <strong>Note:</strong> This folder will not be modified. The launcher creates
          separate copies for each mod profile while preserving your original installation.
        </p>
      </div>
    </div>
  );
};

export default StepSelectBase;