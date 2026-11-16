//! The ledger: the one strongly consistent record of what is live, what is hidden and who may do what.
//!
//! It holds only access and delivery facts. Content stays in Git (FEA-BR-26) and in immutable releases
//! (FEA-BR-29); the ledger is the veto and the pointer, so it must keep working while Git or a build is down.
use crate::{access::Access, model::State as ItemState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a change stands, from the desk's point of view.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum EditState {
    #[serde(rename = "pending review")]
    PendingReview,
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "building")]
    Building,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "live")]
    Live,
    #[serde(rename = "closed")]
    Closed,
}

/// One Git pull request the desk knows about.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Edit {
    pub number: u64,
    pub url: String,
    pub head: String,
    pub branch: String,
    /// The principal key that made the change, or `github:<login>` for one made in Git.
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_author: Option<String>,
    #[serde(default)]
    pub imported: bool,
    pub state: EditState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Everyone who has put a commit on the change; none of them may approve it.
    #[serde(default)]
    pub contributors: Vec<String>,
    /// The merge commit once merged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// Whether the author held the editor role when the change was observed; decides OPN-54's path.
    #[serde(default)]
    pub author_is_editor: bool,
}

/// An approval of one exact head by someone other than its authors.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Approval {
    pub actor: String,
    pub pull: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// The live commit the approved change was merged onto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// `editor` when the author published their own item and no second approver was needed (OPN-54).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BuildState {
    Building,
    Built,
    Failed,
    Live,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Build {
    pub state: BuildState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// An urgent hide, recorded before Git is touched (FEA-BR-28).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Hide {
    pub revision: String,
    pub actor: String,
    pub reason: String,
    pub at: String,
    /// `pending` until the reconciling pull request exists, then its address.
    pub reconciliation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<u64>,
}

/// The access floor of one item as the newest activation set it; an older release can never widen it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Policy {
    pub id: String,
    pub section: String,
    pub state: ItemState,
    pub audience: String,
    #[serde(default)]
    pub deleted: bool,
}

/// An item an editor published themselves: private until an editor makes it public (OPN-54).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Private {
    pub actor: String,
    pub since: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct State {
    /// Bumped on every commit; the host refuses a commit whose base revision is stale.
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub hidden: BTreeMap<String, Hide>,
    #[serde(default)]
    pub policies: BTreeMap<String, Policy>,
    #[serde(default)]
    pub private: BTreeMap<String, Private>,
    #[serde(default)]
    pub roles: BTreeMap<String, Access>,
    #[serde(default)]
    pub edits: BTreeMap<u64, Edit>,
    /// Keyed by the approved head.
    #[serde(default)]
    pub approvals: BTreeMap<String, Approval>,
    /// Keyed by the merge commit that may be published.
    #[serde(default)]
    pub authorized: BTreeMap<String, Approval>,
    /// Keyed by source commit.
    #[serde(default)]
    pub builds: BTreeMap<String, Build>,
    #[serde(default)]
    pub audit_sequence: u64,
    #[serde(default)]
    pub access_revision: u64,
    /// Releases that were once live, with their commit; only these can be rolled back to or read as history.
    #[serde(default)]
    pub activated: BTreeMap<String, String>,
    /// Runtime file maps by their content id, so an old release keeps its own runtime.
    #[serde(default)]
    pub runtimes: BTreeMap<String, BTreeMap<String, String>>,
}

impl State {
    /// A change observed in Git again at a new head loses its approval (FEA-BR-26).
    pub fn forget_approval(&mut self, head: &str) {
        self.approvals.remove(head);
    }
    /// Changing a person's role invalidates their outstanding approvals, and sends those changes back to review.
    pub fn revoke_approvals_of(&mut self, identity: &str) {
        let revoked: Vec<(String, u64)> = self
            .approvals
            .iter()
            .filter(|(_, a)| a.actor == identity)
            .map(|(head, a)| (head.clone(), a.pull))
            .collect();
        for (head, pull) in revoked {
            self.approvals.remove(&head);
            if let Some(edit) = self.edits.get_mut(&pull)
                && edit.state == EditState::Approved
            {
                edit.state = EditState::PendingReview;
            }
        }
    }
}
