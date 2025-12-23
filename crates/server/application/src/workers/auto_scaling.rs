use hodei_server_domain::workers::auto_scaling::{
    AutoScalingStrategy, ScalingContext, ScalingDecision,
};
use std::sync::Arc;

/// Default auto-scaling strategy based on simple thresholds.
pub struct SimpleThresholdStrategy {
    min_workers: u32,
    max_workers: u32,
    jobs_per_worker: u32,
}

impl SimpleThresholdStrategy {
    pub fn new(min_workers: u32, max_workers: u32, jobs_per_worker: u32) -> Self {
        Self {
            min_workers,
            max_workers,
            jobs_per_worker,
        }
    }
}

impl Default for SimpleThresholdStrategy {
    fn default() -> Self {
        Self {
            min_workers: 1,
            max_workers: 10,
            jobs_per_worker: 1,
        }
    }
}

impl AutoScalingStrategy for SimpleThresholdStrategy {
    fn name(&self) -> &str {
        "SimpleThresholdStrategy"
    }

    fn evaluate(&self, context: &ScalingContext) -> ScalingDecision {
        let total_active = context.total_workers();
        let available = context.available_workers();

        // 1. Ensure minimum workers
        if total_active < self.min_workers {
            return ScalingDecision::ScaleUp {
                provider_id: context.provider_id.clone(),
                count: self.min_workers - total_active,
                reason: format!("Scaling up to meet minimum workers ({})", self.min_workers),
            };
        }

        // 2. Scale up if pending jobs exceed available capacity
        let needed_capacity = context.pending_jobs;
        if needed_capacity > available && total_active < self.max_workers {
            let potential_new =
                (needed_capacity - available + self.jobs_per_worker - 1) / self.jobs_per_worker;
            let actual_new = std::cmp::min(potential_new, self.max_workers - total_active);

            if actual_new > 0 {
                return ScalingDecision::ScaleUp {
                    provider_id: context.provider_id.clone(),
                    count: actual_new,
                    reason: format!(
                        "Pending jobs ({}) exceed available workers ({}), scaling up",
                        context.pending_jobs, available
                    ),
                };
            }
        }

        // 3. Scale down if we have too many idle healthy workers and are above minimum
        // (Simplified logic: scale down one by one if idle for too long handled by lifecycle,
        // but here we can make proactive decisions if needed)

        ScalingDecision::None
    }
}

/// Service that coordinates auto-scaling activities.
pub struct AutoScalingService {
    strategy: Arc<dyn AutoScalingStrategy>,
}

impl AutoScalingService {
    pub fn new(strategy: Arc<dyn AutoScalingStrategy>) -> Self {
        Self { strategy }
    }

    /// Evaluates the current context and returns a decision.
    pub fn evaluate(&self, context: &ScalingContext) -> ScalingDecision {
        self.strategy.evaluate(context)
    }

    /// Returns the name of the current strategy.
    pub fn strategy_name(&self) -> &str {
        self.strategy.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::shared_kernel::ProviderId;

    #[test]
    fn test_simple_strategy_scale_up_min() {
        let strategy = SimpleThresholdStrategy::new(2, 10, 1);
        let ctx = ScalingContext::new(ProviderId::new(), 0, 0, 0, 0);
        let decision = strategy.evaluate(&ctx);

        match decision {
            ScalingDecision::ScaleUp { count, .. } => assert_eq!(count, 2),
            _ => panic!("Expected ScaleUp to min workers"),
        }
    }

    #[test]
    fn test_simple_strategy_scale_up_jobs() {
        let strategy = SimpleThresholdStrategy::new(1, 10, 1);
        let ctx = ScalingContext::new(ProviderId::new(), 5, 2, 0, 0); // 5 jobs, 2 available
        let decision = strategy.evaluate(&ctx);

        match decision {
            ScalingDecision::ScaleUp { count, .. } => assert_eq!(count, 3), // 5 - 2 = 3
            _ => panic!("Expected ScaleUp for pending jobs"),
        }
    }

    #[test]
    fn test_simple_strategy_max_limit() {
        let strategy = SimpleThresholdStrategy::new(1, 5, 1);
        let ctx = ScalingContext::new(ProviderId::new(), 10, 1, 4, 0); // 10 jobs, 1 available, 4 busy -> 5 total
        let decision = strategy.evaluate(&ctx);

        assert_eq!(decision, ScalingDecision::None); // Already at max
    }

    #[test]
    fn test_simple_strategy_no_action() {
        let strategy = SimpleThresholdStrategy::new(1, 10, 1);
        let ctx = ScalingContext::new(ProviderId::new(), 1, 2, 0, 0); // 1 job, 2 available
        let decision = strategy.evaluate(&ctx);

        assert_eq!(decision, ScalingDecision::None);
    }
}
