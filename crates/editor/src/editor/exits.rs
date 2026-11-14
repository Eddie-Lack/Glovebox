//! The exit triggers of the open level: the sphere the game tests for each platform with a SetExit.

use super::Editor;
use crate::entities::EntId;
use crate::exits::{self, Rule, Zero};
use crate::vec::V3;
use std::sync::Arc;

/// One platform's exit as the game tests it.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitTrigger {
    /// The game's `exitType` (SetExit's `behavior`).
    pub exit_type: i64,
    /// The platform's position. For a platform that moves, where it starts.
    pub center: V3,
    /// The radius of the sphere. None when the editor does not have the model's vertices.
    pub radius: Option<f64>,
    /// Why the radius is 0.
    pub zero: Option<Zero>,
    /// Who must come within the radius, and what else the game asks.
    pub rule: Rule,
}

impl ExitTrigger {
    /// True when the game can never fire this exit: it tests a distance, and no distance is below 0.
    pub fn dead(&self) -> bool {
        self.radius == Some(0.0) && self.rule.who != exits::Who::OnPlatform
    }
}

impl Editor {
    /// The vertices the faces of a model's root mesh use, read once from the back end.
    pub fn root_vertices(&self, oid: &str) -> Option<Arc<Vec<[f32; 3]>>> {
        if let Some(v) = self.exit_verts.borrow().get(oid) {
            return v.clone();
        }
        let v = self.backend.root_vertices(oid).map(Arc::new);
        self.exit_verts.borrow_mut().insert(oid.to_string(), v.clone());
        v
    }

    /// The exit of a platform, or none when the entity is no platform with a SetExit the game tests.
    pub fn exit_trigger(&self, i: EntId) -> Option<ExitTrigger> {
        let e = self.entities.get(i)?;
        if e.kind != "platform" || e.is_handle || e.derived {
            return None;
        }
        let exit_type = exits::exit_type(&self.doc, e.node)?;
        let verts = e.object_id.as_deref().and_then(|o| self.root_vertices(o));
        let (radius, zero) = match (&verts, e.tag.as_str()) {
            (Some(v), _) => exits::radius(&self.doc, e.node, v),
            (None, "NullPlatform") => exits::radius(&self.doc, e.node, &[]),
            (None, _) => (f64::NAN, None),
        };
        Some(ExitTrigger {
            exit_type,
            center: e.world_pos(&self.doc, &self.entities),
            radius: (!radius.is_nan()).then_some(radius),
            zero,
            rule: exits::rule(exit_type, self.scene_index()),
        })
    }

    /// The platforms with an exit, for the layer's count.
    pub fn exits(&self) -> Vec<EntId> {
        (0..self.entities.len())
            .filter(|&i| {
                let e = &self.entities[i];
                e.kind == "platform" && !e.is_handle && !e.derived && exits::exit_type(&self.doc, e.node).is_some()
            })
            .collect()
    }
}
