// SPDX-License-Identifier: GPL-3.0-or-later

use super::TimeCcConfig;
use crate::compat::tdef::osmo_tdef_get;
use crate::tdef::{TDefError, TDefUnit};

/// Effective values for a single accounting or deadline evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AccountingPolicy {
    pub granularity: u64,
    pub threshold: u64,
    pub forget_after: Option<u64>,
}

impl AccountingPolicy {
    pub fn from_config<S>(cfg: &TimeCcConfig<'_, S>) -> Self {
        Self {
            granularity: cfg.granularity(),
            threshold: cfg.threshold(),
            forget_after: (cfg.forget_sum_usec != 0).then_some(cfg.forget_sum_usec),
        }
    }
}

pub(super) struct PolicyResolution {
    pub policy: AccountingPolicy,
    pub discard_pending: bool,
}

impl<S> TimeCcConfig<'_, S> {
    /// Observe bindings in their established order. Successful assignments stay
    /// visible on a later lookup error; a reset is returned only on full success.
    pub(super) fn resolve_policy(&mut self) -> Result<PolicyResolution, TDefError> {
        let before = AccountingPolicy::from_config(self);
        if self.t_defs.is_some() {
            for (binding, value) in [
                (self.t_gran, &mut self.gran_usec),
                (self.t_round_threshold, &mut self.round_threshold_usec),
                (self.t_forget_sum, &mut self.forget_sum_usec),
            ] {
                if binding != 0 {
                    *value = osmo_tdef_get(self.t_defs.as_deref(), binding, TDefUnit::US, -1)?;
                }
            }
        }
        let policy = AccountingPolicy::from_config(self);
        Ok(PolicyResolution {
            discard_pending: before.granularity != policy.granularity
                || (policy.forget_after.is_some() && before.forget_after != policy.forget_after),
            policy,
        })
    }
}
