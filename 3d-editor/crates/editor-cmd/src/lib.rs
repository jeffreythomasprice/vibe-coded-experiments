mod batch;
mod bounds;
mod commands;
mod ctx;
mod envelope;
mod error;
mod history;
mod session;

pub use bounds::{bounds_json, extent, object_bounds, scene_bounds};
pub use commands::results;
pub use commands::scene::ray_hits;
pub use commands::screenshot::{CameraArg, ColorMode, Mode, Overlay, ScreenshotArgs};
pub use commands::session_args::Attach;
pub use commands::util::descendants;
pub use commands::view::{CameraParams, build_camera, camera_json, resolve_camera};
pub use commands::{Exec, NoArgs, OPS, OpInfo, Safety, parse_args, required_safety};
pub use envelope::{Changes, Request, Response, Success};
pub use error::{CommandError, ErrorCode, ErrorResponse};

/// Typed argument structs of the ops, for clients that send requests in-process.
pub mod args {
    pub use crate::commands::NoArgs;
    pub use crate::commands::brush::Axis;
    pub use crate::commands::brush::CreateBox;
    pub use crate::commands::entity::{SetClassname, SetProperties, SetTransform};
    pub use crate::commands::face::{BrushVertexAt, SetMaterial, TranslateVertices};
    pub use crate::commands::object::{Delete, Rename, SetLayer};
    pub use crate::commands::session_args::{Attach, Save, SelectIds, SelectKind};
    pub use crate::commands::transform::{
        AxisArg, Factors, Flip, Pivot, PivotName, Rotate, Scale, Translate,
    };
    pub use crate::commands::visibility::{Isolate, SetHidden, SetLocked};
}
pub use session::{Actor, Feed, FileState, HistoryState, Session, TxnInfo};
