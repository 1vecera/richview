//! Who may read and do what (FEA-BR-25): roles, scopes, audiences and the visibility of one item.
//!
//! Identity comes verified from the adapter; this module only maps it to a role. Publish stores no passwords.
use crate::{
    host::{Config, Principal},
    ledger::{Policy, State},
    model::{Item, Mode, Release, State as ItemState},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum RoleName {
    #[serde(rename = "anonymous")]
    Anonymous,
    #[serde(rename = "reader")]
    Reader,
    #[serde(rename = "editor")]
    Editor,
    #[serde(rename = "super editor")]
    SuperEditor,
    /// The publishing service: deployment APIs only, never an editorial role.
    #[serde(rename = "publisher")]
    Publisher,
}

/// A role with the sections and audiences it covers; `*` covers all.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Access {
    pub name: RoleName,
    #[serde(default)]
    pub sections: Vec<String>,
    #[serde(default)]
    pub audiences: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_login: Option<String>,
    /// How this person is named in the bylines they write; their stories are the ones the desk lets them edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byline: Option<String>,
}
impl Access {
    fn bare(name: RoleName) -> Self {
        Access {
            name,
            sections: vec![],
            audiences: vec![],
            github_login: None,
            byline: None,
        }
    }
    /// A reader who has not signed in, such as a link-preview crawler.
    pub fn anonymous() -> Self {
        Self::bare(RoleName::Anonymous)
    }
    pub fn is_editor(&self) -> bool {
        matches!(self.name, RoleName::Editor | RoleName::SuperEditor)
    }
}

/// The role of `principal`: the publisher, an assigned role, the bootstrap super editor, or a plain reader.
pub fn role(
    principal: Option<&Principal>,
    cfg: &Config,
    roles: &BTreeMap<String, Access>,
) -> Access {
    let Some(p) = principal else {
        return Access::bare(RoleName::Anonymous);
    };
    if !cfg.publisher_identity.is_empty() && p.key == cfg.publisher_identity {
        return Access::bare(RoleName::Publisher);
    }
    if let Some(assigned) = roles.get(&p.key) {
        return assigned.clone();
    }
    if !cfg.bootstrap_super_editor.is_empty() && p.key == cfg.bootstrap_super_editor {
        return Access {
            name: RoleName::SuperEditor,
            sections: vec!["*".into()],
            audiences: vec!["*".into()],
            github_login: cfg.bootstrap_github_login.clone(),
            byline: None,
        };
    }
    // Access admission alone establishes membership, never editorial permission.
    Access {
        name: RoleName::Reader,
        sections: vec![],
        audiences: vec!["members".into()],
        github_login: None,
        byline: None,
    }
}

/// Two keys are one actor when they are equal or map to the same verified GitHub login.
pub fn same_actor(left: &str, right: &str, cfg: &Config, roles: &BTreeMap<String, Access>) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    let login = |key: &str| -> Option<String> {
        match key.strip_prefix("github:") {
            Some(login) => Some(login.to_lowercase()),
            None => role(
                Some(&Principal {
                    key: key.into(),
                    name: String::new(),
                    email: None,
                    service: false,
                }),
                cfg,
                roles,
            )
            .github_login
            .map(|l| l.to_lowercase()),
        }
    };
    matches!((login(left), login(right)), (Some(a), Some(b)) if a == b)
}

pub fn in_scope(access: &Access, section: &str) -> bool {
    access.sections.iter().any(|s| s == "*" || s == section)
}
/// Whether `access` may read an item of this audience; `internal` is the member edition or an internal publication.
pub fn audience(access: &Access, item_audience: &str, internal: bool) -> bool {
    if access.name == RoleName::Anonymous {
        return !internal && item_audience == "public";
    }
    access
        .audiences
        .iter()
        .any(|a| a == "*" || a == item_audience)
        || (!internal && item_audience == "public")
        || (internal
            && item_audience == "public"
            && access.audiences.iter().any(|a| a == "members"))
}
pub fn can_edit(access: &Access, section: &str, item_audience: &str) -> bool {
    access.is_editor() && in_scope(access, section) && audience(access, item_audience, false)
}

/// The policy floor of an item: what the live ledger says about it, whatever an older release shows.
pub fn floor_of(item: &Item) -> Policy {
    Policy {
        id: item.meta.id.clone(),
        section: item.meta.section.clone(),
        state: item.meta.state.clone(),
        audience: item.meta.audience.clone(),
        deleted: false,
    }
}

/// How an item is being requested.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// A protected preview of an unpublished build: editors in scope only.
    pub preview: bool,
    /// A member edition or an internal publication.
    pub internal: bool,
}

/// Whether `access` may see `item` in `release` right now.
///
/// An urgent hide vetoes everything. A preview needs editorial scope. A live read needs a published item that
/// is not private (OPN-54) and an audience the reader holds, in the release and in the ledger's floor, so
/// rollback or an old release can never widen access.
pub fn visible(
    access: &Access,
    item: &Item,
    release: &Release,
    state: &State,
    how: Reading,
) -> bool {
    let internal = how.internal || release.site.mode == Mode::Internal;
    if state.hidden.contains_key(&item.meta.id) || item.meta.kind == crate::model::Kind::Dashboard {
        return false;
    }
    if how.preview {
        return can_edit(access, &item.meta.section, &item.meta.audience)
            && audience(access, &item.meta.audience, internal);
    }
    if item.meta.state != ItemState::Published || !audience(access, &item.meta.audience, internal) {
        return false;
    }
    // An editor's own publish lands private until the editor makes it public (OPN-54).
    if state.private.contains_key(&item.meta.id)
        && !can_edit(access, &item.meta.section, &item.meta.audience)
    {
        return false;
    }
    match state.policies.get(&item.meta.id) {
        Some(floor) => {
            !floor.deleted
                && floor.state == ItemState::Published
                && audience(access, &floor.audience, internal)
        }
        None => true,
    }
}
