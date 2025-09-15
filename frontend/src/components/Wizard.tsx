import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import StepSelectBase from './StepSelectBase';
import StepValidateDrive from './StepValidateDrive';
import StepCreateStructure from './StepCreateStructure';
import './Wizard.css';

export interface Settings {
  schema: number;
  base_path: string;
  data_root: string;
  overlay_mode: string;
  wizard: {
    completed: boolean;
    completed_at?: string;
    wizard_version?: string;
  };
  preferences: {
    show_debug_info: boolean;
    log_retention_days: number;
    auto_check_updates: boolean;
    max_runtime_builds: number;
    show_progress: boolean;
  };
}

export interface DriveInfo {
  drive_letter?: string;
  is_ntfs: boolean;
  free_space_bytes: number;
  free_space_formatted: string;
  is_valid: boolean;
  error_message?: string;
}

export interface PathValidation {
  is_valid: boolean;
  exists: boolean;
  is_directory: boolean;
  has_gta_exe: boolean;
  error_message?: string;
}

const Wizard: React.FC = () => {
  const [currentStep, setCurrentStep] = useState(1);
  const [basePath, setBasePath] = useState('');
  const [dataRoot, setDataRoot] = useState('');
  const [isProcessing, setIsProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const steps = [
    { number: 1, title: 'Select Game Base', description: 'Choose your clean game installation' },
    { number: 2, title: 'Validate Drive', description: 'Check NTFS and free space' },
    { number: 3, title: 'Create Structure', description: 'Set up runtime directories' },
  ];

  // Generate data root path based on base path
  useEffect(() => {
    if (basePath) {
      try {
        // Extract drive letter from base path
        const match = basePath.match(/^([A-Za-z]):/);
        if (match) {
          const drive = match[1].toUpperCase();
          setDataRoot(`${drive}:\\DeltaRuntime`);
        }
      } catch (err) {
        console.error('Error generating data root:', err);
      }
    }
  }, [basePath]);

  const handleNext = () => {
    if (currentStep < steps.length) {
      setCurrentStep(currentStep + 1);
    }
  };

  const handlePrevious = () => {
    if (currentStep > 1) {
      setCurrentStep(currentStep - 1);
    }
  };

  const handleComplete = async () => {
    setIsProcessing(true);
    setError(null);

    try {
      await invoke('create_data_structure', {
        basePath,
        dataRoot,
      });

      // Wizard completed successfully
      window.location.reload(); // Reload to show main app
    } catch (err) {
      setError(err as string);
    } finally {
      setIsProcessing(false);
    }
  };

  const renderStep = () => {
    switch (currentStep) {
      case 1:
        return (
          <StepSelectBase
            basePath={basePath}
            onBasePathChange={setBasePath}
            onNext={handleNext}
          />
        );
      case 2:
        return (
          <StepValidateDrive
            basePath={basePath}
            dataRoot={dataRoot}
            onDataRootChange={setDataRoot}
            onNext={handleNext}
            onPrevious={handlePrevious}
          />
        );
      case 3:
        return (
          <StepCreateStructure
            basePath={basePath}
            dataRoot={dataRoot}
            onComplete={handleComplete}
            onPrevious={handlePrevious}
            isProcessing={isProcessing}
          />
        );
      default:
        return null;
    }
  };

  return (
    <div className="wizard">
      <div className="wizard-header">
        <h1>DeltaRuntime Setup</h1>
        <p>Welcome! Let's set up your runtime environment in just a few steps.</p>
      </div>

      <div className="wizard-progress">
        {steps.map((step) => (
          <div
            key={step.number}
            className={`progress-step ${
              step.number === currentStep
                ? 'active'
                : step.number < currentStep
                ? 'completed'
                : 'pending'
            }`}
          >
            <div className="step-number">{step.number}</div>
            <div className="step-info">
              <div className="step-title">{step.title}</div>
              <div className="step-description">{step.description}</div>
            </div>
          </div>
        ))}
      </div>

      <div className="wizard-content">
        {error && (
          <div className="error-message">
            <strong>Error:</strong> {error}
          </div>
        )}
        
        {renderStep()}
      </div>

      <div className="wizard-footer">
        <div className="debug-info">
          <p><strong>Base Path:</strong> {basePath || 'Not selected'}</p>
          <p><strong>Data Root:</strong> {dataRoot || 'Not determined'}</p>
        </div>
      </div>
    </div>
  );
};

export default Wizard;