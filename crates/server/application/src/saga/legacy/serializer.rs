//!
//! # Legacy Saga State Serializer
//!
//! Provides serialization and deserialization capabilities for migrating saga state
//! from the legacy format to the new saga-engine v4.0 format.
//!

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;

/// Result type for serialization operations
pub type SerializationResult<T> = Result<T, SerializerError>;

/// Errors that can occur during state serialization
#[derive(Debug, Error, Clone)]
pub enum SerializerError {
    #[error("Invalid legacy format: {0}")]
    InvalidFormat(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u32, found: u32 },
    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
}

/// Legacy saga state format (v1-v3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacySagaState {
    pub saga_id: String,
    pub saga_type: String,
    pub status: LegacySagaStatus,
    pub current_step: String,
    pub context: LegacySagaContext,
    pub compensation_stack: Vec<LegacyCompensationEntry>,
    pub history: Vec<LegacyHistoryEntry>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LegacySagaStatus {
    Pending,
    Running,
    Compensating,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacySagaContext {
    pub data: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyCompensationEntry {
    pub step_id: String,
    pub compensation_data: serde_json::Value,
    pub executed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyHistoryEntry {
    pub step_id: String,
    pub step_name: String,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
    pub status: String,
    pub executed_at: chrono::DateTime<chrono::Utc>,
    pub error: Option<String>,
}

/// New saga-engine format types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaEngineSagaState {
    pub saga_id: SagaId,
    pub saga_type: String,
    pub status: SagaEngineStatus,
    pub current_step_id: String,
    pub context: SagaEngineContext,
    pub steps: Vec<SagaEngineStep>,
    pub history: Vec<SagaEngineHistoryEntry>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaEngineStatus {
    Pending,
    Executing,
    Compensating,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaEngineContext {
    pub data: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaEngineStep {
    pub step_id: String,
    pub step_type: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub status: SagaEngineStepStatus,
    pub compensation: Option<serde_json::Value>,
    pub executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SagaEngineStepStatus {
    Pending,
    Executing,
    Completed,
    Failed,
    Compensated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaEngineHistoryEntry {
    pub step_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Main serializer for migrating saga state
#[derive(Debug, Default)]
pub struct LegacySagaStateSerializer;

impl LegacySagaStateSerializer {
    /// Creates a new serializer instance
    pub fn new() -> Self {
        Self
    }

    /// Deserializes legacy saga state to the new format
    ///
    /// # Arguments
    /// * `legacy_state` - The legacy saga state to convert
    /// * `target_version` - The target saga-engine version (default: 4)
    ///
    /// # Returns
    /// * `Ok(SagaEngineSagaState)` - Converted state
    /// * `Err(SerializerError)` - Conversion error
    pub fn deserialize(
        &self,
        legacy_state: &[u8],
        target_version: u32,
    ) -> SerializationResult<SagaEngineSagaState> {
        // Step 1: Parse legacy JSON
        let legacy: LegacySagaState = serde_json::from_slice(legacy_state)
            .map_err(|e| SerializerError::DeserializationFailed(e.to_string()))?;

        // Step 2: Validate version
        if legacy.version > target_version {
            return Err(SerializerError::VersionMismatch {
                expected: target_version,
                found: legacy.version,
            });
        }

        // Step 3: Convert status
        let status = match legacy.status {
            LegacySagaStatus::Pending => SagaEngineStatus::Pending,
            LegacySagaStatus::Running => SagaEngineStatus::Executing,
            LegacySagaStatus::Compensating => SagaEngineStatus::Compensating,
            LegacySagaStatus::Completed => SagaEngineStatus::Completed,
            LegacySagaStatus::Failed => SagaEngineStatus::Failed,
        };

        // Step 4: Convert context
        let context = SagaEngineContext {
            data: legacy.context.data,
        };

        // Step 5: Convert steps from history
        let mut steps = Vec::new();
        for (idx, entry) in legacy.history.iter().enumerate() {
            let compensation = legacy
                .compensation_stack
                .iter()
                .find(|c| c.step_id == entry.step_id)
                .map(|c| c.compensation_data.clone());

            let step_status = match entry.status.as_str() {
                "completed" => SagaEngineStepStatus::Completed,
                "failed" => SagaEngineStepStatus::Failed,
                _ => SagaEngineStepStatus::Pending,
            };

            steps.push(SagaEngineStep {
                step_id: entry.step_id.clone(),
                step_type: entry.step_name.clone(),
                input: entry.input.clone(),
                output: Some(entry.output.clone()),
                status: step_status,
                compensation,
                executed_at: Some(entry.executed_at),
            });
        }

        // Add pending steps from compensation stack that haven't been executed
        for comp in &legacy.compensation_stack {
            if !steps.iter().any(|s| s.step_id == comp.step_id) {
                steps.push(SagaEngineStep {
                    step_id: comp.step_id.clone(),
                    step_type: "compensation".to_string(),
                    input: serde_json::Value::Null,
                    output: None,
                    status: SagaEngineStepStatus::Pending,
                    compensation: Some(comp.compensation_data.clone()),
                    executed_at: comp.executed_at,
                });
            }
        }

        // Step 6: Convert history to events
        let history = legacy
            .history
            .iter()
            .map(|entry| SagaEngineHistoryEntry {
                step_id: entry.step_id.clone(),
                event_type: format!("step.{}", entry.status),
                data: serde_json::json!({
                    "step_name": entry.step_name,
                    "input": entry.input,
                    "output": entry.output,
                    "error": entry.error,
                }),
                timestamp: entry.executed_at,
            })
            .chain(
                legacy
                    .compensation_stack
                    .iter()
                    .map(|comp| SagaEngineHistoryEntry {
                        step_id: comp.step_id.clone(),
                        event_type: "step.compensated".to_string(),
                        data: serde_json::json!({
                            "status": comp.status,
                        }),
                        timestamp: comp.executed_at.unwrap_or_else(chrono::Utc::now),
                    }),
            )
            .collect();

        Ok(SagaEngineSagaState {
            saga_id: SagaId(legacy.saga_id),
            saga_type: legacy.saga_type,
            status,
            current_step_id: legacy.current_step,
            context,
            steps,
            history,
            created_at: legacy.created_at,
            updated_at: legacy.updated_at,
            version: target_version,
        })
    }

    /// Serializes new saga-engine state to legacy format (for rollback)
    ///
    /// # Arguments
    /// * `engine_state` - The saga-engine state to convert
    /// * `target_version` - The target legacy version (default: 3)
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - Serialized legacy state as JSON bytes
    /// * `Err(SerializerError)` - Serialization error
    pub fn serialize(
        &self,
        engine_state: &SagaEngineSagaState,
        target_version: u32,
    ) -> SerializationResult<Vec<u8>> {
        if target_version > 3 {
            return Err(SerializerError::VersionMismatch {
                expected: target_version,
                found: 3,
            });
        }

        let status = match engine_state.status {
            SagaEngineStatus::Pending => LegacySagaStatus::Pending,
            SagaEngineStatus::Executing => LegacySagaStatus::Running,
            SagaEngineStatus::Compensating => LegacySagaStatus::Compensating,
            SagaEngineStatus::Completed => LegacySagaStatus::Completed,
            SagaEngineStatus::Failed => LegacySagaStatus::Failed,
        };

        let history = engine_state
            .steps
            .iter()
            .filter_map(|step| {
                Some(LegacyHistoryEntry {
                    step_id: step.step_id.clone(),
                    step_name: step.step_type.clone(),
                    input: step.input.clone(),
                    output: step.output.clone().unwrap_or(serde_json::Value::Null),
                    status: match step.status {
                        SagaEngineStepStatus::Completed => "completed".to_string(),
                        SagaEngineStepStatus::Failed => "failed".to_string(),
                        _ => "pending".to_string(),
                    },
                    executed_at: step.executed_at.unwrap_or_else(chrono::Utc::now),
                    error: None,
                })
            })
            .collect();

        let compensation_stack = engine_state
            .steps
            .iter()
            .filter_map(|step| {
                step.compensation
                    .as_ref()
                    .map(|comp_data| LegacyCompensationEntry {
                        step_id: step.step_id.clone(),
                        compensation_data: comp_data.clone(),
                        executed_at: None,
                        status: "pending".to_string(),
                    })
            })
            .collect();

        let legacy = LegacySagaState {
            saga_id: engine_state.saga_id.0.clone(),
            saga_type: engine_state.saga_type.clone(),
            status,
            current_step: engine_state.current_step_id.clone(),
            context: LegacySagaContext {
                data: engine_state.context.data.clone(),
            },
            compensation_stack,
            history,
            created_at: engine_state.created_at,
            updated_at: engine_state.updated_at,
            version: target_version,
        };

        serde_json::to_vec(&legacy).map_err(|e| SerializerError::SerializationFailed(e.to_string()))
    }

    /// Validates that a legacy state can be migrated
    pub fn validate(&self, legacy_state: &[u8]) -> SerializationResult<()> {
        let legacy: LegacySagaState = serde_json::from_slice(legacy_state)
            .map_err(|e| SerializerError::InvalidFormat(e.to_string()))?;

        if legacy.saga_id.is_empty() {
            return Err(SerializerError::MissingField("saga_id".to_string()));
        }

        if legacy.saga_type.is_empty() {
            return Err(SerializerError::MissingField("saga_type".to_string()));
        }

        Ok(())
    }
}

/// Batch serializer for processing multiple saga states
#[derive(Debug)]
pub struct BatchSagaStateSerializer {
    serializer: LegacySagaStateSerializer,
    max_batch_size: usize,
}

impl BatchSagaStateSerializer {
    /// Creates a new batch serializer
    pub fn new(max_batch_size: usize) -> Self {
        Self {
            serializer: LegacySagaStateSerializer::new(),
            max_batch_size,
        }
    }

    /// Processes a batch of legacy saga states
    ///
    /// # Returns
    /// * `Ok((Vec<SagaEngineSagaState>, Vec<SerializerError>))` - Results and errors
    pub fn process_batch(
        &self,
        states: Vec<&[u8]>,
    ) -> (Vec<SagaEngineSagaState>, Vec<(usize, SerializerError)>) {
        let mut results = Vec::with_capacity(states.len().min(self.max_batch_size));
        let mut errors = Vec::new();

        for (idx, state) in states.into_iter().enumerate() {
            match self.serializer.deserialize(state, 4) {
                Ok(converted) => results.push(converted),
                Err(e) => errors.push((idx, e)),
            }
        }

        (results, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_legacy_saga_state() -> LegacySagaState {
        LegacySagaState {
            saga_id: "saga-123".to_string(),
            saga_type: "job-execution".to_string(),
            status: LegacySagaStatus::Running,
            current_step: "provision-worker".to_string(),
            context: LegacySagaContext {
                data: std::collections::HashMap::new(),
            },
            compensation_stack: vec![LegacyCompensationEntry {
                step_id: "step-1".to_string(),
                compensation_data: serde_json::json!({"action": "destroy"}),
                executed_at: None,
                status: "pending".to_string(),
            }],
            history: vec![LegacyHistoryEntry {
                step_id: "step-0".to_string(),
                step_name: "create-job".to_string(),
                input: serde_json::json!({"job_id": "job-456"}),
                output: serde_json::json!({"created": true}),
                status: "completed".to_string(),
                executed_at: chrono::Utc::now(),
                error: None,
            }],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 2,
        }
    }

    #[test]
    fn test_deserialize_legacy_state() {
        let serializer = LegacySagaStateSerializer::new();
        let legacy = create_legacy_saga_state();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();

        let result = serializer.deserialize(&legacy_json, 4);

        assert!(result.is_ok());
        let engine_state = result.unwrap();
        assert_eq!(engine_state.saga_id.0, "saga-123");
        assert_eq!(engine_state.saga_type, "job-execution");
        assert!(matches!(engine_state.status, SagaEngineStatus::Executing));
    }

    #[test]
    fn test_serialize_to_legacy() {
        let serializer = LegacySagaStateSerializer::new();

        let engine_state = SagaEngineSagaState {
            saga_id: SagaId("saga-456".to_string()),
            saga_type: "test-saga".to_string(),
            status: SagaEngineStatus::Completed,
            current_step_id: "final-step".to_string(),
            context: SagaEngineContext {
                data: std::collections::HashMap::new(),
            },
            steps: vec![],
            history: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 4,
        };

        let result = serializer.serialize(&engine_state, 3);

        assert!(result.is_ok());
        let legacy_json = result.unwrap();
        let legacy: LegacySagaState = serde_json::from_slice(&legacy_json).unwrap();
        assert_eq!(legacy.saga_id, "saga-456");
        assert!(matches!(legacy.status, LegacySagaStatus::Completed));
    }

    #[test]
    fn test_validate_legacy_state() {
        let serializer = LegacySagaStateSerializer::new();
        let legacy = create_legacy_saga_state();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();

        let result = serializer.validate(&legacy_json);

        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_processing() {
        let batch_serializer = BatchSagaStateSerializer::new(100);
        let legacy = create_legacy_saga_state();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();

        let states: Vec<&[u8]> = vec![&legacy_json, &legacy_json, &legacy_json];
        let (results, errors) = batch_serializer.process_batch(states);

        assert_eq!(results.len(), 3);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_version_mismatch() {
        let serializer = LegacySagaStateSerializer::new();
        let legacy = create_legacy_saga_state();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();

        // Try to deserialize with lower version than current
        let result = serializer.deserialize(&legacy_json, 1);

        assert!(result.is_err());
        if let Err(SerializerError::VersionMismatch { expected, found }) = result {
            assert_eq!(expected, 1);
            assert_eq!(found, 2);
        }
    }
}
