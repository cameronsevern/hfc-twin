//! Dispatch, repair and proof. A work order ends on what telemetry says, not on what anyone reports.

use schema::common::{ElementId, Origin, ServiceGroupId};
use schema::work_order::{WorkOrder, WorkOrderStatus};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::state::{Twin, TwinError};

/// What was seen when an order was issued, and what came of it. A record of then, so it isn't re-derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// Modems affected at issue.
    pub affected_at_issue: Vec<ElementId>,
    /// Whether the upstream looked wrong at issue.
    pub upstream_degraded_at_issue: bool,
    /// Whether the RPD looked wrong at issue.
    pub rpd_abnormal_at_issue: bool,
    /// How verification came out, once it has.
    pub verdict: Option<Verdict>,
}

/// What verification found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    /// Modems from the evidence that are still affected.
    pub still_affected: Vec<ElementId>,
    /// Whether upstream still looks wrong.
    pub upstream_degraded: bool,
    /// Whether the RPD still looks wrong.
    pub rpd_abnormal: bool,
}

fn order_mut<'a>(
    twin: &'a mut Twin,
    group: &ServiceGroupId,
    id: Uuid,
) -> Result<&'a mut WorkOrder, TwinError> {
    twin.node_mut(group)?
        .work_orders
        .iter_mut()
        .find(|o| o.id == id)
        .ok_or_else(|| TwinError::Refused(format!("no work order {id}")))
}

fn wrong_state(order: &WorkOrder, wanted: &str) -> TwinError {
    TwinError::Refused(format!(
        "work order {} is {:?}, and this needs it {wanted}",
        order.id, order.status
    ))
}

/// Proposed → Dispatched.
pub fn dispatch(twin: &mut Twin, group: &ServiceGroupId, id: Uuid) -> Result<WorkOrder, TwinError> {
    let order = order_mut(twin, group, id)?;
    if order.status != WorkOrderStatus::Proposed {
        return Err(wrong_state(order, "Proposed"));
    }
    order.status = WorkOrderStatus::Dispatched;
    Ok(order.clone())
}

/// Dispatched → Completed, with the simulated repair. A crew fixes what's physically on the component it was sent to, whatever
/// the cause turns out to be. A fault elsewhere stays, however sure the diagnosis was.
pub fn complete(twin: &mut Twin, group: &ServiceGroupId, id: Uuid) -> Result<Value, TwinError> {
    let order = order_mut(twin, group, id)?;
    if order.status != WorkOrderStatus::Dispatched {
        return Err(wrong_state(order, "Dispatched"));
    }
    order.status = WorkOrderStatus::Completed;
    let sent_to = features::sampler::fault_element(&order.parameters.kind);
    let node = twin.node_mut(group)?;
    node.faults
        .retain(|fault| features::sampler::fault_element(&fault.kind) != sent_to);
    Ok(json!({"origin": Origin::Synthetic, "id": id, "status": WorkOrderStatus::Completed}))
}

/// Completed → Verified or NotVerified, from fresh telemetry. A verified order is a label.
pub fn verify(twin: &mut Twin, group: &ServiceGroupId, id: Uuid) -> Result<Value, TwinError> {
    let features = twin.features(group)?;
    let who = twin.who(group, &features)?;
    let node = twin.node_mut(group)?;
    let order = node
        .work_orders
        .iter_mut()
        .find(|o| o.id == id)
        .ok_or_else(|| TwinError::Refused(format!("no work order {id}")))?;
    if order.status != WorkOrderStatus::Completed {
        return Err(wrong_state(order, "Completed"));
    }
    let evidence = node
        .evidence
        .get_mut(&id)
        .ok_or_else(|| TwinError::Refused(format!("no evidence for {id}")))?;
    let still_affected: Vec<ElementId> = evidence
        .affected_at_issue
        .iter()
        .filter(|m| who.affected.contains(m))
        .cloned()
        .collect();
    let upstream = evidence.upstream_degraded_at_issue && who.upstream_degraded;
    let rpd = evidence.rpd_abnormal_at_issue && who.rpd_abnormal;
    let cleared = still_affected.is_empty() && !upstream && !rpd;
    let verdict = Verdict {
        still_affected,
        upstream_degraded: upstream,
        rpd_abnormal: rpd,
    };
    evidence.verdict = Some(verdict.clone());
    order.status = if cleared {
        WorkOrderStatus::Verified
    } else {
        WorkOrderStatus::NotVerified
    };
    Ok(json!({
        "origin": Origin::Synthetic,
        "id": id,
        "status": order.status,
        "affected_at_issue": evidence.affected_at_issue,
        "verdict": verdict,
    }))
}

/// Every telemetry-verified outcome, the primary label source (`docs/DESIGN.md`).
pub fn labels(twin: &Twin) -> Value {
    let mut out = Vec::new();
    for node in twin.nodes.values() {
        for order in &node.work_orders {
            if order.status == WorkOrderStatus::Verified {
                out.push(json!({
                    "origin": Origin::Synthetic,
                    "service_group": order.service_group,
                    "t": order.t,
                    "cause": order.cause,
                    "element_id": order.element_id,
                    "parameters": order.parameters,
                }));
            }
        }
    }
    json!({"labels": out})
}
