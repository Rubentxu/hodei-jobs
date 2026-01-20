//!
//! # State Equivalence Verifier
//!
//! Verifies that legacy saga state and new saga-engine state are equivalent
//! for validation during migration.
//!

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;

use super::serializer::{LegacySagaStateSerializer, SagaEngineSagaState, SerializerError};

/// Result of state equivalence verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub is_equivalent: bool,
    pub discrepancies: Vec<Discrepancy>,
    pub confidence: f64,
    pub verified_at: chrono::DateTime<chrono::Utc>,
}

impl VerificationResult {
    pub fn success() -> Self {
        Self {
            is_equivalent: true,
            discrepancies: vec![],
            confidence: 1.0,
            verified_at: chrono::Utc::now(),
        }
    }

    pub fn with_discrepancies(discrepancies: Vec<Discrepancy>, confidence: f64) -> Self {
        Self {
            is_equivalent: false,
            discrepancies,
            confidence,
            verified_at: chrono::Utc::now(),
        }
    }
}

/// A discrepancy found between legacy and new state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discrepancy {
    pub field: String,
    pub legacy_value: serde_json::Value,
    pub new_value: serde_json::Value,
    pub severity: DiscrepancySeverity,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscrepancySeverity {
    Critical, // Data loss or corruption
    Warning,  // Potential data inconsistency
    Info,     // Minor difference (format, etc.)
}

/// Verifier for state equivalence
#[derive(Debug)]
pub struct StateEquivalenceVerifier {
    serializer: LegacySagaStateSerializer,
    strict_mode: bool,
}

impl Default for StateEquivalenceVerifier {
    fn default() -> Self {
        Self::new(false)
    }
}

impl StateEquivalenceVerifier {
    /// Creates a new verifier
    pub fn new(strict_mode: bool) -> Self {
        Self {
            serializer: LegacySagaStateSerializer::new(),
            strict_mode,
        }
    }

    /// Verifies that legacy state converted to new format is equivalent to expected new state
    pub fn verify(
        &self,
        legacy_state: &[u8],
        expected_new_state: &SagaEngineSagaState,
    ) -> Result<VerificationResult, SerializerError> {
        let converted_state = self.serializer.deserialize(legacy_state, 4)?;

        let mut discrepancies = Vec::new();

        // Check saga_id
        if converted_state.saga_id.0 != expected_new_state.saga_id.0 {
            discrepancies.push(Discrepancy {
                field: "saga_id".to_string(),
                legacy_value: serde_json::json!(converted_state.saga_id.0),
                new_value: serde_json::json!(expected_new_state.saga_id.0),
                severity: DiscrepancySeverity::Critical,
                description: "Saga ID mismatch".to_string(),
            });
        }

        // Check saga_type
        if converted_state.saga_type != expected_new_state.saga_type {
            discrepancies.push(Discrepancy {
                field: "saga_type".to_string(),
                legacy_value: serde_json::json!(converted_state.saga_type),
                new_value: serde_json::json!(expected_new_state.saga_type),
                severity: DiscrepancySeverity::Critical,
                description: "Saga type mismatch".to_string(),
            });
        }

        // Check status
        if self.strict_mode && converted_state.status != expected_new_state.status {
            discrepancies.push(Discrepancy {
                field: "status".to_string(),
                legacy_value: serde_json::json!(format!("{:?}", converted_state.status)),
                new_value: serde_json::json!(format!("{:?}", expected_new_state.status)),
                severity: DiscrepancySeverity::Warning,
                description: "Status value mismatch".to_string(),
            });
        }

        // Check context data
        if converted_state.context.data != expected_new_state.context.data {
            discrepancies.push(Discrepancy {
                field: "context.data".to_string(),
                legacy_value: serde_json::json!(converted_state.context.data),
                new_value: serde_json::json!(expected_new_state.context.data),
                severity: DiscrepancySeverity::Warning,
                description: "Context data mismatch".to_string(),
            });
        }

        // Check steps count
        if converted_state.steps.len() != expected_new_state.steps.len() {
            discrepancies.push(Discrepancy {
                field: "steps".to_string(),
                legacy_value: serde_json::json!(converted_state.steps.len()),
                new_value: serde_json::json!(expected_new_state.steps.len()),
                severity: DiscrepancySeverity::Warning,
                description: "Steps count mismatch".to_string(),
            });
        }

        // Calculate confidence
        let confidence = if discrepancies.is_empty() {
            1.0
        } else {
            let critical_count = discrepancies
                .iter()
                .filter(|d| matches!(d.severity, DiscrepancySeverity::Critical))
                .count();
            if critical_count > 0 { 0.0 } else { 0.5 }
        };

        Ok(if discrepancies.is_empty() {
            VerificationResult::success()
        } else {
            VerificationResult::with_discrepancies(discrepancies, confidence)
        })
    }

