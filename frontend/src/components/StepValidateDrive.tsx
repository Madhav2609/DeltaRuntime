import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { DriveInfo } from './Wizard';

interface StepValidateDriveProps {
  basePath: string;
  dataRoot: string;
  onDataRootChange: (path: string) => void;
  onNext: () => void;
  onPrevious: () => void;
}

const StepValidateDrive: React.FC<StepValidateDriveProps> = ({
  dataRoot,
  onDataRootChange,
  onNext,
  onPrevious,
}) => {
  const [driveInfo, setDriveInfo] = useState<DriveInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [customPath, setCustomPath] = useState(false);

  useEffect(() => {
    if (dataRoot) {
      checkDrive(dataRoot);
    }
  }, [dataRoot]);

  const checkDrive = async (path: string) => {
    if (!path) return;

    setIsChecking(true);
    try {
      const info = await invoke<DriveInfo>('get_drive_info', { path });
      setDriveInfo(info);
    } catch (err) {
      setDriveInfo({
        is_ntfs: false,
        free_space_bytes: 0,
        free_space_formatted: 'Unknown',
        is_valid: false,
        error_message: err as string,
      });
    } finally {
      setIsChecking(false);
    }
  };

  const handleCustomPath = () => {
    setCustomPath(true);
  };

  const handlePathChange = (value: string) => {
    onDataRootChange(value);
    if (value) {
      checkDrive(value);
    } else {
      setDriveInfo(null);
    }
  };

  const canProceed = driveInfo?.is_valid === true;
  const needsMoreSpace = driveInfo && driveInfo.free_space_bytes < 5 * 1024 * 1024 * 1024; // 5GB

  return (
    <div className="wizard-step">
      <h2>Step 2: Validate Drive & Storage</h2>
      <p>
        We'll verify that your chosen drive supports NTFS features and has enough space
        for mod profiles and save isolation.
      </p>

      <div className="form-group">
        <label htmlFor="data-root">Mod Launcher Data Root:</label>
        {!customPath ? (
          <div className="auto-path-display">
            <input
              id="data-root"
              type="text"
              value={dataRoot}
              readOnly
              className="path-input readonly"
            />
            <button type="button" onClick={handleCustomPath} className="change-button">
              Change...
            </button>
          </div>
        ) : (
          <input
            id="data-root"
            type="text"
            value={dataRoot}
            onChange={(e) => handlePathChange(e.target.value)}
            className={`path-input ${
              driveInfo
                ? driveInfo.is_valid
                  ? 'valid'
                  : 'invalid'
                : ''
            }`}
            placeholder="C:\ModLauncher"
          />
        )}
      </div>

      {isChecking && (
        <div className="validation-status validating">
          <div className="spinner"></div>
          Checking drive compatibility...
        </div>
      )}

      {driveInfo && (
        <div className={`validation-status ${driveInfo.is_valid ? 'valid' : 'invalid'}`}>
          {driveInfo.is_valid ? (
            <div className="validation-success">
              <span className="checkmark">✓</span>
              Drive validation successful!
              <div className="drive-details">
                <div className="detail-row">
                  <span className="label">Drive:</span>
                  <span className="value">{driveInfo.drive_letter || 'N/A'}</span>
                </div>
                <div className="detail-row">
                  <span className="label">File System:</span>
                  <span className="value">NTFS ✓</span>
                </div>
                <div className="detail-row">
                  <span className="label">Free Space:</span>
                  <span className={`value ${needsMoreSpace ? 'warning' : ''}`}>
                    {driveInfo.free_space_formatted}
                    {needsMoreSpace && ' (Low space warning)'}
                  </span>
                </div>
              </div>
              {needsMoreSpace && (
                <div className="space-warning">
                  <strong>Note:</strong> You have less than 5GB free space. Consider
                  freeing up space or choosing a different drive for optimal performance.
                </div>
              )}
            </div>
          ) : (
            <div className="validation-error">
              <span className="error-mark">✗</span>
              Drive validation failed:
              <ul>
                {!driveInfo.is_ntfs && (
                  <li>Drive is not NTFS - hardlinks and junctions require NTFS</li>
                )}
                {driveInfo.free_space_bytes < 1024 * 1024 * 1024 && (
                  <li>Insufficient free space (need at least 1GB)</li>
                )}
                {driveInfo.error_message && <li>{driveInfo.error_message}</li>}
              </ul>
            </div>
          )}
        </div>
      )}

      <div className="step-actions">
        <button type="button" onClick={onPrevious} className="previous-button">
          ← Previous: Select Base
        </button>
        <button
          type="button"
          onClick={onNext}
          disabled={!canProceed}
          className="next-button"
        >
          Next: Create Structure →
        </button>
      </div>

      <div className="step-help">
        <h3>Requirements:</h3>
        <ul>
          <li><strong>NTFS File System:</strong> Required for hardlinks and junctions</li>
          <li><strong>Free Space:</strong> At least 1GB, recommended 5GB+</li>
          <li><strong>Write Access:</strong> Permission to create directories and files</li>
        </ul>
        <p>
          The mod launcher uses advanced NTFS features to efficiently manage multiple
          mod profiles without duplicating game files.
        </p>
      </div>
    </div>
  );
};

export default StepValidateDrive;