    /// Verifies that round-trip conversion maintains data integrity
    pub fn verify_round_trip(
        &self,
        original_legacy: &[u8],
    ) -> Result<VerificationResult, SerializerError> {
        let converted = self.serializer.deserialize(original_legacy, 4)?;
        let back_to_legacy = self.serializer.serialize(&converted, 3)?;

        let original: super::serializer::LegacySagaState = serde_json::from_slice(original_legacy)
            .map_err(|e| SerializerError::DeserializationFailed(e.to_string()))?;
        let round_tripped: super::serializer::LegacySagaState =
            serde_json::from_slice(&back_to_legacy)
                .map_err(|e| SerializerError::DeserializationFailed(e.to_string()))?;

        let mut discrepancies = Vec::new();

        if original.saga_id != round_tripped.saga_id {
            discrepancies.push(Discrepancy {
                field: "saga_id".to_string(),
                legacy_value: serde_json::json!(original.saga_id),
                new_value: serde_json::json!(round_tripped.saga_id),
                severity: DiscrepancySeverity::Critical,
                description: "Saga ID changed during round-trip".to_string(),
            });
        }

        if original.saga_type != round_tripped.saga_type {
            discrepancies.push(Discrepancy {
                field: "saga_type".to_string(),
                legacy_value: serde_json::json!(original.saga_type),
                new_value: serde_json::json!(round_tripped.saga_type),
                severity: DiscrepancySeverity::Critical,
                description: "Saga type changed during round-trip".to_string(),
            });
        }

        let is_empty = discrepancies.is_empty();
        let confidence = if is_empty { 1.0 } else { 0.0 };

        Ok(VerificationResult {
            is_equivalent: is_empty,
            discrepancies,
            confidence,
            verified_at: chrono::Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::serializer::SagaId;
    use super::super::serializer::{
        LegacySagaState, SagaEngineContext, SagaEngineSagaState, SagaEngineStatus,
    };
    use super::*;

    fn create_test_legacy_state() -> LegacySagaState {
        LegacySagaState {
            saga_id: "test-saga-123".to_string(),
            saga_type: "test-type".to_string(),
            status: super::super::serializer::LegacySagaStatus::Running,
            current_step: "step-1".to_string(),
            context: super::super::serializer::LegacySagaContext {
                data: std::collections::HashMap::new(),
            },
            compensation_stack: vec![],
            history: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 2,
        }
    }

    #[test]
    fn test_verify_equivalent_states() {
        let verifier = StateEquivalenceVerifier::new(false);
        let legacy = create_test_legacy_state();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();

        let expected = SagaEngineSagaState {
            saga_id: SagaId("test-saga-123".to_string()),
            saga_type: "test-type".to_string(),
            status: SagaEngineStatus::Executing,
            current_step_id: "step-1".to_string(),
            context: SagaEngineContext {
                data: std::collections::HashMap::new(),
            },
            steps: vec![],
            history: vec![],
            created_at: legacy.created_at,
            updated_at: legacy.updated_at,
            version: 4,
        };

        let result = verifier.verify(&legacy_json, &expected);

        assert!(result.is_ok());
        assert!(result.unwrap().is_equivalent);
    }

    #[test]
    fn test_detect_saga_id_mismatch() {
        let verifier = StateEquivalenceVerifier::new(true);
        let legacy = create_test_legacy_state();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();

        let expected = SagaEngineSagaState {
            saga_id: SagaId("different-id".to_string()),
            saga_type: "test-type".to_string(),
            status: SagaEngineStatus::Executing,
            current_step_id: "step-1".to_string(),
            context: SagaEngineContext {
                data: std::collections::HashMap::new(),
            },
            steps: vec![],
            history: vec![],
            created_at: legacy.created_at,
            updated_at: legacy.updated_at,
            version: 4,
        };

        let result = verifier.verify(&legacy_json, &expected);

        assert!(result.is_ok());
        let verification = result.unwrap();
        assert!(!verification.is_equivalent);
        assert!(
            verification
                .discrepancies
                .iter()
                .any(|d| d.field == "saga_id")
        );
    }

    #[test]
    fn test_round_trip_verification() {
        let verifier = StateEquivalenceVerifier::new(false);
        let legacy = create_test_legacy_state();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();

        let result = verifier.verify_round_trip(&legacy_json);

        assert!(result.is_ok());
        assert!(result.unwrap().is_equivalent);
    }
}
